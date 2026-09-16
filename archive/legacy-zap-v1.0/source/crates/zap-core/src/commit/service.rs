use super::*;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

/// The official mutation service. R03 exposes no registered production cells.
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#commit-service")]
pub struct CommitService<S: TransactionStore> {
    pub(super) store: S,
    pub(super) identity: StoreIdentity,
    pub(super) reducer_epoch: ReducerEpoch,
    pub(super) query_epoch: QueryEpoch,
    pub(super) cells: CellSet,
    pub(super) queries: QuerySet,
    pub(super) records: RecordSet,
    pub(super) routes: RouteRegistry,
    pub(super) authority: BoundCredentialAuthority,
    pub(super) trust_seal: Arc<()>,
    pub(super) permit: TransactionPermit,
    pub(super) completion: Option<CompletionEvaluator>,
    pub(super) basis: Option<Arc<dyn BasisProvider>>,
    pub(super) action_impact: Option<Arc<dyn crate::ActionImpactProvider>>,
    pub(super) action_admission: Option<Arc<dyn crate::ActionAdmissionProvider>>,
    pub(super) affected_scope: Option<Arc<dyn crate::AffectedScopeProvider>>,
    pub(super) packet_resolution: Option<Arc<dyn crate::PacketResolutionProvider>>,
    pub(super) dispatch_eligibility: Option<Arc<dyn DispatchEligibilityProvider>>,
    pub(super) affected_jobs: Option<Arc<dyn AffectedJobProvider>>,
    pub(super) artifacts: Option<Arc<dyn crate::ArtifactWitnessProvider>>,
}

impl<S: TransactionStore> CommitService<S> {
    pub fn credential_authority(&self) -> &BoundCredentialAuthority {
        &self.authority
    }

    pub fn query_set(&self) -> &QuerySet {
        &self.queries
    }

    pub fn supports_effect_preparation(&self, kind: &zap_wire::EventKind) -> bool {
        self.cells.has_effect_contract(kind)
    }

    pub fn prepare_effect_bundle(
        &self,
        at: ReadAt,
        actor: Option<ActorRef>,
        draft: crate::EffectBundleDraft,
    ) -> Result<crate::PreparedEffectBundle, ZapError> {
        let snapshot = self.store.read(at)?;
        crate::preflight::prepare_effect_bundle(
            &self.cells,
            &self.records,
            &snapshot,
            actor.as_ref(),
            crate::preflight::PreflightProviders {
                basis: self.basis.as_deref(),
                affected_scope: self.affected_scope.as_deref(),
                affected_jobs: self.affected_jobs.as_deref(),
                packet_resolution: self.packet_resolution.as_deref(),
            },
            &draft,
        )
    }

    pub fn with_prepared_effect_bundle<T, F>(
        &self,
        at: ReadAt,
        actor: Option<ActorRef>,
        draft: crate::EffectBundleDraft,
        inspect: F,
    ) -> Result<T, ZapError>
    where
        F: FnOnce(&dyn StateReader, &crate::PreparedEffectBundle) -> Result<T, ZapError>,
    {
        let snapshot = self.store.read(at)?;
        crate::preflight::with_prepared_effect_bundle(
            &self.cells,
            &self.records,
            &snapshot,
            actor.as_ref(),
            crate::preflight::PreflightProviders {
                basis: self.basis.as_deref(),
                affected_scope: self.affected_scope.as_deref(),
                affected_jobs: self.affected_jobs.as_deref(),
                packet_resolution: self.packet_resolution.as_deref(),
            },
            &draft,
            inspect,
        )
    }

    pub fn prepare_effect_comparison(
        &self,
        at: ReadAt,
        actor: Option<ActorRef>,
        draft: crate::EffectComparisonDraft,
    ) -> Result<crate::PreparedEffectComparison, ZapError> {
        let snapshot = self.store.read(at)?;
        crate::preflight::prepare_effect_comparison(
            &self.cells,
            &self.records,
            &snapshot,
            actor.as_ref(),
            crate::preflight::PreflightProviders {
                basis: self.basis.as_deref(),
                affected_scope: self.affected_scope.as_deref(),
                affected_jobs: self.affected_jobs.as_deref(),
                packet_resolution: self.packet_resolution.as_deref(),
            },
            &draft,
        )
    }

