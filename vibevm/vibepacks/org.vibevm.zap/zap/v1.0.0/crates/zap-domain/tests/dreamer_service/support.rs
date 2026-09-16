use std::collections::BTreeSet;
use std::ops::Bound;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use zap_core::*;
use zap_domain::economics::{
    ChangeControlAdmissionProvider, DomainActionImpactProvider, DomainAffectedScopeProvider,
};
use zap_store::RedbStore;
use zap_wire::*;

pub const REMOVAL_SEED_KIND: &str = "test.dream-removal-seed";
pub const JOB_MUTATION_KIND: &str = "test.dream-job-mutation";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalSeed {
    pub strategy: zap_domain::lowering::StrategicPlanRecord,
    pub work: Vec<zap_domain::control::WorkRecord>,
    pub evidence: Vec<zap_domain::acceptance::EvidenceAdjudicationRecord>,
    pub candidates: Vec<CandidateProvenanceInput>,
    pub deferrals: Vec<zap_domain::control::DeferralRecord>,
    pub jobs: Vec<WorkExecutionObservationRecord>,
}

impl CanonicalEncode for RemovalSeed {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RemovalSeed {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for RemovalSeed {
    const KIND: &'static str = REMOVAL_SEED_KIND;
}

struct RemovalSeedCell;

impl TransitionCell for RemovalSeedCell {
    type Payload = RemovalSeed;
    type Output = zap_domain::seams::DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = vec![
            RecordFamily::parse(zap_domain::lowering::StrategicPlanRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::control::WorkRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::acceptance::EvidenceAdjudicationRecord::FAMILY)?,
            RecordFamily::parse(CandidateProvenanceRecord::FAMILY)?,
            RecordFamily::parse(zap_domain::control::DeferralRecord::FAMILY)?,
            RecordFamily::parse(WorkExecutionObservationRecord::FAMILY)?,
        ];
        affected_records.sort();
        let mut affected_indexes =
            zap_domain::viewer_index_families_for_records(&affected_records)?;
        affected_indexes.extend(zap_core::affected_job_index_families_for_records(
            &affected_records,
        )?);
        affected_indexes.sort();
        affected_indexes.dedup();
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(REMOVAL_SEED_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let current = state
            .get_typed::<zap_domain::lowering::StrategicPlanRecord>(
                &command.payload().strategy.strategic_revision_id,
            )?
            .ok_or_else(|| test_error("seed strategy source missing"))?;
        changes.replace(current.revision, command.payload().strategy.clone())?;
        for row in &command.payload().work {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().evidence {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().candidates {
            changes.insert(CandidateProvenanceRecord::new(row.clone())?)?;
        }
        for row in &command.payload().deferrals {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().jobs {
            changes.insert(row.clone())?;
        }
        Ok(zap_domain::seams::DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mutation", rename_all = "snake_case")]
pub enum JobMutation {
    Insert {
        record: WorkExecutionObservationRecord,
    },
    Replace {
        record: WorkExecutionObservationRecord,
    },
    Remove {
        job_id: JobId,
        expected: Revision,
    },
}

impl CanonicalEncode for JobMutation {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for JobMutation {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for JobMutation {
    const KIND: &'static str = JOB_MUTATION_KIND;
}

struct JobMutationCell;

impl TransitionCell for JobMutationCell {
    type Payload = JobMutation;
    type Output = zap_domain::seams::DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let affected_records = vec![RecordFamily::parse(WorkExecutionObservationRecord::FAMILY)?];
        let affected_indexes =
            zap_core::affected_job_index_families_for_records(&affected_records)?;
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(JOB_MUTATION_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SAFE-DRAIN",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        match command.payload() {
            JobMutation::Insert { record } => changes.insert(record.clone())?,
            JobMutation::Replace { record } => {
                let current = state
                    .get_typed::<WorkExecutionObservationRecord>(&record.job_id)?
                    .ok_or_else(|| test_error("job replacement source missing"))?;
                changes.replace(current.revision, record.clone())?;
            }
            JobMutation::Remove { job_id, expected } => {
                changes.remove::<WorkExecutionObservationRecord>(job_id.clone(), *expected)?;
            }
        }
        Ok(zap_domain::seams::DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

pub struct CurrentAffectedJobs;

impl AffectedJobProvider for CurrentAffectedJobs {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &AffectedJobRequest,
    ) -> Result<AffectedJobView, ZapError> {
        let rows = state
            .scan_typed::<WorkExecutionObservationRecord>(
                KeyRange {
                    start: Bound::Unbounded,
                    end: Bound::Unbounded,
                },
                PageLimit::within(1_000, 1_000)?,
            )?
            .items
            .into_iter()
            .filter(|row| {
                (request.input.work_ids.binary_search(&row.work_id).is_ok()
                    || row
                        .subjects
                        .iter()
                        .any(|subject| request.input.subjects.binary_search(subject).is_ok()))
                    && (!row.execution.is_terminal()
                        || matches!(row.effect, EffectState::Started | EffectState::Unknown))
            })
            .collect();
        AffectedJobView::new(
            request.digest,
            state.revision(),
            rows,
            AffectedJobCompleteness::Complete,
        )
    }
}

struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == b"dreamer-test-secret"
    }
}

struct Bootstrap {
    identity: StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl TrustBootstrapSource for Bootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_owner(
            CredentialId::parse("owner-dreamer-test")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ControlClass::CampaignStop,
                    ControlClass::CharterActivate,
                    ControlClass::ChangeDecisionRecord,
                    ControlClass::CombinedCharterChangeDecision,
                    ControlClass::PauseResume,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-dreamer-test")?,
        )?;
        registrar.bind_owner(
            CredentialId::parse("owner-dreamer-charter-only")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([ControlClass::CharterAmend]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-dreamer-charter-only")?,
        )?;
        registrar.bind_owner(
            CredentialId::parse("owner-dreamer-decision-only")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([ControlClass::ChangeDecisionRecord]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-dreamer-decision-only")?,
        )?;
        registrar.bind_coordinator(
            CredentialId::parse("coordinator-dreamer-test")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ActionClass::parse("outcome.adopt")?,
                    ActionClass::parse("plan.lower")?,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-coordinator-dreamer-test")?,
        )?;
        let internal = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal-dreamer-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([
                EventKind::parse("dreamer.grill-completed")?,
                EventKind::parse("dreamer.recalculated")?,
                EventKind::parse("economics.baseline-established")?,
                EventKind::parse("economics.change-admission-prepared")?,
                EventKind::parse("economics.change-assessment-adjudicated")?,
                EventKind::parse("economics.change-hold-resolved")?,
                EventKind::parse(REMOVAL_SEED_KIND)?,
                EventKind::parse(JOB_MUTATION_KIND)?,
            ]),
        })?;
        self.internal
            .set(internal)
            .map_err(|_| test_error("internal handle initialized twice"))?;
        let trusted = registrar.bind_trusted_host(TrustedHostBinding {
            principal_id: PrincipalId::parse("trusted-dreamer-test")?,
            harness_id: HarnessId::parse("harness-dreamer-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            observation: ObservationRef::parse("observation-dreamer-test")?,
            allowed_events: BTreeSet::from([
                EventKind::parse("knowledge.source-recorded")?,
                EventKind::parse("dreamer.fact-answered")?,
            ]),
        })?;
        self.trusted
            .set(trusted)
            .map_err(|_| test_error("trusted handle initialized twice"))?;
        let data = registrar.bind_agent_data(AgentDataBinding {
            principal_id: PrincipalId::parse("data-dreamer-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            allowed_events: BTreeSet::from([
                EventKind::parse("control.charter-drafted")?,
                EventKind::parse("domain.intent-proposed")?,
                EventKind::parse("domain.outcome-proposed")?,
                EventKind::parse("economics.change-assessment-proposed")?,
                EventKind::parse("planning.strategy-proposed")?,
                EventKind::parse("dreamer.exploration-started")?,
                EventKind::parse("dreamer.scope-change-requested")?,
                EventKind::parse("dreamer.grill-question-saved")?,
            ]),
        })?;
        self.data
            .set(data)
            .map_err(|_| test_error("data issuer initialized twice"))
    }
}

pub struct Harness {
    pub service: CommitService<RedbStore>,
    pub store: RedbStore,
    pub identity: StoreIdentity,
    pub cells: CellSet,
    pub records: RecordSet,
    pub admission: Arc<ChangeControlAdmissionProvider>,
    owner: AuthenticatedPrincipal,
    coordinator: AuthenticatedPrincipal,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl Harness {
    pub fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
        let cells = CellSet::compose([
            zap_domain::cell_set()?,
            CellSet::single(RemovalSeedCell)?,
            CellSet::single(JobMutationCell)?,
        ])?;
        let routes = RouteRegistry::compose([
            zap_domain::route_set()?,
            RouteRegistry::single(
                EventKind::parse(REMOVAL_SEED_KIND)?,
                RouteClass::ServiceInternal,
            ),
            RouteRegistry::single(
                EventKind::parse(JOB_MUTATION_KIND)?,
                RouteClass::ServiceInternal,
            ),
        ])?;
        let store = RedbStore::create(path, identity.clone())?
            .with_records(records.clone(), QueryEpoch::new(1)?);
        let mut index_families = zap_domain::viewer_graph_index_families()?;
        index_families.extend(zap_core::affected_job_index_families()?);
        index_families.extend(zap_runtime::runtime_index_families()?);
        index_families.sort();
        index_families.dedup();
        let mut index_algorithms = zap_domain::viewer_index_algorithms()?;
        index_algorithms.extend(zap_core::affected_job_index_algorithms()?);
        index_algorithms.extend(zap_runtime::runtime_index_algorithms()?);
        index_algorithms.sort();
        index_algorithms.dedup();
        store.rebuild_indexes_v2(index_families, index_algorithms, Revision::GENESIS)?;
        let internal = Arc::new(OnceLock::new());
        let trusted = Arc::new(OnceLock::new());
        let data = Arc::new(OnceLock::new());
        let admission = Arc::new(ChangeControlAdmissionProvider::new()?);
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(Bootstrap {
                identity: identity.clone(),
                internal: internal.clone(),
                trusted: trusted.clone(),
                data: data.clone(),
            }),
        )
        .cells(cells.clone())
        .records(records.clone())
        .queries(zap_domain::query_set()?)
        .routes(routes)
        .basis_provider(Arc::new(zap_domain::knowledge::DomainBasisProvider))
        .action_impact_provider(Arc::new(DomainActionImpactProvider))
        .action_admission_provider(admission.clone())
        .affected_scope_provider(Arc::new(DomainAffectedScopeProvider))
        .affected_job_provider(Arc::new(CurrentAffectedJobs))
        .build()?;
        let owner = service.credential_authority().authenticate(
            &CredentialId::parse("owner-dreamer-test")?,
            SecretInput::new(b"dreamer-test-secret"),
            &identity.campaign_id,
        )?;
        let coordinator = service.credential_authority().authenticate(
            &CredentialId::parse("coordinator-dreamer-test")?,
            SecretInput::new(b"dreamer-test-secret"),
            &identity.campaign_id,
        )?;
        Ok(Self {
            service,
            store,
            identity,
            cells,
            records,
            admission,
            owner,
            coordinator,
            internal,
            trusted,
            data,
        })
    }

    pub fn data<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        basis: BasisBinding,
        command: &str,
    ) -> Result<CommitReceipt, ZapError> {
        let frame = frame(&self.identity, payload, self.store.head()?, basis, command)?;
        let grant = self
            .data
            .get()
            .ok_or_else(|| test_error("data issuer missing"))?
            .authorize(&frame)?;
        self.service
            .execute(PrincipalContext::AgentData(&grant), frame)
    }

    pub fn owner<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        command: &str,
    ) -> Result<CommitReceipt, ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            self.store.head()?,
            BasisBinding::NotApplicable,
            command,
        )?;
        self.service
            .execute(PrincipalContext::Credentialed(&self.owner), frame)
    }

    pub fn internal<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        basis: BasisBinding,
        command: &str,
    ) -> Result<CommitReceipt, ZapError> {
        let frame = frame(&self.identity, payload, self.store.head()?, basis, command)?;
        let permit = self
            .internal
            .get()
            .ok_or_else(|| test_error("internal handle missing"))?
            .authorize(&frame, OperationId::parse(&format!("operation:{command}"))?)?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)
    }

