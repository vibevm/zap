use zap_core::{ActorRef, CandidateEffectState, OperationRef, PrincipalRole, ProducerRef};
use zap_domain::lowering::*;
use zap_wire::*;

#[test]
fn bounded_offline_journal_preserves_attempt_chain_and_rejects_candidate_labels()
-> Result<(), ZapError> {
    let manifest = manifest()?;
    let attempt = &manifest.attempts[0];
    let mut journal = OfflineEncounterJournal::new(&manifest)?;
    let first = encounter(
        attempt,
        EncounterId::parse("encounter.0")?,
        0,
        manifest.encounter_genesis,
        EncounterKind::AttemptStarted,
        None,
    )?
    .seal()?;
    journal.append(first.clone())?;
    let observation = encounter(
        attempt,
        EncounterId::parse("encounter.1")?,
        1,
        first.digest,
        EncounterKind::Observation,
        None,
    )?
    .seal()?;
    journal.append(observation.clone())?;
    let failure = OfflineEncounter {
        approach: Some(EncounterApproachBinding {
            problem_id: ProblemId::parse("problem.one")?,
            epoch: 1,
            approach_digest: PayloadDigest::hash(b"approach.one"),
        }),
        ..encounter(
            attempt,
            EncounterId::parse("encounter.2")?,
            2,
            observation.digest,
            EncounterKind::Failure,
            None,
        )?
    }
    .seal()?;
    journal.append(failure)?;
    let delta = journal.finish()?;
    validate_delta(&manifest, &delta)?;
    validate_encounter_delta(&manifest, &delta)?;
    assert_eq!(delta.encounters.len(), 3);

    let candidate_without_id = encounter(
        attempt,
        EncounterId::parse("encounter.bad-candidate")?,
        3,
        delta.final_digest,
        EncounterKind::CandidateProduced,
        None,
    )?;
    assert!(candidate_without_id.seal().is_err());
    let wrong_previous = encounter(
        attempt,
        EncounterId::parse("encounter.bad-chain")?,
        3,
        PayloadDigest::hash(b"foreign"),
        EncounterKind::Observation,
        None,
    )?
    .seal()?;
    let mut rebuilt = OfflineEncounterJournal::new(&manifest)?;
    assert!(rebuilt.append(wrong_previous).is_err());

    let archive = ReturnArchiveReceipt {
        source_bundle_id: manifest.bundle_id.clone(),
        source_manifest_digest: manifest.digest,
        delta_digest: delta.digest,
        entries_digest: PayloadDigest::hash(b"return.entries"),
        archive_artifact: ArtifactDigest::hash(b"return.archive"),
        byte_len: 128,
        harness_id: HarnessId::parse("harness.offline")?,
        observation: ObservationRef::parse("observation.offline")?,
    };
    let returned = seal_return_bundle(ReturnBundleInput {
        source_bundle_id: manifest.bundle_id.clone(),
        source_manifest_digest: manifest.digest,
        base_id: manifest.base_id.clone(),
        binding: manifest.binding.clone(),
        delta,
        archive,
        digest: ReturnBundleDigest::hash(b"pending"),
    })?;
    validate_return_bundle(&manifest, &returned)?;
    Ok(())
}

fn manifest() -> Result<WeakBundleManifest, ZapError> {
    let packet_id = PacketId::parse("packet.offline")?;
    let job_id = JobId::parse("job.offline")?;
    let attempt_id = AttemptId::parse("attempt.offline")?;
    let producer = ProducerRef {
        actor: ActorRef {
            principal_id: PrincipalId::parse("worker.offline")?,
            operation: OperationRef::Attempt(attempt_id.clone()),
            role: PrincipalRole::Worker,
        },
        job_id: job_id.clone(),
        attempt_id: attempt_id.clone(),
        packet_id: packet_id.clone(),
    };
    WeakBundleManifest {
        bundle_id: BundleId::parse("bundle.offline")?,
        store_id: StoreId::parse("store.offline")?,
        campaign_id: CampaignId::parse("campaign.offline")?,
        base_id: BaseId::parse("base.offline")?,
        export_revision: Revision::new(8),
        binding: BundleStrategyBinding {
            strategy_id: StrategicRevisionId::parse("strategy.offline")?,
            strategy_revision: Revision::new(1),
            strategy_semantic_digest: PayloadDigest::hash(b"strategy"),
            lowering_id: LoweringId::parse("lowering.offline")?,
            lowering_revision: Revision::new(1),
            lowering_semantic_digest: PayloadDigest::hash(b"lowering"),
        },
        packets: vec![BundlePacketBinding {
            packet_id: packet_id.clone(),
            packet_digest: PacketDigest::hash(b"packet"),
            work_id: WorkId::parse("work.offline")?,
            contract_id: ContractId::parse("contract.offline")?,
            contract_version: Revision::new(1),
            contract_digest: ContractDigest::hash(b"contract"),
            relevant_basis: RelevantBasisDigest::hash(b"basis"),
        }],
        attempts: vec![BundleAttemptBinding {
            job_id,
            attempt_id,
            dispatch_id: DispatchId::parse("dispatch.offline")?,
            effect_id: EffectId::parse("effect.offline")?,
            packet_id,
            packet_resolution_digest: PacketResolutionDigest::hash(b"resolution"),
            dispatch_intent_digest: DispatchIntentDigest::hash(b"intent"),
            producer,
            capability_observation: CapabilityObservationId::parse("capability.offline")?,
            capability_digest: CapabilityDigest::hash(b"capability"),
            workspace_manifest: ArtifactDigest::hash(b"workspace"),
        }],
        sources: Vec::new(),
        rules: Vec::new(),
        forks: Vec::new(),
        capabilities: vec![CapabilityObservationId::parse("capability.offline")?],
        permissions: vec![CharterPermissionBinding {
            charter_id: CharterId::parse("charter.offline")?,
            charter_revision: Revision::new(1),
            charter_digest: PayloadDigest::hash(b"charter"),
            action: ActionClass::parse("work.dispatch")?,
            packet_ids: vec![PacketId::parse("packet.offline")?],
            digest: PayloadDigest::hash(b"permission"),
        }],
        stop_rules: Vec::new(),
        entries: vec![BundleEntryBinding {
            kind: BundleEntryKind::Packet,
            path: BoundedText::parse("packets/packet.offline.json")?,
            artifact: ArtifactDigest::hash(b"packet.entry"),
            byte_len: 64,
        }],
        maximum_archive_bytes: 1024,
        maximum_encounters: 8,
        maximum_return_bytes: 8192,
        encounter_genesis: PayloadDigest::hash(b"pending"),
        simulated: true,
        digest: BundleDigest::hash(b"pending"),
    }
    .seal()
}

fn encounter(
    attempt: &BundleAttemptBinding,
    encounter_id: EncounterId,
    sequence: u32,
    previous_digest: PayloadDigest,
    kind: EncounterKind,
    candidate_id: Option<CandidateId>,
) -> Result<OfflineEncounter, ZapError> {
    Ok(OfflineEncounter {
        encounter_id,
        sequence,
        previous_digest,
        causes: Vec::new(),
        job_id: attempt.job_id.clone(),
        attempt_id: attempt.attempt_id.clone(),
        packet_id: attempt.packet_id.clone(),
        producer: attempt.producer.clone(),
        kind,
        approach: None,
        selected_fork: None,
        candidate_id,
        detail: BoundedText::parse("bounded encounter")?,
        artifacts: Vec::new(),
        evidence_ids: Vec::new(),
        effect_state: CandidateEffectState::NotStarted,
        digest: PayloadDigest::hash(b"pending"),
    })
}
