use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use zap_core::{
    ActionAdmissionBasis, ActionAdmissionNeeds, ActionAdmissionObservation,
    ActionAdmissionPreflight, ActionAdmissionProvider, ActionAdmissionRequest,
    ActionProductOutcome, ActorRef, AdmissionHookDescriptor, AdmissionMutationScope,
    AuthenticatedPrincipal, CandidateProvenanceInput, CandidateProvenanceRecord, CapabilityId,
    CellDescriptor, CellDescriptorInput, CellSet, ChangeSet, CommandPayload, CommitService,
    CommitServiceBuilder, ControllerEpoch, CoordinatorScope, CredentialAuthority,
    EffectBundleDraft, EffectBundleRequest, EffectDraft, OperationRef, OwnerScope,
    PrincipalContext, RecordFamily, RecordSet, ReplayContext, ReplayProviders, RouteRegistry,
    SecretInput, SecretVerifier, StateReader, StateReaderExt, StoredRecord, TransitionCell,
    TrustBootstrapSource, TrustRegistrar, TrustedHostBinding, TrustedHostHandle, ValidatedCommand,
    WorkExecutionObservationRecord, WorkRevalidationReleaseInput, WorkRevalidationReleaseRecord,
};
use zap_domain::acceptance::{CandidateReviewRecord, EvidenceAdjudicationRecord};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::economics::{DomainActionImpactProvider, DomainAffectedScopeProvider};
use zap_domain::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use zap_domain::knowledge::{
    AdaptiveReviewRecord, DomainBasisProvider, FactRecord, KnowledgeClosureRecord,
    KnowledgeDependencyRecord, RegionRecord, SemanticAssessmentRecord, SourceApplicabilityRecord,
    SourceRecord,
};
use zap_domain::lowering::StrategicPlanRecord;
use zap_domain::milestone_planning::{MilestonePlanProposalRecord, MilestonePlanStateRecord};
use zap_domain::milestones::{MilestoneRecord, MilestoneRevisionRecord};
use zap_domain::seams::DomainMutation;
use zap_domain::seams::{
    CharterDutyAuthority, CompletionDutyPolicy, LifecycleStatus, ObligationDisposition,
};
use zap_store::RedbStore;
use zap_wire::{
    ActionClass, AdmissionId, AuthorizationRef, BaseId, BasisBinding, BoundedText, CampaignId,
    CanonicalCommandFrame, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CharterId, CodecEpoch, CommandDigest, CommandHeader, CommandHeaderInput, CommandId,
    CommandReason, CommandReasonInput, ControlClass, CredentialId, ErrorCode, ErrorDetail, EventId,
    EventKind, FixSurface, IntentId, ObservationRef, OutcomeId, PayloadDigest, PolicyId,
    PrincipalId, ProtocolEpoch, QueryEpoch, ReducerEpoch, RequirementRef, Revision, RouteClass,
    StoreEpoch, StoreId, ZapError,
};

const SEED_KIND: &str = "fixture.knowledge-seed";
const TEST_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-DATA-AND-VIEWER#DEPENDENT-INVALIDATION";

include!("support/seed.rs");
struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == b"domain-test-secret"
    }
}

struct TestBootstrap {
    pub(super) identity: zap_core::StoreIdentity,
    pub(super) trusted: Arc<Mutex<Option<TrustedHostHandle>>>,
    pub(super) allow_milestone_accept: bool,
}

impl TrustBootstrapSource for TestBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_owner(
            CredentialId::parse("owner-domain-test")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([ControlClass::CharterActivate]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-domain-test")?,
        )?;
        let mut actions = BTreeSet::from([
            ActionClass::parse("evidence.adjudicate")?,
            ActionClass::parse("adaptive.apply")?,
            ActionClass::parse("plan.lower")?,
            ActionClass::parse("stage.accept")?,
        ]);
        if self.allow_milestone_accept {
            actions.insert(ActionClass::parse("work.accept")?);
        }
        registrar.bind_coordinator(
            CredentialId::parse("coordinator-domain-test")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                actions,
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-coordinator-domain-test")?,
        )?;
        let handle = registrar.bind_trusted_host(TrustedHostBinding {
            principal_id: PrincipalId::parse("trusted-domain-test")?,
            harness_id: zap_wire::HarnessId::parse("harness-domain-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            observation: ObservationRef::parse("observation-domain-test")?,
            allowed_events: BTreeSet::from([EventKind::parse("knowledge.source-recaptured")?]),
        })?;
        *self.trusted.lock().map_err(|_| test_error())? = Some(handle);
        Ok(())
    }
}

