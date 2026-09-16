#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use zap_core::*;
use zap_domain::owner_control::{
    CampaignPaused, PauseRecord, PauseResumed, PauseScope, PauseSource, PauseStatus,
};
use zap_runtime::*;
use zap_store::{ArtifactStore, RedbStore};
use zap_wire::*;

use super::frame;

include!("r11_candidate/read_port.rs");
include!("r11_candidate/candidate_input.rs");

pub(super) struct RecordedCandidate {
    pub(super) candidate_id: CandidateId,
    pub(super) artifact: ArtifactDigest,
    pub(super) job: RuntimeJobRecord,
    pub(super) required_stage: zap_core::MaturityStage,
    pub(super) second: Option<(
        CandidateId,
        ArtifactDigest,
        RuntimeJobRecord,
        zap_core::MaturityStage,
    )>,
}

pub(super) fn record_real_candidate(
    input: RecordCandidateInput<'_>,
) -> Result<RecordedCandidate, Box<dyn std::error::Error>> {
    let RecordCandidateInput {
        service,
        store,
        identity,
        artifact_store,
        packet_artifact_store,
        capture_root,
        capabilities,
        observation,
        trusted,
        internal,
        job_id,
        probe_claim,
        second_probe,
    } = input;
    let snapshot = store.read(ReadAt::Current)?;
    let job = snapshot
        .get_typed::<RuntimeJobRecord>(job_id)?
        .ok_or("runtime job missing before native launch")?;
    drop(snapshot);
    let bridge = NativeBridge::new(capabilities.clone(), observation.clone())?;
    let awaiting = AgentHost::dispatch(&bridge, job.intent.clone())?;
    assert_eq!(awaiting.state, DispatchState::AwaitingHarness);
    let reads = SnapshotPort {
        store: store.clone(),
    };
    let factory = FrameFactory {
        store: store.clone(),
        identity: identity.clone(),
        sequence: AtomicU64::new(1_000),
        prepare_fail: AtomicBool::new(false),
        prepare_calls: AtomicU64::new(0),
    };
    let principal = service.credential_authority().authenticate(
        &CredentialId::parse("coordinator.packet-resolution")?,
        SecretInput::new(b"lowering-test-secret"),
        &identity.campaign_id,
    )?;
    let provenance = DriverProvenance::bind(capabilities, observation.clone())?;
    let awaiting_frame = factory.dispatch_receipt(&job, &awaiting, &provenance)?;
    let awaiting_grant = trusted.authorize(
        &awaiting_frame,
        OperationRef::Command(awaiting_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&awaiting_grant),
        awaiting_frame,
    )?;
    let second_job = second_probe
        .map(|(job_id, _)| {
            store
                .read(ReadAt::Current)?
                .get_typed::<RuntimeJobRecord>(job_id)?
                .ok_or_else(|| test_error("second runtime job is missing before native launch"))
        })
        .transpose()?;
    let second_bridge = second_job
        .as_ref()
        .map(|_| NativeBridge::new(capabilities.clone(), observation.clone()))
        .transpose()?;
    if let (Some(job), Some(bridge)) = (&second_job, &second_bridge) {
        let awaiting = AgentHost::dispatch(bridge, job.intent.clone())?;
        let frame = factory.dispatch_receipt(job, &awaiting, &provenance)?;
        let grant = trusted.authorize(
            &frame,
            OperationRef::Command(frame.header().command_id().clone()),
        )?;
        service.submit(PrincipalContext::TrustedObservation(&grant), frame)?;
    }
    let driver = NativeDriverCoordinator::new(&reads, service, &bridge, &factory);
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("owner.packet-resolution")?,
        SecretInput::new(b"lowering-test-secret"),
        &identity.campaign_id,
    )?;
    let pause_id = PauseId::parse("pause.native-pickup")?;
    let pause_digest = PayloadDigest::hash(b"pause.native-pickup");
    let pause_frame = frame(
        identity,
        &CampaignPaused {
            pause: PauseRecord {
                pause_id: pause_id.clone(),
                campaign_id: identity.campaign_id.clone(),
                scope: PauseScope::Campaign(identity.campaign_id.clone()),
                source: PauseSource::Owner,
                reason: BoundedText::parse("Hold native driver pickup after durable enqueue")?,
                charter_revision: Revision::new(1),
                status: PauseStatus::Active,
                state_digest: pause_digest,
                revision: store.head()?.checked_next()?,
            },
        },
        store.head()?,
        "command.pause.native-pickup",
    )?;
    service.submit(PrincipalContext::Credentialed(&owner), pause_frame)?;
    assert_eq!(
        driver
            .prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Paused)
    );
    if let (Some(job), Some(bridge)) = (&second_job, &second_bridge) {
        let second_driver = NativeDriverCoordinator::new(&reads, service, bridge, &factory);
        assert_eq!(
            second_driver
                .prepare_launch(
                    &job.job_id,
                    &job.dispatch_id,
                    NativeDriverAuthority::Privileged(&principal),
                )
                .err()
                .map(|error| error.code),
            Some(ErrorCode::Paused)
        );
    }
    assert_eq!(bridge.pending_intents()?.len(), 1);
    let resume_frame = frame(
        identity,
        &PauseResumed {
            pause_id,
            expected_state_digest: pause_digest,
        },
        store.head()?,
        "command.resume.native-pickup",
    )?;
    service.submit(PrincipalContext::Credentialed(&owner), resume_frame)?;
    let authorization = driver.prepare_launch(
        &job.job_id,
        &job.dispatch_id,
        NativeDriverAuthority::Privileged(&principal),
    )?;
    assert!(matches!(
        authorization,
        NativeDriverStep::AuthorizationCommitted { .. }
    ));
    if let (Some(job), Some(bridge)) = (&second_job, &second_bridge) {
        let second_driver = NativeDriverCoordinator::new(&reads, service, bridge, &factory);
        assert!(matches!(
            second_driver.prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )?,
            NativeDriverStep::AuthorizationCommitted { .. }
        ));
    }
    let launch = driver.prepare_launch(
        &job.job_id,
        &job.dispatch_id,
        NativeDriverAuthority::Internal(internal),
    )?;
    let NativeDriverStep::ReadyToInvoke { ticket } = launch else {
        return Err(Box::new(test_error("native launch ticket was not issued")));
    };
    let second_ticket = if let (Some(job), Some(bridge)) = (&second_job, &second_bridge) {
        let second_driver = NativeDriverCoordinator::new(&reads, service, bridge, &factory);
        let NativeDriverStep::ReadyToInvoke { ticket } = second_driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Internal(internal),
        )?
        else {
            return Err("second native launch ticket was not issued".into());
        };
        Some(ticket)
    } else {
        None
    };
    let mut probe = prepare_native_probe(probe_claim, packet_artifact_store, "alpha")?;
    let mut second_probe_context = second_probe
        .map(|(_, claim)| prepare_native_probe(claim, packet_artifact_store, "beta"))
        .transpose()?
        .flatten();
    if let Some(probe) = &probe {
        let second = second_probe
            .map(|(_, claim)| claim)
            .ok_or("native probe requires a second committed claim")?;
        write_probe_manifest(
            probe,
            store,
            identity,
            probe_claim,
            second,
            second_probe_context
                .as_ref()
                .ok_or("native probe second lane is missing")?,
        )?;
        write_probe_phase(probe, store, "ready_for_native_start")?;
    }
    let opaque_handle = match &mut probe {
        Some(probe) => wait_for_start_receipt(probe)?,
        None => "simulated-r09-native-handle".to_owned(),
    };
    let second_opaque_handle = match &mut second_probe_context {
        Some(probe) => Some(wait_for_start_receipt(probe)?),
        None => second_job
            .as_ref()
            .map(|_| "simulated-r09-native-handle-two".to_owned()),
    };
    let handle = ExternalJobHandle::new(
        capabilities.adapter.name.clone(),
        capabilities.harness_id.clone(),
        identity.campaign_id.clone(),
        job.job_id.clone(),
        job.attempt_id.clone(),
        job.intent.digest()?,
        BoundedText::parse(&opaque_handle)?,
    );
    driver.record_receipt(
        &ticket,
        DispatchState::Running,
        handle.clone(),
        &provenance,
        trusted,
    )?;
    let second_handle = if let (Some(job), Some(bridge), Some(ticket), Some(opaque)) = (
        &second_job,
        &second_bridge,
        second_ticket.as_ref(),
        second_opaque_handle,
    ) {
        let handle = ExternalJobHandle::new(
            capabilities.adapter.name.clone(),
            capabilities.harness_id.clone(),
            identity.campaign_id.clone(),
            job.job_id.clone(),
            job.attempt_id.clone(),
            job.intent.digest()?,
            BoundedText::parse(&opaque)?,
        );
        NativeDriverCoordinator::new(&reads, service, bridge, &factory).record_receipt(
            ticket,
            DispatchState::Running,
            handle.clone(),
            &provenance,
            trusted,
        )?;
        Some(handle)
    } else {
        None
    };
    if let Some(probe) = &probe {
        write_probe_phase(probe, store, "native_running_receipts_recorded")?;
    }
    let candidate_bytes = if super::fixtures::r16_deterministic_fixture_enabled() {
        deterministic_fixture_candidate(probe_claim, packet_artifact_store)?
    } else {
        finish_native_probe(probe.as_ref())?
    };
    let second_candidate_bytes = second_probe
        .map(|(_, claim)| {
            if super::fixtures::r16_deterministic_fixture_enabled() {
                deterministic_fixture_candidate(claim, packet_artifact_store)
            } else {
                finish_native_probe(second_probe_context.as_ref())
            }
        })
        .transpose()?;
    if let Some(probe) = &probe {
        write_probe_phase(probe, store, "native_final_observations_received")?;
    }
    driver.record_job_observation(
        &handle,
        JobObservation {
            state: HostJobState::Succeeded,
            observation: observation.clone(),
            active: Some(false),
            ownership_verified: true,
        },
        &provenance,
        trusted,
    )?;
    if let (Some(bridge), Some(handle)) = (&second_bridge, &second_handle) {
        NativeDriverCoordinator::new(&reads, service, bridge, &factory).record_job_observation(
            handle,
            JobObservation {
                state: HostJobState::Succeeded,
                observation: observation.clone(),
                active: Some(false),
                ownership_verified: true,
            },
            &provenance,
            trusted,
        )?;
    }

    let candidate_path = capture_root.join("r09-candidate.patch");
    std::fs::write(&candidate_path, candidate_bytes)?;
    let artifact = artifact_store.prepare_file(&candidate_path)?.publish()?;
    let current = store
        .read(ReadAt::Current)?
        .get_typed::<RuntimeJobRecord>(job_id)?
        .ok_or("terminal runtime job missing")?;
    let mut checks = Vec::new();
    for verification_id in &current.candidate_result_contract.template.required_checks {
        service.submit(
            PrincipalContext::Credentialed(&principal),
            factory.verification_claim(&VerificationClaimedPayload {
                record: VerificationRecord {
                    verification_id: verification_id.clone(),
                    job_id: current.job_id.clone(),
                    scope: VerificationScope::SafeBoundary {
                        attempt_id: current.attempt_id.clone(),
                        effect_id: current.effect_id.clone(),
                        boundary: current.intent.safe_stop.boundary.clone(),
                    },
                    state: VerificationState::Claimed,
                    observation: None,
                    artifacts: Vec::new(),
                    revision: store.head()?.checked_next()?,
                },
            })?,
        )?;
        let claimed = store
            .read(ReadAt::Current)?
            .get_typed::<VerificationRecord>(verification_id)?
            .ok_or("claimed candidate verification missing")?;
        let result_frame = factory.verification_result(&VerificationResultPayload {
            verification_id: verification_id.clone(),
            expected_record_revision: claimed.revision,
            state: VerificationState::Passed,
            observation: observation.clone(),
            artifacts: vec![artifact.digest()],
            provenance: provenance.clone(),
        })?;
        let grant = trusted.authorize(
            &result_frame,
            OperationRef::Command(result_frame.header().command_id().clone()),
        )?;
        service.submit(PrincipalContext::TrustedObservation(&grant), result_frame)?;
        checks.push(CheckRef {
            verification_id: verification_id.clone(),
            observation: observation.clone(),
        });
    }
    driver.record_safe_state(
        job_id,
        SafeState::Completed,
        observation.clone(),
        checks
            .iter()
            .map(|check| check.verification_id.clone())
            .collect(),
        &provenance,
        trusted,
    )?;
    let candidate_id = CandidateId::parse("candidate.packet-resolution")?;
    let candidate = CandidateResult {
        candidate_id: candidate_id.clone(),
        producer: current.producer.clone(),
        work_id: current.work_id.clone(),
        contract_id: current.contract_id.clone(),
        contract_digest: current.contract_digest,
        relevant_basis: current.relevant_basis,
        terminal_observation: observation.clone(),
        artifacts: vec![ArtifactRef {
            digest: artifact.digest(),
            kind: ArtifactKind::Patch,
            byte_len: artifact.byte_len(),
        }],
        criteria: current
            .candidate_result_contract
            .template
            .required_criteria
            .iter()
            .map(|criterion| CriterionResult {
                requirement: criterion.requirement.clone(),
                disposition: CriterionDisposition::Satisfied,
                evidence: Vec::new(),
            })
            .collect(),
        checks,
        discoveries: Vec::new(),
        unresolved: Vec::new(),
        proposed_follow_up: None,
        effect_state: CandidateEffectState::NotStarted,
        safe_boundary: current.intent.safe_stop.boundary.clone(),
    };
    driver.record_candidate(&handle, candidate, &provenance, trusted)?;
    let mut second_recorded = None;
    if let (Some(job), Some(bridge), Some(handle), Some(bytes)) = (
        &second_job,
        &second_bridge,
        &second_handle,
        second_candidate_bytes,
    ) {
        let path = capture_root.join("r16-candidate-second.patch");
        std::fs::write(&path, bytes)?;
        let artifact = artifact_store.prepare_file(&path)?.publish()?;
        let current = store
            .read(ReadAt::Current)?
            .get_typed::<RuntimeJobRecord>(&job.job_id)?
            .ok_or("second terminal runtime job missing")?;
        let mut checks = Vec::new();
        for verification_id in &current.candidate_result_contract.template.required_checks {
            service.submit(
                PrincipalContext::Credentialed(&principal),
                factory.verification_claim(&VerificationClaimedPayload {
                    record: VerificationRecord {
                        verification_id: verification_id.clone(),
                        job_id: current.job_id.clone(),
                        scope: VerificationScope::SafeBoundary {
                            attempt_id: current.attempt_id.clone(),
                            effect_id: current.effect_id.clone(),
                            boundary: current.intent.safe_stop.boundary.clone(),
                        },
                        state: VerificationState::Claimed,
                        observation: None,
                        artifacts: Vec::new(),
                        revision: store.head()?.checked_next()?,
                    },
                })?,
            )?;
            let claimed = store
                .read(ReadAt::Current)?
                .get_typed::<VerificationRecord>(verification_id)?
                .ok_or("claimed second verification missing")?;
            let result_frame = factory.verification_result(&VerificationResultPayload {
                verification_id: verification_id.clone(),
                expected_record_revision: claimed.revision,
                state: VerificationState::Passed,
                observation: observation.clone(),
                artifacts: vec![artifact.digest()],
                provenance: provenance.clone(),
            })?;
            let grant = trusted.authorize(
                &result_frame,
                OperationRef::Command(result_frame.header().command_id().clone()),
            )?;
            service.submit(PrincipalContext::TrustedObservation(&grant), result_frame)?;
            checks.push(CheckRef {
                verification_id: verification_id.clone(),
                observation: observation.clone(),
            });
        }
        let second_driver = NativeDriverCoordinator::new(&reads, service, bridge, &factory);
        second_driver.record_safe_state(
            &job.job_id,
            SafeState::Completed,
            observation.clone(),
            checks
                .iter()
                .map(|check| check.verification_id.clone())
                .collect(),
            &provenance,
            trusted,
        )?;
        let second_candidate_id = CandidateId::parse("candidate.packet-second")?;
        let candidate = CandidateResult {
            candidate_id: second_candidate_id.clone(),
            producer: current.producer.clone(),
            work_id: current.work_id.clone(),
            contract_id: current.contract_id.clone(),
            contract_digest: current.contract_digest,
            relevant_basis: current.relevant_basis,
            terminal_observation: observation.clone(),
            artifacts: vec![ArtifactRef {
                digest: artifact.digest(),
                kind: ArtifactKind::Patch,
                byte_len: artifact.byte_len(),
            }],
            criteria: current
                .candidate_result_contract
                .template
                .required_criteria
                .iter()
                .map(|criterion| CriterionResult {
                    requirement: criterion.requirement.clone(),
                    disposition: CriterionDisposition::Satisfied,
                    evidence: Vec::new(),
                })
                .collect(),
            checks,
            discoveries: Vec::new(),
            unresolved: Vec::new(),
            proposed_follow_up: None,
            effect_state: CandidateEffectState::NotStarted,
            safe_boundary: current.intent.safe_stop.boundary.clone(),
        };
        second_driver.record_candidate(handle, candidate, &provenance, trusted)?;
        let final_job = store
            .read(ReadAt::Current)?
            .get_typed::<RuntimeJobRecord>(&job.job_id)?
            .ok_or("second candidate runtime job missing")?;
        let second_stage = second_probe
            .map(|(_, claim)| claim.work.required_stage)
            .ok_or("second native claim stage is missing")?;
        second_recorded = Some((
            second_candidate_id,
            artifact.digest(),
            final_job,
            second_stage,
        ));
    }
    let snapshot = store.read(ReadAt::Current)?;
    let final_job = snapshot
        .get_typed::<RuntimeJobRecord>(job_id)?
        .ok_or("candidate runtime job missing")?;
    assert!(
        snapshot
            .get_typed::<CandidateResultRecord>(&candidate_id)?
            .is_some()
    );
    assert!(
        snapshot
            .get_typed::<CandidateProvenanceRecord>(&candidate_id)?
            .is_some()
    );
    assert!(final_job.execution.is_terminal());
    assert_eq!(final_job.collection, CollectionState::CandidateRecorded);
    assert_eq!(final_job.safe, SafeState::Completed);
    if let Some(probe) = &probe {
        write_probe_phase(probe, store, "product_candidates_recorded")?;
    }
    Ok(RecordedCandidate {
        candidate_id,
        artifact: artifact.digest(),
        job: final_job,
        required_stage: probe_claim.work.required_stage,
        second: second_recorded,
    })
}

include!("r11_candidate/probe.rs");
