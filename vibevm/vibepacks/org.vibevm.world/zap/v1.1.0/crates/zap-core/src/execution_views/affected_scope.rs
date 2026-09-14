use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    AffectedScopeDigest, AttemptId, CanonicalOutput, CodecEpoch, ContractDigest, ContractId,
    HoldId, IndependenceDigest, JobId, PayloadDigest, RelevantBasisDigest, Revision, SafeJobDigest,
    SubjectRef, WorkId, ZapError,
};

use crate::{AffectedJobView, StateReader, ValidationGeneration};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#AFFECTED-HOLD");

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#affected-scope")]
pub struct AffectedScopeRequest {
    roots: Vec<SubjectRef>,
    direct_work_ids: Vec<WorkId>,
    allow_missing_initial_work: bool,
    request_digest: PayloadDigest,
}

impl AffectedScopeRequest {
    pub fn new(
        mut roots: Vec<SubjectRef>,
        mut direct_work_ids: Vec<WorkId>,
    ) -> Result<Self, ZapError> {
        roots.sort();
        roots.dedup();
        direct_work_ids.sort();
        direct_work_ids.dedup();
        if roots.is_empty() && direct_work_ids.is_empty() {
            return Err(scope_error("affected scope request must have a typed root"));
        }
        let request_digest = canonical_digest(&(&roots, &direct_work_ids, false))?;
        Ok(Self {
            roots,
            direct_work_ids,
            allow_missing_initial_work: false,
            request_digest,
        })
    }

    pub fn initial_lowering_root(work_id: WorkId) -> Result<Self, ZapError> {
        let roots = vec![SubjectRef::Work(work_id.clone())];
        let direct_work_ids = vec![work_id];
        let request_digest = canonical_digest(&(&roots, &direct_work_ids, true))?;
        Ok(Self {
            roots,
            direct_work_ids,
            allow_missing_initial_work: true,
            request_digest,
        })
    }

    pub fn initial_milestone_plan(
        mut roots: Vec<SubjectRef>,
        mut absent_work_ids: Vec<WorkId>,
    ) -> Result<Self, ZapError> {
        roots.sort();
        roots.dedup();
        absent_work_ids.sort();
        absent_work_ids.dedup();
        if roots.is_empty() {
            return Err(scope_error(
                "initial milestone scope requires a typed outcome or obligation root",
            ));
        }
        let request_digest = canonical_digest(&(&roots, &absent_work_ids, true))?;
        Ok(Self {
            roots,
            direct_work_ids: absent_work_ids,
            allow_missing_initial_work: true,
            request_digest,
        })
    }

    pub fn roots(&self) -> &[SubjectRef] {
        &self.roots
    }

    pub fn direct_work_ids(&self) -> &[WorkId] {
        &self.direct_work_ids
    }

    pub const fn request_digest(&self) -> PayloadDigest {
        self.request_digest
    }

