use super::*;
use zap_domain::owner_control::{ApproachEpochAdvanced, ApproachEpochRecord};

#[allow(clippy::too_many_arguments)]
pub(super) fn import_failure_return(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    artifact_store: &ArtifactStore,
    capture_root: &std::path::Path,
    harness_id: &HarnessId,
    trusted: &TrustedHostHandle,
    bundle: &WeakBundleRecord,
    job: &RuntimeJobRecord,
) -> Result<ReturnImportRecord, Box<dyn std::error::Error>> {
    let problem_id = ProblemId::parse("problem.return-relower")?;
    let approach_digest = PayloadDigest::hash(b"approach.return-relower");
    establish_approach_epoch(service, store, identity, problem_id.clone())?;
    let mut journal = OfflineEncounterJournal::new(&bundle.manifest)?;
    let started = OfflineEncounter {
        encounter_id: EncounterId::parse("encounter.relower-started")?,
        sequence: 0,
        previous_digest: bundle.manifest.encounter_genesis,
        causes: Vec::new(),
        job_id: job.job_id.clone(),
        attempt_id: job.attempt_id.clone(),
        packet_id: job.packet_id.clone(),
        producer: job.producer.clone(),
        kind: EncounterKind::AttemptStarted,
        approach: None,
        selected_fork: None,
        candidate_id: None,
        detail: BoundedText::parse("second offline return started")?,
        artifacts: Vec::new(),
        evidence_ids: Vec::new(),
        effect_state: CandidateEffectState::NotStarted,
        digest: PayloadDigest::hash(b"pending"),
    }
    .seal()?;
    journal.append(started.clone())?;
    journal.append(
        OfflineEncounter {
            encounter_id: EncounterId::parse("encounter.relower-failure")?,
            sequence: 1,
            previous_digest: started.digest,
            causes: vec![started.encounter_id],
            job_id: job.job_id.clone(),
            attempt_id: job.attempt_id.clone(),
            packet_id: job.packet_id.clone(),
            producer: job.producer.clone(),
            kind: EncounterKind::Failure,
            approach: Some(EncounterApproachBinding {
                problem_id: problem_id.clone(),
                epoch: 1,
                approach_digest,
            }),
            selected_fork: None,
            candidate_id: None,
            detail: BoundedText::parse("the returned implementation approach failed")?,
            artifacts: Vec::new(),
            evidence_ids: Vec::new(),
            effect_state: CandidateEffectState::NotStarted,
            digest: PayloadDigest::hash(b"pending"),
        }
        .seal()?,
    )?;
    let delta = journal.finish()?;
    let archive_path = capture_root.join("return-archive-relower.bin");
    std::fs::write(&archive_path, b"simulated-return-archive-relower")?;
    let archive = artifact_store.prepare_file(&archive_path)?.publish()?;
    let input = seal_return_bundle(ReturnBundleInput {
        source_bundle_id: bundle.bundle_id.clone(),
        source_manifest_digest: bundle.manifest_digest,
        base_id: identity.base_id.clone(),
        binding: bundle.manifest.binding.clone(),
        delta: delta.clone(),
        archive: ReturnArchiveReceipt {
            source_bundle_id: bundle.bundle_id.clone(),
            source_manifest_digest: bundle.manifest_digest,
            delta_digest: delta.digest,
            entries_digest: PayloadDigest::hash(b"return-relower-entries"),
            archive_artifact: archive.digest(),
            byte_len: archive.byte_len(),
            harness_id: harness_id.clone(),
            observation: ObservationRef::parse("observation.packet-resolution")?,
        },
        digest: ReturnBundleDigest::hash(b"pending"),
    })?;
    let import_frame = frame(
        identity,
        &ReturnImported {
            schema: ReturnImportedSchema::V1,
            expected_import_revision: Revision::GENESIS,
            input: input.clone(),
        },
        store.head()?,
        "command.return.packet-relower",
    )?;
    let grant = trusted.authorize(
        &import_frame,
        OperationRef::Command(import_frame.header().command_id().clone()),
    )?;
    let receipt = service.submit(PrincipalContext::TrustedObservation(&grant), import_frame)?;
    let repeated = frame(
        identity,
        &ReturnImported {
            schema: ReturnImportedSchema::V1,
            expected_import_revision: Revision::GENESIS,
            input,
        },
        store.head()?,
        "command.return.packet-relower-repeated",
    )?;
    let repeated_grant = trusted.authorize(
        &repeated,
        OperationRef::Command(repeated.header().command_id().clone()),
    )?;
    let repeated_receipt = service.submit(
        PrincipalContext::TrustedObservation(&repeated_grant),
        repeated,
    )?;
    assert_eq!(
        repeated_receipt.revision(),
        receipt.revision().checked_next()?
    );
    let imported = store
        .read(ReadAt::Current)?
        .get_typed::<ReturnImportRecord>(&bundle.bundle_id)?
        .ok_or("relowering return import missing")?;
    assert_eq!(
        imported.classifications,
        vec![
            (
                EncounterId::parse("encounter.relower-started")?,
                ReturnClassification::ApplicableObservation,
            ),
            (
                EncounterId::parse("encounter.relower-failure")?,
                ReturnClassification::Failure,
            ),
        ]
    );
    let snapshot = store.read(ReadAt::Current)?;
    let failed = snapshot
        .get_typed::<FailedApproachRecord>(&FailedApproachKey {
            problem_id: problem_id.clone(),
            epoch: 1,
            approach_digest,
        })?
        .ok_or("failed approach history missing")?;
    let epoch = snapshot
        .get_typed::<ApproachEpochRecord>(&problem_id)?
        .ok_or("approach epoch missing after return")?;
    assert_eq!(failed.disposition, CounterDisposition::Counted);
    assert_eq!(epoch.failed_approaches, 1);
    Ok(imported)
}

fn establish_approach_epoch(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    problem_id: ProblemId,
) -> Result<(), Box<dyn std::error::Error>> {
    let head = store.head()?;
    let frame = frame(
        identity,
        &ApproachEpochAdvanced {
            epoch: ApproachEpochRecord {
                problem_id,
                epoch: 1,
                failed_approaches: 0,
                reason: BoundedText::parse("Track the failed offline approach exactly once")?,
                revision: head.checked_next()?,
            },
        },
        head,
        "command.approach-epoch.packet-relower",
    )?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("owner.packet-resolution")?,
        SecretInput::new(b"lowering-test-secret"),
        &identity.campaign_id,
    )?;
    service.submit(PrincipalContext::Credentialed(&owner), frame)?;
    Ok(())
}
