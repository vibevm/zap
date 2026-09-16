use zap_api::{
    CommitReceiptView, PrepareCompositeSuccessorRequest, PreparedCompositeSuccessorView,
    PreparedEffectBundleView, QueryInput, ReconcileRequest, RecordCompositeSuccessorRequest,
    RecordedCompositeSuccessorView, SubmissionStatusView,
};
use zap_core::{CommandPayload, PrincipalContext, ReadAt, StateReader, TransactionStore};
use zap_domain::milestone_planning::{
    CompositePlanCandidateBindingRecord, CompositePlanCandidateRecorded,
    CompositePrecursorEffectBinding, CompositeSuccessorPlanIntent, MilestonePlanProposed,
    MilestonePlanProposedSchema, prepare_composite_successor_plan,
};
use zap_domain::milestones::{MilestoneCreated, MilestoneRevised};
use zap_wire::{
    BasisBinding, BoundedText, CanonicalCommandFrame, CanonicalDecode, CanonicalOutput,
    CanonicalPayload, CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, ErrorCode, EventId,
    EventKind, ProtocolEpoch, ZapError,
};

use super::ApplicationService;
use super::application::service_error;

const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE";

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE");

impl ApplicationService {
    pub fn prepare_composite_successor(
        &self,
        request: PrepareCompositeSuccessorRequest,
    ) -> Result<PreparedCompositeSuccessorView, ZapError> {
        self.validate_composite_identity(&request)?;
        let bindings = validate_precursors(&request)?;
        let request_digest = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &request)?.digest();
        let intent =
            CompositeSuccessorPlanIntent::decode_canonical(&request.plan_intent.canonical()?)?;
        let draft = request.precursors.clone().into_core()?;
        let candidate_revision = request.expected_revision.checked_next()?;
        let (plan, precursor_preparation) = self.service.with_prepared_effect_bundle(
            ReadAt::Revision(request.expected_revision),
            None,
            draft,
            |state, prepared| {
                Ok((
                    prepare_composite_successor_plan(state, &intent, candidate_revision)?,
                    PreparedEffectBundleView::from(prepared),
                ))
            },
        )?;
        let frame =
            self.composite_candidate_frame(&request, request_digest, &intent, &plan, bindings)?;
        Ok(PreparedCompositeSuccessorView {
            request,
            request_digest,
            plan: canonical_input(&plan)?,
            precursor_preparation,
            reconciliation: ReconcileRequest {
                command_id: frame.header().command_id().clone(),
                command_digest: frame.digest(),
            },
        })
    }

    pub fn record_composite_successor(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: RecordCompositeSuccessorRequest,
    ) -> Result<RecordedCompositeSuccessorView, ZapError> {
        let supplied = request.prepared;
        self.validate_composite_store(&supplied.request)?;
        let request_digest =
            CanonicalOutput::encode_json(CodecEpoch::CURRENT, &supplied.request)?.digest();
        if request_digest != supplied.request_digest {
            return Err(composite_error(
                ErrorCode::IdempotencyConflict,
                "composite successor request digest changed",
            ));
        }
        let intent = CompositeSuccessorPlanIntent::decode_canonical(
            &supplied.request.plan_intent.canonical()?,
        )?;
        let plan = zap_domain::milestone_planning::MilestonePlanProposalRecord::decode_canonical(
            &supplied.plan.canonical()?,
        )?;
        let bindings = validate_precursors(&supplied.request)?;
        let frame = self.composite_candidate_frame(
            &supplied.request,
            request_digest,
            &intent,
            &plan,
            bindings,
        )?;
        if supplied.reconciliation.command_id != *frame.header().command_id()
            || supplied.reconciliation.command_digest != frame.digest()
        {
            return Err(composite_error(
                ErrorCode::IdempotencyConflict,
                "composite successor reconciliation identity changed",
            ));
        }
        self.authorize_composite_data(
            credential_id,
            secret,
            supplied.request.expected_revision,
            &plan,
        )?;
        if let Some((digest, receipt)) = self.store.lookup_commit(frame.header().command_id())? {
            if digest != frame.digest() {
                return Err(composite_error(
                    ErrorCode::IdempotencyConflict,
                    "composite successor command identity was reused with different content",
                ));
            }
            return Ok(RecordedCompositeSuccessorView {
                prepared: supplied,
                submission: SubmissionStatusView::Committed {
                    receipt: CommitReceiptView::from(&receipt),
                },
            });
        }
        let prepared = self.prepare_composite_successor(supplied.request.clone())?;
        if prepared != supplied {
            return Err(composite_error(
                ErrorCode::StaleRevision,
                "composite successor preparation changed before recording",
            ));
        }
        let permit = self
            .authorities
            .internal
            .get()
            .ok_or_else(|| service_error(ErrorCode::Unavailable, "internal handle missing"))?
            .authorize(&frame, supplied.request.operation_id.clone())?;
        let submission = self.submit(PrincipalContext::ServiceInternal(&permit), frame)?;
        Ok(RecordedCompositeSuccessorView {
            prepared: supplied,
            submission,
        })
    }

    fn validate_composite_identity(
        &self,
        request: &PrepareCompositeSuccessorRequest,
    ) -> Result<(), ZapError> {
        self.validate_composite_store(request)?;
        let snapshot = self.store.read(ReadAt::Current)?;
        if snapshot.revision() != request.expected_revision {
            return Err(composite_error(
                ErrorCode::StaleRevision,
                "composite successor revision is stale",
            ));
        }
        Ok(())
    }

    fn validate_composite_store(
        &self,
        request: &PrepareCompositeSuccessorRequest,
    ) -> Result<(), ZapError> {
        if &request.store != self.identity() {
            return Err(composite_error(
                ErrorCode::Conflict,
                "composite successor names a foreign store",
            ));
        }
        Ok(())
    }

    fn composite_candidate_frame(
        &self,
        request: &PrepareCompositeSuccessorRequest,
        request_digest: zap_wire::PayloadDigest,
        intent: &CompositeSuccessorPlanIntent,
        plan: &zap_domain::milestone_planning::MilestonePlanProposalRecord,
        precursors: Vec<CompositePrecursorEffectBinding>,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        let operation_digest =
            CanonicalOutput::encode_json(CodecEpoch::CURRENT, &request.operation_id)?.digest();
        let command_id =
            CommandId::parse(&format!("composite-successor.candidate.{operation_digest}"))?;
        let payload = CompositePlanCandidateRecorded {
            operation_id: request.operation_id.clone(),
            request_digest,
            expected_plan_state_revision: intent.expected_plan_state_revision,
            plan: plan.clone(),
            binding: CompositePlanCandidateBindingRecord {
                plan_key: plan.key.clone(),
                operation_id: request.operation_id.clone(),
                request_digest,
                precursors,
                revision: request.expected_revision.checked_next()?,
            },
        };
        CanonicalCommandFrame::new(
            CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: request.store.store_id.clone(),
                campaign_id: request.store.campaign_id.clone(),
                base_id: request.store.base_id.clone(),
                command_id: command_id.clone(),
                event_id: EventId::parse(&format!("event.{}", command_id.as_str()))?,
                expected_revision: request.expected_revision,
                kind: EventKind::parse(CompositePlanCandidateRecorded::KIND)?,
                causes: Vec::new(),
                basis: BasisBinding::NotApplicable,
            })?,
            zap_wire::CommandReason::new(zap_wire::CommandReasonInput {
                summary: BoundedText::parse("Record one projected composite successor candidate")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            CanonicalPayload::encode_json(CodecEpoch::CURRENT, &payload)?,
        )
    }

    fn authorize_composite_data(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        expected_revision: zap_wire::Revision,
        plan: &zap_domain::milestone_planning::MilestonePlanProposalRecord,
    ) -> Result<(), ZapError> {
        if !self.authorities.data_secret.matches(credential_id, secret) {
            return Err(composite_error(
                ErrorCode::Unauthorized,
                "composite successor data authentication failed",
            ));
        }
        let proof = CanonicalCommandFrame::new(
            CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: self.identity().store_id.clone(),
                campaign_id: self.identity().campaign_id.clone(),
                base_id: self.identity().base_id.clone(),
                command_id: CommandId::parse("composite-successor.data-authority-proof")?,
                event_id: EventId::parse("event.composite-successor.data-authority-proof")?,
                expected_revision,
                kind: EventKind::parse(MilestonePlanProposed::KIND)?,
                causes: Vec::new(),
                basis: BasisBinding::Exact(plan.relevant_basis),
            })?,
            zap_wire::CommandReason::new(zap_wire::CommandReasonInput {
                summary: BoundedText::parse("Attest composite successor data authority")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            CanonicalPayload::encode_json(
                CodecEpoch::CURRENT,
                &MilestonePlanProposed {
                    schema: MilestonePlanProposedSchema::V1,
                    plan: plan.clone(),
                },
            )?,
        )?;
        self.authorities
            .data
            .get()
            .ok_or_else(|| service_error(ErrorCode::Unavailable, "data handle missing"))?
            .authorize(&proof)?;
        Ok(())
    }
}