    pub const fn allows_missing_initial_work(&self) -> bool {
        self.allow_missing_initial_work
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#affected-scope")]
pub enum AffectedScopeCompleteness {
    Complete,
    Incomplete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#affected-scope")]
pub struct DerivedAffectedScope {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
    pub unknown_boundary: Vec<SubjectRef>,
    pub completeness: AffectedScopeCompleteness,
    pub relevant_basis: RelevantBasisDigest,
}

impl DerivedAffectedScope {
    pub fn validate(mut self) -> Result<Self, ZapError> {
        sort_unique(&mut self.affected_work_ids);
        sort_unique(&mut self.dependent_work_ids);
        sort_unique(&mut self.subjects);
        sort_unique(&mut self.unknown_boundary);
        if self
            .dependent_work_ids
            .iter()
            .any(|id| self.affected_work_ids.binary_search(id).is_ok())
            || (self.completeness == AffectedScopeCompleteness::Complete
                && !self.unknown_boundary.is_empty())
            || (self.completeness == AffectedScopeCompleteness::Incomplete
                && self.unknown_boundary.is_empty())
        {
            return Err(scope_error("derived affected closure is inconsistent"));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#affected-scope")]
pub struct AffectedScopeView {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
    pub unknown_boundary: Vec<SubjectRef>,
    pub completeness: AffectedScopeCompleteness,
    pub relevant_basis: RelevantBasisDigest,
    pub jobs: AffectedJobView,
    pub digest: AffectedScopeDigest,
}

impl AffectedScopeView {
    pub fn new(derived: DerivedAffectedScope, jobs: AffectedJobView) -> Result<Self, ZapError> {
        let derived = derived.validate()?;
        let digest = affected_scope_digest(&derived)?;
        Ok(Self {
            request_digest: derived.request_digest,
            observed_revision: derived.observed_revision,
            affected_work_ids: derived.affected_work_ids,
            dependent_work_ids: derived.dependent_work_ids,
            subjects: derived.subjects,
            unknown_boundary: derived.unknown_boundary,
            completeness: derived.completeness,
            relevant_basis: derived.relevant_basis,
            jobs,
            digest,
        })
    }

    pub fn validate(&self) -> Result<(), ZapError> {
        let derived = DerivedAffectedScope {
            request_digest: self.request_digest,
            observed_revision: self.observed_revision,
            affected_work_ids: self.affected_work_ids.clone(),
            dependent_work_ids: self.dependent_work_ids.clone(),
            subjects: self.subjects.clone(),
            unknown_boundary: self.unknown_boundary.clone(),
            completeness: self.completeness,
            relevant_basis: self.relevant_basis,
        }
        .validate()?;
        if affected_scope_digest(&derived)? != self.digest {
            return Err(scope_error(
                "affected scope digest does not match its closure",
            ));
        }
        Ok(())
    }
}

/// ```
/// use zap_core::{AffectedScopeProvider, AffectedScopeRequest, StateReader};
/// fn derive(provider: &dyn AffectedScopeProvider, state: &dyn StateReader, request: &AffectedScopeRequest) -> Result<zap_core::DerivedAffectedScope, zap_wire::ZapError> {
///     let scope = provider.derive(state, request)?;
///     assert_eq!(scope.request_digest, request.request_digest());
///     Ok(scope)
/// }
/// ```
pub trait AffectedScopeProvider: Send + Sync + 'static {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError>;

    fn assess_independence(
        &self,
        state: &dyn StateReader,
        request: &IndependenceRequest,
        candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#independence-witness"
)]
pub struct IndependenceRequest {
    hold_id: HoldId,
    held_scope: AffectedScopeDigest,
    candidate: AffectedScopeRequest,
    relevant_basis: RelevantBasisDigest,
    request_digest: PayloadDigest,
}

impl IndependenceRequest {
    pub fn new(
        hold_id: HoldId,
        held_scope: AffectedScopeDigest,
        candidate: AffectedScopeRequest,
        relevant_basis: RelevantBasisDigest,
    ) -> Result<Self, ZapError> {
        let request_digest = canonical_digest(&(
            &hold_id,
            held_scope,
            candidate.request_digest(),
            relevant_basis,
        ))?;
        Ok(Self {
            hold_id,
            held_scope,
            candidate,
            relevant_basis,
            request_digest,
        })
    }

    pub fn hold_id(&self) -> &HoldId {
        &self.hold_id
    }
    pub const fn held_scope(&self) -> AffectedScopeDigest {
        self.held_scope
    }
    pub fn candidate(&self) -> &AffectedScopeRequest {
        &self.candidate
    }
    pub const fn relevant_basis(&self) -> RelevantBasisDigest {
        self.relevant_basis
    }
    pub const fn request_digest(&self) -> PayloadDigest {
        self.request_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#independence-witness"
)]
pub struct IndependenceView {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub hold_id: HoldId,
    pub held_scope: AffectedScopeDigest,
    pub candidate_request_digest: PayloadDigest,
    pub candidate_scope: AffectedScopeDigest,
    pub independent: bool,
    pub unknown_boundary: Vec<SubjectRef>,
    pub relevant_basis: RelevantBasisDigest,
    pub digest: IndependenceDigest,
}

impl IndependenceView {
    pub fn new(
        request: &IndependenceRequest,
        observed_revision: Revision,
        candidate_scope: AffectedScopeDigest,
        independent: bool,
        mut unknown_boundary: Vec<SubjectRef>,
    ) -> Result<Self, ZapError> {
        sort_unique(&mut unknown_boundary);
        let mut view = Self {
            request_digest: request.request_digest(),
            observed_revision,
            hold_id: request.hold_id().clone(),
            held_scope: request.held_scope(),
            candidate_request_digest: request.candidate().request_digest(),
            candidate_scope,
            independent,
            unknown_boundary,
            relevant_basis: request.relevant_basis(),
            digest: IndependenceDigest::hash(&[]),
        };
        view.digest = IndependenceDigest::hash(
            CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &(
                    view.request_digest,
                    &view.hold_id,
                    view.held_scope,
                    view.candidate_request_digest,
                    view.candidate_scope,
                    view.independent,
                    &view.unknown_boundary,
                    view.relevant_basis,
                ),
            )?
            .as_bytes(),
        );
        Ok(view)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#safe-job-witness")]
pub struct SafeJobRequest {
    hold_id: HoldId,
    expected_scope: AffectedScopeDigest,
    current_scope: AffectedScopeRequest,
    mode: SafeJobValidationMode,
    held_jobs: Vec<HeldJobIdentity>,
    request_digest: PayloadDigest,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#safe-job-witness")]
pub enum SafeJobValidationMode {
    #[default]
    ExactScope,
    HeldExecutions,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#safe-job-witness")]
pub struct HeldJobIdentity {
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
}

impl HeldJobIdentity {
    pub fn from_observation(row: &crate::WorkExecutionObservationRecord) -> Self {
        Self {
            job_id: row.job_id.clone(),
            attempt_id: row.attempt_id.clone(),
            work_id: row.work_id.clone(),
            contract_id: row.contract_id.clone(),
            contract_digest: row.contract_digest,
            validation_generation: row.validation_generation,
        }
    }

    pub fn matches(&self, row: &crate::WorkExecutionObservationRecord) -> bool {
        self.job_id == row.job_id
            && self.attempt_id == row.attempt_id
            && self.work_id == row.work_id
            && self.contract_id == row.contract_id
            && self.contract_digest == row.contract_digest
            && self.validation_generation == row.validation_generation
    }
}

impl SafeJobRequest {
    pub fn new(
        hold_id: HoldId,
        expected_scope: AffectedScopeDigest,
        current_scope: AffectedScopeRequest,
    ) -> Result<Self, ZapError> {
        let request_digest =
            canonical_digest(&(&hold_id, expected_scope, current_scope.request_digest()))?;
        Ok(Self {
            hold_id,
            expected_scope,
            current_scope,
            mode: SafeJobValidationMode::ExactScope,
            held_jobs: Vec::new(),
            request_digest,
        })
    }
    pub fn held_executions(
        hold_id: HoldId,
        expected_scope: AffectedScopeDigest,
        current_scope: AffectedScopeRequest,
        mut held_jobs: Vec<HeldJobIdentity>,
    ) -> Result<Self, ZapError> {
        held_jobs.sort();
        if held_jobs
            .windows(2)
            .any(|pair| pair[0].job_id == pair[1].job_id)
        {
            return Err(scope_error(
                "held execution identities must be unique by job",
            ));
        }
        let request_digest = canonical_digest(&(
            &hold_id,
            expected_scope,
            current_scope.request_digest(),
            SafeJobValidationMode::HeldExecutions,
            &held_jobs,
        ))?;
        Ok(Self {
            hold_id,
            expected_scope,
            current_scope,
            mode: SafeJobValidationMode::HeldExecutions,
            held_jobs,
            request_digest,
        })
    }
    pub fn hold_id(&self) -> &HoldId {
        &self.hold_id
    }
    pub const fn expected_scope(&self) -> AffectedScopeDigest {
        self.expected_scope
    }
    pub fn current_scope(&self) -> &AffectedScopeRequest {
        &self.current_scope
    }
    pub const fn mode(&self) -> SafeJobValidationMode {
        self.mode
    }
    pub fn held_jobs(&self) -> &[HeldJobIdentity] {
        &self.held_jobs
    }
    pub const fn request_digest(&self) -> PayloadDigest {
        self.request_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#safe-job-witness")]
pub struct SafeJobView {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub hold_id: HoldId,
    #[serde(default)]
    pub mode: SafeJobValidationMode,
    #[serde(default)]
    pub held_jobs: Vec<HeldJobIdentity>,
    pub scope: AffectedScopeView,
    pub job_ids: Vec<JobId>,
    pub all_safe: bool,
    pub digest: SafeJobDigest,
}

impl SafeJobView {
    pub fn new(
        request: &SafeJobRequest,
        observed_revision: Revision,
        scope: AffectedScopeView,
        mut job_ids: Vec<JobId>,
        all_safe: bool,
    ) -> Result<Self, ZapError> {
        sort_unique(&mut job_ids);
        let digest_body = match request.mode() {
            SafeJobValidationMode::ExactScope => CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &(request.request_digest(), &scope, &job_ids, all_safe),
            )?,
            SafeJobValidationMode::HeldExecutions => CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &(
                    request.request_digest(),
                    request.mode(),
                    request.held_jobs(),
                    &scope,
                    &job_ids,
                    all_safe,
                ),
            )?,
        };
        let digest = SafeJobDigest::hash(digest_body.as_bytes());
        Ok(Self {
            request_digest: request.request_digest(),
            observed_revision,
            hold_id: request.hold_id().clone(),
            mode: request.mode(),
            held_jobs: request.held_jobs().to_vec(),
            scope,
            job_ids,
            all_safe,
            digest,
        })
    }
}

#[derive(Clone)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#independence-witness"
)]
pub struct IndependenceWitness {
    view: IndependenceView,
    pub(crate) transaction_seal: Arc<()>,
    pub(crate) service_seal: Arc<()>,
}

impl IndependenceWitness {
    pub(crate) fn new(
        view: IndependenceView,
        transaction_seal: Arc<()>,
        service_seal: Arc<()>,
    ) -> Self {
        Self {
            view,
            transaction_seal,
            service_seal,
        }
    }
    pub fn view(&self) -> &IndependenceView {
        &self.view
    }
}

#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#safe-job-witness")]
pub struct SafeJobWitness {
    view: SafeJobView,
    pub(crate) transaction_seal: Arc<()>,
    pub(crate) service_seal: Arc<()>,
}

impl SafeJobWitness {
    pub(crate) fn new(view: SafeJobView, transaction_seal: Arc<()>, service_seal: Arc<()>) -> Self {
        Self {
            view,
            transaction_seal,
            service_seal,
        }
    }
    pub fn view(&self) -> &SafeJobView {
        &self.view
    }
}

fn affected_scope_digest(derived: &DerivedAffectedScope) -> Result<AffectedScopeDigest, ZapError> {
    let bytes = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &(
            derived.request_digest,
            &derived.affected_work_ids,
            &derived.dependent_work_ids,
            &derived.subjects,
            &derived.unknown_boundary,
            derived.completeness,
            derived.relevant_basis,
        ),
    )?;
    Ok(AffectedScopeDigest::hash(bytes.as_bytes()))
}

fn canonical_digest(value: &impl Serialize) -> Result<PayloadDigest, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?.digest())
}

fn sort_unique<T: Ord>(values: &mut Vec<T>) {
    values.sort();
    values.dedup();
}

fn scope_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#AFFECTED-HOLD",
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}