    pub fn execute(
        &self,
        principal: PrincipalContext<'_>,
        frame: CanonicalCommandFrame,
    ) -> Result<CommitReceipt, ZapError> {
        let cell = self
            .cells
            .cell(frame.header().kind())
            .ok_or_else(ZapError::unsupported_operation)?;
        let descriptor = cell.descriptor();
        let decoded = cell.decode_payload(frame.payload())?;
        validate_frame_identity(&self.identity, frame.header())?;
        let route = self
            .routes
            .route(frame.header().kind())
            .ok_or_else(unauthorized)?;
        if descriptor.route() != route {
            return Err(transaction_mismatch());
        }
        validate_static_entitlement(&principal, route, &frame, &self.trust_seal)?;
        if let Some((digest, receipt)) = self.store.lookup_commit(frame.header().command_id())? {
            if digest == frame.digest() {
                return Ok(receipt.into_exact_retry());
            }
            return Err(idempotency_conflict());
        }
        let required_artifacts = cell.artifact_digests(decoded.as_ref())?;
        let artifact_guard = if required_artifacts.is_empty() {
            None
        } else {
            let guard = self
                .artifacts
                .as_ref()
                .ok_or_else(missing_artifact_provider)?
                .prepare(&required_artifacts)?;
            if guard.digests() != required_artifacts {
                return Err(missing_artifact_provider());
            }
            Some(guard)
        };
        self.store.transact(&self.permit, |write| {
            let _artifact_guard = artifact_guard;
            if SnapshotRead::identity(write) != self.identity {
                return Err(transaction_mismatch());
            }
            validate_frame_identity(&self.identity, frame.header())?;
            if let Some((digest, receipt)) = write.existing_commit(frame.header().command_id())? {
                if digest == frame.digest() {
                    return Ok(receipt);
                }
                return Err(idempotency_conflict());
            }
            if SnapshotRead::revision(write) != frame.header().expected_revision() {
                return Err(stale_revision_error());
            }
            let basis_request = cell.basis_request(write, decoded.as_ref())?;
            let action_basis = match (&basis_request, frame.header().basis()) {
                (Some(request), BasisBinding::Exact(expected)) => {
                    let provider = self.basis.as_ref().ok_or_else(stale_basis_error)?;
                    provider.validate_scope(write, request, request.roots())?;
                    let observed = provider.relevant_basis(write, request)?;
                    if observed.digest != *expected {
                        return Err(stale_basis_error());
                    }
                    Some(crate::ActionBasis {
                        request: request.clone(),
                        relevant: observed,
                    })
                }
                (Some(_), BasisBinding::NotApplicable) | (None, BasisBinding::Exact(_)) => {
                    return Err(stale_basis_error());
                }
                (None, BasisBinding::NotApplicable) => None,
            };
            let actor = principal_actor(&principal, route, frame.header())?;
            let resolved_command = crate::preflight::resolve_command_preflight(
                &self.cells,
                &self.records,
                write,
                actor.as_ref(),
                crate::preflight::PreflightProviders {
                    basis: self.basis.as_deref(),
                    affected_scope: self.affected_scope.as_deref(),
                    affected_jobs: self.affected_jobs.as_deref(),
                    packet_resolution: self.packet_resolution.as_deref(),
                },
                cell,
                decoded.as_ref(),
                &self.trust_seal,
                frame.header().command_id(),
                frame.digest(),
                frame.header().expected_revision(),
                None,
            )?;
            let mut admission_changes = ChangeSet::new();
            let mut product_changes = ChangeSet::new();
            let mut action_request = None;
            let mut action_needs = None;
            let mut action_resolved = None;
            let mut action_admission_observation = None;
            let authority = match route {
                RouteClass::Privileged(action) => {
                    let actor = actor.clone().ok_or_else(unauthorized)?;
                    let impact_request = cell
                        .action_impact_request(decoded.as_ref())?
                        .ok_or_else(unauthorized)?;
                    let impact_provider = self.action_impact.as_ref().ok_or_else(unauthorized)?;
                    let impact_context = crate::ActionImpactContext {
                        action,
                        kind: frame.header().kind(),
                        event_id: frame.header().event_id(),
                        payload_digest: frame.payload().digest(),
                        relevant_basis: action_basis.as_ref().map(|basis| &basis.relevant),
                    };
                    let impact =
                        impact_provider.classify(write, &impact_context, &impact_request)?;
                    validate_action_impact(write, &impact_context, &impact_request, &impact)?;
                    let request = crate::ActionAdmissionRequest {
                        action: action.clone(),
                        header: frame.header().clone(),
                        command_digest: frame.digest(),
                        payload_digest: frame.payload().digest(),
                        basis: action_basis.clone(),
                        impact,
                        impact_request,
                    };
                    let provider = self.action_admission.as_ref().ok_or_else(unauthorized)?;
                    let needs = provider.needs(write, &actor, &request)?;
                    validate_action_needs(&request, &needs, &frame)?;
                    let resolved = crate::preflight::resolve_action_needs(
                        &self.cells,
                        &self.records,
                        write,
                        &actor,
                        crate::preflight::PreflightProviders {
                            basis: self.basis.as_deref(),
                            affected_scope: self.affected_scope.as_deref(),
                            affected_jobs: self.affected_jobs.as_deref(),
                            packet_resolution: self.packet_resolution.as_deref(),
                        },
                        &needs,
                        &resolved_command.transaction_seal,
                        &self.trust_seal,
                    )?;
                    validate_selected_effect_coverage(&request, &needs, &resolved.record)?;
                    let preflight = crate::ActionAdmissionPreflight::new(
                        &resolved.record,
                        &resolved.independence,
                        &resolved.safe_jobs,
                        &resolved.transaction_seal,
                        &self.trust_seal,
                    );
                    let observation = provider.admit(write, &actor, &request, &preflight)?;
                    validate_action_admission(
                        provider.descriptor(),
                        &request,
                        &preflight,
                        &observation,
                    )?;
                    provider.apply(
                        write,
                        &request,
                        &observation,
                        &preflight,
                        &mut admission_changes,
                    )?;
                    let authority = AdmittedAuthority::privileged(
                        actor,
                        action.clone(),
                        observation.basis.clone(),
                    );
                    action_request = Some(request);
                    action_needs = Some(needs);
                    action_resolved = Some(resolved);
                    action_admission_observation = Some(observation);
                    authority
                }
                _ => admit_non_privileged(&principal, route, &frame, &self.trust_seal)?,
            };
            let completion = if descriptor.requires_completion() {
                Some(
                    self.completion
                        .as_ref()
                        .ok_or_else(missing_completion_provider)?
                        .view(write)?,
                )
            } else {
                None
            };
            if resolved_command.packet_resolution.is_some()
                != descriptor.requires_packet_resolution()
            {
                return Err(missing_dispatch_eligibility_provider());
            }
            let owned_dispatch_request = if resolved_command.packet_resolution.is_none()
                && descriptor.requires_dispatch_eligibility()
            {
                Some(
                    cell.dispatch_eligibility_request(decoded.as_ref())?
                        .ok_or_else(missing_dispatch_eligibility_provider)?,
                )
            } else {
                None
            };
            let dispatch_request = resolved_command
                .packet_resolution
                .as_ref()
                .map(|claim| &claim.record().eligibility)
                .or(owned_dispatch_request.as_ref());
            let dispatch_eligibility = if let Some(request) = dispatch_request {
                let view = self
                    .dispatch_eligibility
                    .as_ref()
                    .ok_or_else(missing_dispatch_eligibility_provider)?
                    .evaluate(write, request)?;
                if view.request_digest != request.digest
                    || view.observed_revision != SnapshotRead::revision(write)
                    || !view.eligible
                {
                    return Err(dispatch_held());
                }
                Some(view)
            } else {
                None
            };
            let affected_jobs = if descriptor.requires_affected_jobs() {
                let request = cell
                    .affected_job_request(decoded.as_ref())?
                    .ok_or_else(missing_affected_job_provider)?;
                let view = self
                    .affected_jobs
                    .as_ref()
                    .ok_or_else(missing_affected_job_provider)?
                    .evaluate(write, &request)?;
                if view.request_digest != request.digest
                    || view.observed_revision != SnapshotRead::revision(write)
                    || view.completeness != AffectedJobCompleteness::Complete
                {
                    return Err(affected_jobs_unknown());
                }
                Some(view)
            } else {
                None
            };
            let action_preflight_record = action_resolved
                .as_ref()
                .map(|resolved| resolved.record.clone());
            let command_preflight_record = resolved_command.record.clone();
            let validated_preflight = Arc::new(crate::ValidatedCommandPreflight::new(
                resolved_command.record,
                resolved_command.safe_jobs,
                resolved_command.transaction_seal,
                self.trust_seal.clone(),
                action_preflight_record.clone(),
                resolved_command.packet_resolution,
            ));
            let header = crate::ValidatedHeader::new(
                frame.header().clone(),
                frame.digest(),
                authority.clone(),
                completion.clone(),
                dispatch_eligibility.clone(),
                affected_jobs.clone(),
                validated_preflight,
            );
            let output = cell.validate_apply_decoded(
                write,
                &header,
                frame.reason(),
                decoded,
                &mut product_changes,
            )?;
            let product_mutations = prepare_scoped_mutations(
                &product_changes,
                &self.records,
                descriptor.affected_records(),
            )?;
            let product_mutation_digest = crate::change::effect_mutation_digest(
                &product_changes,
                &self.records,
                descriptor.affected_records(),
            )?;
            let action_outcome = match (
                action_request.as_ref(),
                action_needs.as_ref(),
                action_resolved.as_ref(),
                action_admission_observation.as_ref(),
            ) {
                (Some(request), Some(needs), Some(resolved), Some(observation)) => {
                    let selected = resolved.record.selected_effect.as_ref();
                    let relevant_after = if request.impact.class
                        == crate::ActionImpactClass::SemanticChange
                    {
                        let effect_request = needs
                            .selected_effect()
                            .and_then(|bundle| bundle.effects().first())
                            .ok_or_else(transaction_mismatch)?;
                        let expected = selected
                            .and_then(|bundle| bundle.effects.first())
                            .ok_or_else(transaction_mismatch)?;
                        if expected.mutation_digest != product_mutation_digest {
                            return Err(transaction_mismatch());
                        }
                        let effect_payload = cell.decode_payload(frame.payload())?;
                        let context = crate::EffectScopeContext::new(
                            StateReader::identity(write),
                            actor.clone(),
                            effect_request.effect_id().clone(),
                            effect_request.product_event_id().clone(),
                            StateReader::revision(write),
                        );
                        let scope = cell
                            .effect_scope(write, &context, effect_payload.as_ref())?
                            .ok_or_else(transaction_mismatch)?;
                        let overlay = crate::change::ChangeSetOverlay::new(
                            write,
                            &product_changes,
                            &self.records,
                            descriptor.affected_records(),
                        )?;
                        let basis_provider = self.basis.as_ref().ok_or_else(stale_basis_error)?;
                        basis_provider.validate_scope(
                            &overlay,
                            scope.basis(),
                            scope.basis().roots(),
                        )?;
                        let after = basis_provider.relevant_basis(&overlay, scope.basis())?;
                        if after.digest != expected.relevant_after {
                            return Err(transaction_mismatch());
                        }
                        Some(after.digest)
                    } else {
                        None
                    };
                    let outcome = crate::ActionProductOutcome {
                        selected_effect: selected.map(|bundle| bundle.digest),
                        mutation_digest: product_mutation_digest,
                        relevant_after,
                    };
                    let preflight = crate::ActionAdmissionPreflight::new(
                        &resolved.record,
                        &resolved.independence,
                        &resolved.safe_jobs,
                        &resolved.transaction_seal,
                        &self.trust_seal,
                    );
                    self.action_admission
                        .as_ref()
                        .ok_or_else(unauthorized)?
                        .verify_after(write, request, observation, &preflight, &outcome)?;
                    Some(outcome)
                }
                (None, None, None, None) => None,
                _ => return Err(transaction_mismatch()),
            };
            let admission_descriptor = action_admission_observation
                .as_ref()
                .and(self.action_admission.as_deref())
                .map(crate::ActionAdmissionProvider::descriptor);
            let admission_mutations = match admission_descriptor {
                Some(hook) => prepare_scoped_mutations(
                    &admission_changes,
                    &self.records,
                    hook.scope_for(
                        &action_admission_observation
                            .as_ref()
                            .ok_or_else(transaction_mismatch)?
                            .basis,
                    )
                    .affected_records(),
                )?,
                None => Vec::new(),
            };
            let admission_indexes = match admission_descriptor {
                Some(hook) => prepare_index_rows(
                    write,
                    &self.records,
                    hook.scope_for(
                        &action_admission_observation
                            .as_ref()
                            .ok_or_else(transaction_mismatch)?
                            .basis,
                    )
                    .affected_indexes(),
                    &admission_mutations,
                )?,
                None => Vec::new(),
            };
            let product_indexes = prepare_index_rows(
                write,
                &self.records,
                descriptor.affected_indexes(),
                &product_mutations,
            )?;
            let mutations = combine_mutation_batches([admission_mutations, product_mutations])?;
            let index_rows = combine_index_batches([admission_indexes, product_indexes])?;
            let revision = SnapshotRead::revision(write).checked_next()?;
            let transaction_id =
                TransactionId::parse(&format!("tx:{}", frame.header().command_id()))?;
            let previous_event_digest = write.head_event_digest()?;
            let logical_event = LogicalEventV2 {
                schema_version: LOGICAL_EVENT_SCHEMA_2,
                store: self.identity.clone(),
                revision,
                previous_revision: SnapshotRead::revision(write),
                header: frame.header().clone(),
                reason: frame.reason().clone(),
                transaction_id: transaction_id.clone(),
                command_digest: frame.digest(),
                reducer_epoch: self.reducer_epoch,
                query_epoch: self.query_epoch,
                authority: authority.clone(),
                command_preflight: command_preflight_record,
                action_admission: action_admission_observation,
                action_preflight: action_preflight_record,
                action_outcome,
                completion,
                dispatch_eligibility,
                affected_jobs,
                artifacts: required_artifacts,
                payload: frame.payload().as_bytes().to_vec(),
                output: output.as_bytes().to_vec(),
                previous_event_digest,
            };
            let event = CanonicalOutput::encode_json(self.identity.codec_epoch, &logical_event)?;
            let event_digest = EventDigest::hash(event.as_bytes());
            let receipt = CommitReceipt::from_validated_parts(CommitReceiptParts {
                store: self.identity.clone(),
                command_id: frame.header().command_id().clone(),
                event_id: frame.header().event_id().clone(),
                transaction_id,
                revision,
                event_digest,
                output,
                disposition: CommitDisposition::Committed,
            })?;
            let intent = ValidatedCommitIntent::new(
                write.binding(),
                receipt,
                frame.digest(),
                event.as_bytes().to_vec(),
                mutations,
                index_rows,
            );
            write.apply_commit(&intent)
        })
    }

    pub fn reconcile(&self, command: &CommandId) -> Result<CommitStatus, ZapError> {
        self.store.transact(&self.permit, |write| {
            if SnapshotRead::identity(write) != self.identity {
                return Err(transaction_mismatch());
            }
            match write.existing_commit(command)? {
                Some((_, receipt)) => Ok(CommitStatus::Committed(receipt.into_reconciled())),
                None => Ok(CommitStatus::NotCommitted {
                    command_id: command.clone(),
                }),
            }
        })
    }
}

impl<S: TransactionStore> CommandPort for CommitService<S> {
    fn submit(
        &self,
        principal: PrincipalContext<'_>,
        command: CanonicalCommandFrame,
    ) -> Result<CommitReceipt, ZapError> {
        self.execute(principal, command)
    }

    fn reconcile(&self, command: &CommandId) -> Result<CommitStatus, ZapError> {
        self.reconcile(command)
    }
}
