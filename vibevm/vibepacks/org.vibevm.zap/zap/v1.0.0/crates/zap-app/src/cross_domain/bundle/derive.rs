use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
);

impl ApplicationBundleClosureProvider {
    pub(super) fn derive(
        &self,
        state: &dyn StateReader,
        request: &BundleClosureRequest,
        captured: Option<&BundleClosureRecord>,
        semantic_entries: Option<
            &BTreeMap<(BundleEntryKind, BoundedText<4096>), BundleArtifactCapture>,
        >,
        verify_physical: bool,
    ) -> Result<BundleClosureRecord, ZapError> {
        let rebuilt = BundleClosureRequest::new(
            request.bundle_id.clone(),
            request.lowering_id.clone(),
            request.packet_ids.clone(),
            request.maximum_archive_bytes,
            request.maximum_encounters,
            request.maximum_return_bytes,
            request.simulated,
        )?;
        if &rebuilt != request {
            return Err(bundle_conflict("bundle request digest is invalid"));
        }
        let lowering = state
            .get_typed::<LoweringRecord>(&request.lowering_id)?
            .ok_or_else(|| bundle_missing("bundle lowering is missing"))?;
        let strategy = state
            .get_typed::<StrategicPlanRecord>(&lowering.strategic_revision_id)?
            .ok_or_else(|| bundle_missing("bundle strategy is missing"))?;
        if lowering.state != PlanningRevisionState::Current
            || strategy.state != PlanningRevisionState::Current
        {
            return Err(bundle_conflict(
                "bundle strategy or lowering is not current",
            ));
        }
        let all_packets = scan_all::<WorkerPacketRecord>(state)?;
        let jobs = scan_all::<RuntimeJobRecord>(state)?;
        let mut packet_bindings = Vec::new();
        let mut attempt_bindings = Vec::new();
        let mut sources = BTreeMap::new();
        let mut rules = BTreeMap::new();
        let mut forks = BTreeMap::new();
        let mut material_entries = BTreeMap::new();
        let mut semantic_entry_bindings = Vec::new();
        for packet_id in &request.packet_ids {
            let packet = all_packets
                .iter()
                .find(|row| &row.packet_id == packet_id)
                .ok_or_else(|| bundle_missing("selected packet is missing"))?;
            if packet.state != PacketState::Current
                || packet.strategy_id != strategy.strategic_revision_id
                || packet.strategy_revision != strategy.revision
                || packet.strategy_semantic_digest != strategy.semantic_digest
                || packet.lowering_id != lowering.lowering_id
                || packet.lowering_revision != lowering.revision
                || packet.lowering_semantic_digest != lowering.semantic_digest
            {
                return Err(bundle_conflict(
                    "selected packet is not current for the lowering",
                ));
            }
            let matching_jobs = jobs
                .iter()
                .filter(|job| &job.packet_id == packet_id)
                .collect::<Vec<_>>();
            let [job] = matching_jobs.as_slice() else {
                return Err(bundle_conflict(
                    "packet must have exactly one actual runtime attempt",
                ));
            };
            if job.execution != ExecutionState::DispatchPending || job.receipt.is_some() {
                return Err(bundle_conflict(
                    "offline bundle requires one claimed unlaunched job",
                ));
            }
            if job.work_id != packet.work_id || job.packet_digest != packet.packet_digest {
                return Err(bundle_conflict(
                    "runtime attempt does not match the exact current packet identity",
                ));
            }
            if job.contract_id != packet.contract_id
                || job.contract_version.get() != packet.contract_version.get()
                || job.contract_digest != packet.contract_digest
                || job.validation_generation.get() != packet.validation_generation
            {
                return Err(bundle_conflict(
                    "runtime attempt does not match the exact current packet contract",
                ));
            }
            let packet_path = entry_path(BundleEntryKind::Packet, packet.packet_id.as_str())?;
            let captured_packet = semantic_entries
                .map(|entries| {
                    semantic_body::<PortablePacketBody>(
                        entries,
                        BundleEntryKind::Packet,
                        &packet_path,
                    )
                })
                .transpose()?;
            if captured_packet
                .as_ref()
                .is_some_and(|body| body.packet != *packet)
            {
                return Err(bundle_conflict(
                    "captured packet body differs from the current packet record",
                ));
            }
            let resolution = self.packet_materials(
                state,
                packet,
                job,
                captured_packet.as_ref().map(|body| &body.claim),
                captured,
                verify_physical,
            )?;
            if resolution.digest != job.packet_resolution_digest
                || resolution.workspace_artifact != job.workspace_manifest
            {
                return Err(bundle_conflict(
                    "runtime packet resolution changed before export",
                ));
            }
            let contract = state
                .get_typed::<TaskContractRecord>(&job.contract_id)?
                .ok_or_else(|| bundle_missing("bundle assignment contract is missing"))?;
            if !contract.active
                || contract.work_id != job.work_id
                || contract.version.get() != job.contract_version.get()
                || contract.contract_digest != job.contract_digest
            {
                return Err(bundle_conflict(
                    "bundle assignment contract differs from the runtime job",
                ));
            }
            for capture in [
                semantic_capture(
                    BundleEntryKind::Packet,
                    packet_path,
                    &PortablePacketBody {
                        packet: packet.clone(),
                        claim: resolution.claim.clone(),
                    },
                )?,
                semantic_capture(
                    BundleEntryKind::Assignment,
                    entry_path(BundleEntryKind::Assignment, packet.work_id.as_str())?,
                    &PortableAssignmentBody {
                        contract,
                        job: (*job).clone(),
                    },
                )?,
                semantic_capture(
                    BundleEntryKind::ResultSchema,
                    entry_path(BundleEntryKind::ResultSchema, packet.packet_id.as_str())?,
                    &job.candidate_result_contract,
                )?,
            ] {
                semantic_entry_bindings.push(entry_for(
                    self.artifacts.as_ref(),
                    captured,
                    semantic_entries,
                    verify_physical,
                    &capture,
                )?);
            }
            packet_bindings.push(BundlePacketBinding {
                packet_id: packet.packet_id.clone(),
                packet_digest: packet.packet_digest,
                work_id: packet.work_id.clone(),
                contract_id: packet.contract_id.clone(),
                contract_version: packet.contract_version,
                contract_digest: packet.contract_digest,
                relevant_basis: job.relevant_basis,
            });
            attempt_bindings.push(BundleAttemptBinding {
                job_id: job.job_id.clone(),
                attempt_id: job.attempt_id.clone(),
                dispatch_id: job.dispatch_id.clone(),
                effect_id: job.effect_id.clone(),
                packet_id: job.packet_id.clone(),
                packet_resolution_digest: job.packet_resolution_digest,
                dispatch_intent_digest: job.intent.digest()?,
                producer: job.producer.clone(),
                capability_observation: job.expected_producer.capability_observation.clone(),
                capability_digest: job.expected_producer.capability_digest,
                workspace_manifest: job.workspace_manifest,
            });
            for row in resolution.sources {
                insert_same(
                    &mut sources,
                    row.source_id.clone(),
                    BundleSourceBinding {
                        source_id: row.source_id,
                        source_digest: row.source_digest,
                        artifact: row.material.artifact,
                        byte_len: row.material.byte_len,
                    },
                )?;
                material_entries.insert(
                    (BundleEntryKind::Source, row.material.artifact),
                    row.material.byte_len,
                );
            }
            for row in resolution.rules {
                let key = (row.requirement.clone(), row.source_id.clone());
                insert_same(
                    &mut rules,
                    key,
                    BundleRuleBinding {
                        requirement: row.requirement,
                        source_id: row.source_id,
                        source_digest: row.source_digest,
                        artifact: row.material.artifact,
                        byte_len: row.material.byte_len,
                    },
                )?;
                material_entries.insert(
                    (BundleEntryKind::Rule, row.material.artifact),
                    row.material.byte_len,
                );
            }
            for row in resolution.forks {
                insert_same(
                    &mut forks,
                    row.fork_id.clone(),
                    BundleForkBinding {
                        fork_id: row.fork_id,
                        semantic_digest: row.semantic_digest,
                        artifact: row.material.artifact,
                        byte_len: row.material.byte_len,
                    },
                )?;
                material_entries.insert(
                    (BundleEntryKind::Fork, row.material.artifact),
                    row.material.byte_len,
                );
            }
            material_entries.insert(
                (BundleEntryKind::Workspace, job.workspace_manifest),
                resolution.workspace_byte_len,
            );
        }

        let capability_ids = attempt_bindings
            .iter()
            .map(|row| row.capability_observation.clone())
            .collect::<BTreeSet<_>>();
        for attempt in &attempt_bindings {
            let capability = state
                .get_typed::<CapabilityObservationRecord>(&attempt.capability_observation)?
                .ok_or_else(|| bundle_missing("bundle capability observation is missing"))?;
            if capability.capabilities.digest()? != attempt.capability_digest {
                return Err(bundle_conflict(
                    "bundle capability digest does not match the actual attempt",
                ));
            }
        }
        let charter = one_active_charter(state)?;
        let mut actions = BTreeSet::from([ActionClass::parse("work.dispatch")?]);
        actions.extend(fork_actions(&strategy, &lowering)?);
        if actions
            .iter()
            .any(|action| !charter.allowed_actions.contains(action))
        {
            return Err(bundle_conflict(
                "bundle action is outside current charter permission",
            ));
        }
        let permissions = actions
            .into_iter()
            .map(|action| {
                let mut permission = CharterPermissionBinding {
                    charter_id: charter.charter_id.clone(),
                    charter_revision: charter.revision,
                    charter_digest: charter.digest,
                    action,
                    packet_ids: request.packet_ids.clone(),
                    digest: PayloadDigest::hash(b"pending"),
                };
                permission.digest = canonical_digest(&(
                    &permission.charter_id,
                    permission.charter_revision,
                    permission.charter_digest,
                    &permission.action,
                    &permission.packet_ids,
                ))?;
                Ok(permission)
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        let stop_rules = scan_all::<StopRuleRecord>(state)?
            .into_iter()
            .filter(|row| row.active && row.campaign_id == state.identity().campaign_id)
            .map(|rule| {
                let digest = canonical_digest(&rule)?;
                let path = entry_path(BundleEntryKind::StopRule, rule.stop_rule_id.as_str())?;
                let capture = semantic_capture(BundleEntryKind::StopRule, path, &rule)?;
                let entry = entry_for(
                    self.artifacts.as_ref(),
                    captured,
                    semantic_entries,
                    verify_physical,
                    &capture,
                )?;
                Ok((
                    StopRuleBinding {
                        stop_rule_id: rule.stop_rule_id,
                        revision: rule.revision,
                        rule_digest: digest,
                        artifact: entry.artifact,
                        byte_len: entry.byte_len,
                    },
                    entry,
                ))
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        let mut entries = material_entries
            .into_iter()
            .map(|((kind, artifact), byte_len)| {
                Ok(BundleEntryBinding {
                    kind,
                    path: entry_path(kind, &artifact.to_string())?,
                    artifact,
                    byte_len,
                })
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        entries.extend(stop_rules.iter().map(|(_, entry)| entry.clone()));
        entries.extend(semantic_entry_bindings);
        for id in &capability_ids {
            let capability = state
                .get_typed::<CapabilityObservationRecord>(id)?
                .ok_or_else(|| bundle_missing("capability disappeared"))?;
            let capture = semantic_capture(
                BundleEntryKind::Capability,
                entry_path(BundleEntryKind::Capability, id.as_str())?,
                &capability,
            )?;
            entries.push(entry_for(
                self.artifacts.as_ref(),
                captured,
                semantic_entries,
                verify_physical,
                &capture,
            )?);
        }
        for permission in &permissions {
            let capture = semantic_capture(
                BundleEntryKind::Permission,
                entry_path(BundleEntryKind::Permission, permission.action.as_str())?,
                permission,
            )?;
            entries.push(entry_for(
                self.artifacts.as_ref(),
                captured,
                semantic_entries,
                verify_physical,
                &capture,
            )?);
        }
        let identity = state.identity();
        let manifest = WeakBundleManifest {
            bundle_id: request.bundle_id.clone(),
            store_id: identity.store_id,
            campaign_id: identity.campaign_id,
            base_id: identity.base_id,
            export_revision: state.revision(),
            binding: BundleStrategyBinding {
                strategy_id: strategy.strategic_revision_id,
                strategy_revision: strategy.revision,
                strategy_semantic_digest: strategy.semantic_digest,
                lowering_id: lowering.lowering_id,
                lowering_revision: lowering.revision,
                lowering_semantic_digest: lowering.semantic_digest,
            },
            packets: packet_bindings,
            attempts: attempt_bindings,
            sources: sources.into_values().collect(),
            rules: rules.into_values().collect(),
            forks: forks.into_values().collect(),
            capabilities: capability_ids.into_iter().collect(),
            permissions,
            stop_rules: stop_rules.into_iter().map(|(rule, _)| rule).collect(),
            entries,
            maximum_archive_bytes: request.maximum_archive_bytes,
            maximum_encounters: request.maximum_encounters,
            maximum_return_bytes: request.maximum_return_bytes,
            encounter_genesis: PayloadDigest::hash(b"pending"),
            simulated: request.simulated,
            digest: zap_wire::BundleDigest::hash(b"pending"),
        }
        .seal()?;
        let closure = BundleClosureRecord {
            request_digest: request.request_digest,
            observed_revision: state.revision(),
            manifest,
        };
        if let Some(captured) = captured
            && &closure != captured
        {
            return Err(bundle_conflict(
                "captured bundle closure differs from current derivation",
            ));
        }
        Ok(closure)
    }
}