fn validate_precursors(
    request: &PrepareCompositeSuccessorRequest,
) -> Result<Vec<CompositePrecursorEffectBinding>, ZapError> {
    if request.precursors.effects.is_empty()
        || !request.precursors.committed_prefix.is_empty()
        || request.precursors.no_op_basis.is_some()
    {
        return Err(composite_error(
            ErrorCode::InvalidValue,
            "composite successor requires a nonempty uncommitted precursor bundle",
        ));
    }
    let mut bindings = Vec::with_capacity(request.precursors.effects.len());
    for effect in &request.precursors.effects {
        match effect.kind.as_str() {
            "milestone.created" => {
                MilestoneCreated::decode_canonical(&effect.payload.canonical()?)?;
            }
            "milestone.revised" => {
                MilestoneRevised::decode_canonical(&effect.payload.canonical()?)?;
            }
            _ => {
                return Err(composite_error(
                    ErrorCode::Unauthorized,
                    "composite successor precursor is not a milestone create or revise effect",
                ));
            }
        }
        bindings.push(CompositePrecursorEffectBinding {
            effect_id: effect.effect_id.clone(),
            index: effect.index,
            kind: effect.kind.clone(),
            payload_digest: effect.payload.canonical()?.digest(),
            predecessors: effect.predecessors.clone(),
            product_event_id: effect.product_event_id.clone(),
        });
    }
    Ok(bindings)
}

fn canonical_input<T: serde::Serialize>(value: &T) -> Result<QueryInput, ZapError> {
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, value)?;
    Ok(QueryInput {
        codec: payload.codec(),
        canonical_json: payload.as_bytes().to_vec(),
    })
}

fn composite_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        REQUIREMENT,
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}