    pub fn trusted<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        command: &str,
    ) -> Result<CommitReceipt, ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            self.store.head()?,
            BasisBinding::NotApplicable,
            command,
        )?;
        let grant = self
            .trusted
            .get()
            .ok_or_else(|| test_error("trusted handle missing"))?
            .authorize(
                &frame,
                OperationRef::Command(frame.header().command_id().clone()),
            )?;
        self.service
            .execute(PrincipalContext::TrustedObservation(&grant), frame)
    }

    pub fn privileged_frame<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        basis: BasisBinding,
        command: &str,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        frame(&self.identity, payload, self.store.head()?, basis, command)
    }

    pub fn execute_privileged(
        &self,
        frame: CanonicalCommandFrame,
    ) -> Result<CommitReceipt, ZapError> {
        self.service
            .execute(PrincipalContext::Credentialed(&self.coordinator), frame)
    }
}

pub fn frame<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    payload: &P,
    revision: Revision,
    basis: BasisBinding,
    command: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse(command)?,
            event_id: EventId::parse(&format!("event:{command}"))?,
            expected_revision: revision,
            kind: EventKind::parse(P::KIND)?,
            causes: Vec::new(),
            basis,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Focused Dreamer service scenario")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-dreamer-test")?,
        campaign_id: CampaignId::parse("campaign-dreamer-test")?,
        base_id: BaseId::parse("base-dreamer-test")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

pub fn test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-EXACT-APPLICATION",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