include!("support/admission.rs");
include!("support/harness.rs");
pub(super) fn identity() -> Result<zap_core::StoreIdentity, ZapError> {
    Ok(zap_core::StoreIdentity {
        store_id: StoreId::parse("store-domain-test")?,
        campaign_id: CampaignId::parse("campaign-domain-test")?,
        base_id: BaseId::parse("base-domain-test")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

pub(super) fn active_intent(id: &str) -> Result<IntentRecord, ZapError> {
    Ok(IntentRecord {
        intent_id: IntentId::parse(id)?,
        revision: Revision::new(1),
        previous_intent_id: None,
        summary: BoundedText::parse("Knowledge service intent")?,
        beneficiaries: vec![BoundedText::parse("Users")?],
        values: vec![BoundedText::parse("Evidence")?],
        constraints: Vec::new(),
        source_refs: Vec::new(),
        status: LifecycleStatus::Active,
        fingerprint: PayloadDigest::hash(id.as_bytes()),
        owner_binding: None,
    })
}

pub(super) fn active_outcome(id: &str, intent_id: &str) -> Result<OutcomeRecord, ZapError> {
    Ok(OutcomeRecord {
        outcome_id: OutcomeId::parse(id)?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: IntentId::parse(intent_id)?,
        summary: BoundedText::parse("Knowledge service outcome")?,
        benefits: vec![BoundedText::parse("Current evidence")?],
        guarantees: vec![BoundedText::parse("Scoped invalidation")?],
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: zap_domain::seams::CompletionDutyDisposition::Required,
        promotion_disposition: zap_domain::seams::CompletionDutyDisposition::Required,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    })
}

pub(super) fn active_charter(
    id: &str,
    intent_id: &str,
    outcome_id: &str,
) -> Result<CharterRecord, ZapError> {
    Ok(CharterRecord {
        charter_id: CharterId::parse(id)?,
        policy_id: PolicyId::parse(&format!("policy:{id}"))?,
        campaign_id: CampaignId::parse("campaign-domain-test")?,
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: IntentId::parse(intent_id)?,
        intent_digest: PayloadDigest::hash(intent_id.as_bytes()),
        expected_outcome_id: OutcomeId::parse(outcome_id)?,
        allowed_actions: Vec::new(),
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::Required,
            promotion: CharterDutyAuthority::Required,
        },
        status: LifecycleStatus::Active,
        digest: PayloadDigest::hash(id.as_bytes()),
    })
}

pub(super) fn frame<P: CommandPayload>(
    identity: &zap_core::StoreIdentity,
    payload: &P,
    expected_revision: Revision,
    basis: BasisBinding,
    command_id: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: identity.store_id.clone(),
        campaign_id: identity.campaign_id.clone(),
        base_id: identity.base_id.clone(),
        command_id: CommandId::parse(command_id)?,
        event_id: EventId::parse(&command_id.replace("command", "event"))?,
        expected_revision,
        kind: EventKind::parse(P::KIND)?,
        causes: Vec::new(),
        basis,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("domain integration test")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let encoded = payload.encode_canonical(CodecEpoch::CURRENT)?;
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, encoded.as_bytes())?;
    CanonicalCommandFrame::new(header, reason, payload)
}

fn test_index_families() -> Result<Vec<zap_core::IndexFamily>, ZapError> {
    let mut families = zap_domain::viewer_graph_index_families()?;
    families.extend(zap_core::affected_job_index_families()?);
    families.sort();
    families.dedup();
    Ok(families)
}

fn test_index_algorithms() -> Result<Vec<zap_core::IndexAlgorithm>, ZapError> {
    let mut algorithms = zap_domain::viewer_index_algorithms()?;
    algorithms.extend(zap_core::affected_job_index_algorithms()?);
    algorithms.sort();
    Ok(algorithms)
}

pub(super) fn test_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unauthorized,
        TEST_REQ,
        "test grant or state lock is unavailable",
        FixSurface::Authority,
        ErrorDetail::None,
    )
}
