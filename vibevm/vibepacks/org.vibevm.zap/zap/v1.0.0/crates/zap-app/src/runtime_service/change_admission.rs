specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION");

use serde::Serialize;
use zap_api::{
    ChangeAdmissionAdvanceRequest, ChangeAdmissionAdvanceView, CommitReceiptView,
    OwnerDecisionContextView,
};
use zap_core::{
    CommandPayload, CommitReceipt, CredentialAuthority, PrincipalContext, ReadAt, SecretInput,
    SnapshotRead, StateReaderExt, TransactionStore,
};
use zap_domain::economics::{
    AdmissionDisposition, ChangeAdmissionPrepared, ChangeAdmissionRecord,
    ChangeAssessmentAdjudicated, ChangeAssessmentRecord, assessment_digest, forecast_digest,
};
use zap_wire::{
    BasisBinding, BoundedText, CanonicalCommandFrame, CanonicalPayload, CodecEpoch, CommandHeader,
    CommandHeaderInput, CommandId, CommandReason, CommandReasonInput, EventId, HoldId,
    ProtocolEpoch, Revision, ZapError,
};

use super::ApplicationService;

mod validation;
use validation::{
    latest_forecast, validate_persisted_request, validate_product_identity,
    validate_selected_action_scope,
};

impl ApplicationService {
    #[specmark::spec(
        implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION"
    )]
    pub fn advance_change_admission(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: ChangeAdmissionAdvanceRequest,
    ) -> Result<ChangeAdmissionAdvanceView, ZapError> {
        let identity = self.identity();
        if request.store != *identity {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleBasis,
                "change admission request names a foreign store",
            ));
        }
        let principal = self.service.credential_authority().authenticate(
            credential_id,
            SecretInput::new(secret),
            &identity.campaign_id,
        )?;
        if !principal.allows_action(&request.action) {
            return Err(orchestration_error(
                zap_wire::ErrorCode::Unauthorized,
                "credential is not configured for the requested planning action",
            ));
        }
        if request.action != zap_wire::ActionClass::parse("plan.lower")? {
            return Err(orchestration_error(
                zap_wire::ErrorCode::UnsupportedOperation,
                "change admission orchestration supports only registered planning effects",
            ));
        }
        validate_selected_action_scope(&request)?;

        let adjudication_id = stage_command_id(&request, "adjudicate")?;
        let admission_id = stage_command_id(&request, "admission")?;
        if let (Some(adjudication), Some(admission)) = (
            self.committed(&adjudication_id)?,
            self.committed(&admission_id)?,
        ) {
            return Ok(ChangeAdmissionAdvanceView::Ready {
                operation_id: request.operation_id,
                assessment_id: request.assessment_id,
                alternative_id: request.alternative_id,
                observed_revision: self.store.head()?,
                adjudication: CommitReceiptView::from(&adjudication),
                admission: CommitReceiptView::from(&admission),
            });
        }
        let snapshot = self.store.read(ReadAt::Current)?;
        validate_persisted_request(&request, &snapshot)?;
        drop(snapshot);
        let adjudication = if let Some(receipt) = self.committed(&adjudication_id)? {
            receipt
        } else {
            self.adjudicate_change(&request, adjudication_id)?
        };
        let snapshot = self.store.read(ReadAt::Current)?;
        let assessment = snapshot
            .get_typed::<ChangeAssessmentRecord>(&request.assessment_id)?
            .ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::MissingReference,
                    "change assessment is missing after adjudication",
                )
            })?;
        let current_assessment_digest = assessment_digest(&assessment)?;
        let exact_initial_retry = request.decision_id.is_none()
            && request.assessment_digest == request.source_assessment_digest;
        if assessment.recommended_alternative_id.as_ref() != Some(&request.alternative_id)
            || (request.assessment_digest != current_assessment_digest && !exact_initial_retry)
        {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleBasis,
                "adjudicated assessment no longer matches the requested alternative",
            ));
        }
        validate_product_identity(&request, &assessment)?;
        if assessment.admission == AdmissionDisposition::OwnerDecisionRequired
            && request.decision_id.is_none()
        {
            let hold_id = assessment.hold_id.clone().ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::InternalInvariant,
                    "Owner-required adjudication did not create its hold",
                )
            })?;
            let selected = assessment
                .alternatives
                .iter()
                .find(|row| row.alternative_id == request.alternative_id)
                .ok_or_else(|| {
                    orchestration_error(
                        zap_wire::ErrorCode::Conflict,
                        "selected alternative is missing",
                    )
                })?;
            let effect_fingerprints = selected
                .effects
                .iter()
                .map(zap_domain::economics::ChangeEffect::fingerprint)
                .collect::<Result<Vec<_>, ZapError>>()?;
            let effect_preflight_digests = selected
                .effects
                .iter()
                .map(|effect| effect.preflight_digest)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    orchestration_error(
                        zap_wire::ErrorCode::InternalInvariant,
                        "adjudicated effect preflight is missing",
                    )
                })?;
            let latest_forecast = latest_forecast(&snapshot, &assessment.assessment_id)?;
            let forecast_id = latest_forecast
                .as_ref()
                .map(|forecast| forecast.forecast_id.clone());
            let forecast_digest = latest_forecast.as_ref().map(forecast_digest).transpose()?;
            let decision = OwnerDecisionContextView {
                assessment_digest: current_assessment_digest,
                forecast_id,
                forecast_digest,
                policy_id: assessment.policy_id.clone(),
                policy_revision: assessment.policy_revision,
                recommended_alternative_id: request.alternative_id.clone(),
                effect_fingerprints,
                effect_preflight_digests,
                decision_revision: snapshot.revision().checked_next()?,
            };
            return Ok(ChangeAdmissionAdvanceView::OwnerDecisionRequired {
                operation_id: request.operation_id,
                assessment_id: request.assessment_id,
                alternative_id: request.alternative_id,
                observed_revision: snapshot.revision(),
                hold_id,
                assessment_digest: current_assessment_digest,
                decision,
                adjudication: CommitReceiptView::from(&adjudication),
            });
        }
        drop(snapshot);

        let admission = if let Some(receipt) = self.committed(&admission_id)? {
            receipt
        } else {
            self.prepare_selected_admission(&request, admission_id)?
        };
        Ok(ChangeAdmissionAdvanceView::Ready {
            operation_id: request.operation_id,
            assessment_id: request.assessment_id,
            alternative_id: request.alternative_id,
            observed_revision: self.store.head()?,
            adjudication: CommitReceiptView::from(&adjudication),
            admission: CommitReceiptView::from(&admission),
        })
    }

    fn adjudicate_change(
        &self,
        request: &ChangeAdmissionAdvanceRequest,
        command_id: CommandId,
    ) -> Result<CommitReceipt, ZapError> {
        let snapshot = self.store.read(ReadAt::Current)?;
        if snapshot.revision() != request.expected_revision {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleRevision,
                "assessment adjudication does not begin at the expected revision",
            ));
        }
        let assessment = snapshot
            .get_typed::<ChangeAssessmentRecord>(&request.assessment_id)?
            .ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::MissingReference,
                    "proposed change assessment is missing",
                )
            })?;
        if assessment.adjudicated
            || assessment_digest(&assessment)? != request.source_assessment_digest
            || request.assessment_digest != request.source_assessment_digest
            || assessment.comparison_basis_digest != request.relevant_basis
            || request.comparison.draft.assessment_id != request.assessment_id
        {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleBasis,
                "assessment or comparison basis is stale",
            ));
        }
        drop(snapshot);
        let prepared = self.service.prepare_effect_comparison(
            ReadAt::Current,
            request.comparison.actor.clone(),
            request.comparison.draft.clone().into_core()?,
        )?;
        if prepared.store() != &request.store
            || prepared.observed_revision() != request.expected_revision
            || prepared.relevant_basis() != request.relevant_basis
            || !prepared
                .alternatives()
                .iter()
                .any(|row| row.request().alternative_id() == &request.alternative_id)
        {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleBasis,
                "prepared comparison differs from the requested assessment boundary",
            ));
        }
        let mut jobs = prepared
            .alternatives()
            .iter()
            .flat_map(|bundle| {
                (0..bundle.request().effects().len())
                    .filter_map(|index| bundle.affected_scope(index))
                    .flat_map(|scope| scope.jobs.jobs.iter().map(|job| job.job_id.clone()))
            })
            .collect::<Vec<_>>();
        jobs.sort();
        jobs.dedup();
        let hold_id = (assessment.admission == AdmissionDisposition::OwnerDecisionRequired)
            .then(|| HoldId::parse(&format!("hold.{}", request.operation_id.as_str())))
            .transpose()?;
        let payload = ChangeAssessmentAdjudicated {
            assessment_id: request.assessment_id.clone(),
            hold_id,
            drain_job_ids: jobs,
            independence_basis: request.relevant_basis,
            independent_effect_fingerprints: Vec::new(),
        };
        self.commit_internal(
            &request.operation_id,
            command_id,
            request.expected_revision,
            BasisBinding::Exact(request.relevant_basis),
            &payload,
        )
    }

    fn prepare_selected_admission(
        &self,
        request: &ChangeAdmissionAdvanceRequest,
        command_id: CommandId,
    ) -> Result<CommitReceipt, ZapError> {
        let snapshot = self.store.read(ReadAt::Current)?;
        let assessment = snapshot
            .get_typed::<ChangeAssessmentRecord>(&request.assessment_id)?
            .ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::MissingReference,
                    "assessment is missing",
                )
            })?;
        let alternative = assessment
            .alternatives
            .iter()
            .find(|row| row.alternative_id == request.alternative_id)
            .ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::Conflict,
                    "selected alternative is missing",
                )
            })?;
        let current = snapshot.get_typed::<ChangeAdmissionRecord>(&assessment.change_id)?;
        let applied_effect_ids = current
            .as_ref()
            .map_or_else(Vec::new, |row| row.applied_effect_ids.clone());
        let effect_index = applied_effect_ids.len();
        let effect = alternative.effects.get(effect_index).ok_or_else(|| {
            orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "selected alternative has no next effect",
            )
        })?;
        let prepared = self.service.prepare_effect_comparison(
            ReadAt::Current,
            request.comparison.actor.clone(),
            request.comparison.draft.clone().into_core()?,
        )?;
        let bundle = prepared
            .alternatives()
            .iter()
            .find(|row| row.request().alternative_id() == &request.alternative_id)
            .ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::Conflict,
                    "prepared alternative is missing",
                )
            })?;
        if bundle.request().committed_prefix() != applied_effect_ids {
            return Err(orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "prepared effect prefix differs from the applied admission prefix",
            ));
        }
        let item = bundle
            .view()
            .effects
            .iter()
            .find(|item| item.effect_id == effect.effect_id)
            .ok_or_else(|| {
                orchestration_error(zap_wire::ErrorCode::Conflict, "prepared effect is missing")
            })?;
        let product = request.product.canonical()?;
        let latest_forecast = latest_forecast(&snapshot, &assessment.assessment_id)?;
        let forecast_id = latest_forecast
            .as_ref()
            .map(|forecast| forecast.forecast_id.clone());
        let forecast_digest = latest_forecast.as_ref().map(forecast_digest).transpose()?;
        let routes = zap_domain::route_set()?;
        let registered_action = match routes.route(&effect.kind) {
            Some(zap_wire::RouteClass::Privileged(action)) => action,
            _ => {
                return Err(orchestration_error(
                    zap_wire::ErrorCode::UnsupportedOperation,
                    "selected effect is not a registered privileged planning operation",
                ));
            }
        };
        if registered_action != &request.action
            || request.action != zap_wire::ActionClass::parse("plan.lower")?
        {
            return Err(orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "selected effect action differs from the authorized planning action",
            ));
        }
        if product.header().store_id() != &request.store.store_id
            || product.header().campaign_id() != &request.store.campaign_id
            || product.header().base_id() != &request.store.base_id
        {
            return Err(orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "product command names a foreign store",
            ));
        }
        if product.header().kind() != &effect.kind
            || product.header().event_id() != &effect.product_event_id
            || product.payload().digest() != effect.payload_digest
        {
            return Err(orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "product command differs from the selected prepared effect",
            ));
        }
        if product.header().basis() != &BasisBinding::Exact(effect.relevant_before) {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleBasis,
                "product command does not bind the selected effect's local basis",
            ));
        }
        let expected_product_revision = snapshot.revision().checked_next()?;
        if product.header().expected_revision() != expected_product_revision {
            return Err(orchestration_error(
                zap_wire::ErrorCode::StaleRevision,
                "product command does not follow the admission commit revision",
            ));
        }
        let impact_digest = self.service.prepare_action_impact(&product)?.digest;
        let admission = ChangeAdmissionRecord {
            change_id: assessment.change_id.clone(),
            assessment_id: assessment.assessment_id.clone(),
            assessment_digest: assessment_digest(&assessment)?,
            forecast_id,
            forecast_digest,
            decision_id: request.decision_id.clone(),
            alternative_id: request.alternative_id.clone(),
            effect_id: effect.effect_id.clone(),
            effect_index: effect.index,
            effect_fingerprint: effect.fingerprint()?,
            relevant_before: effect.relevant_before,
            action: request.action.clone(),
            command_id: product.header().command_id().clone(),
            impact_digest,
            effect_item_digest: item.stable_digest,
            effect_preflight_digest: bundle.view().digest,
            payload_digest: product.payload().digest(),
            product_event_id: product.header().event_id().clone(),
            exception_id: request.exception_id.clone(),
            hold_id: assessment.hold_id.clone(),
            final_effect: effect_index + 1 == alternative.effects.len(),
            applied_effect_ids,
            applied: false,
            revision: Revision::GENESIS,
        };
        let expected_revision = snapshot.revision();
        drop(snapshot);
        self.commit_internal(
            &request.operation_id,
            command_id,
            expected_revision,
            BasisBinding::NotApplicable,
            &ChangeAdmissionPrepared { admission },
        )
    }

    fn committed(&self, command_id: &CommandId) -> Result<Option<CommitReceipt>, ZapError> {
        Ok(self
            .store
            .lookup_commit(command_id)?
            .map(|(_, receipt)| receipt))
    }

    fn commit_internal<P: CommandPayload + Serialize>(
        &self,
        operation_id: &zap_wire::OperationId,
        command_id: CommandId,
        expected_revision: Revision,
        basis: BasisBinding,
        payload: &P,
    ) -> Result<CommitReceipt, ZapError> {
        let event_id = EventId::parse(&format!("event.{}", command_id.as_str()))?;
        let frame = CanonicalCommandFrame::new(
            CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: self.identity().store_id.clone(),
                campaign_id: self.identity().campaign_id.clone(),
                base_id: self.identity().base_id.clone(),
                command_id,
                event_id,
                expected_revision,
                kind: zap_wire::EventKind::parse(P::KIND)?,
                causes: Vec::new(),
                basis,
            })?,
            CommandReason::new(CommandReasonInput {
                summary: BoundedText::parse("Advance one selected change admission")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
        )?;
        if let Some((digest, receipt)) = self.store.lookup_commit(frame.header().command_id())? {
            return if digest == frame.digest() {
                Ok(receipt)
            } else {
                Err(orchestration_error(
                    zap_wire::ErrorCode::IdempotencyConflict,
                    "stable internal command identity was reused with different content",
                ))
            };
        }
        let permit = self
            .authorities
            .internal
            .get()
            .ok_or_else(|| {
                orchestration_error(
                    zap_wire::ErrorCode::Unavailable,
                    "internal authority is unavailable",
                )
            })?
            .authorize(&frame, operation_id.clone())?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)
    }
}

fn stage_command_id(
    request: &ChangeAdmissionAdvanceRequest,
    stage: &str,
) -> Result<CommandId, ZapError> {
    let identity = if stage == "admission" {
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, request)?
            .digest()
            .to_string()
    } else {
        CanonicalPayload::encode_json(
            CodecEpoch::CURRENT,
            &(
                &request.operation_id,
                &request.assessment_id,
                &request.alternative_id,
                request.source_assessment_digest,
            ),
        )?
        .digest()
        .to_string()
    };
    CommandId::parse(&format!("change-admission.{stage}.{identity}"))
}

fn orchestration_error(code: zap_wire::ErrorCode, why: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION",
        why,
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
