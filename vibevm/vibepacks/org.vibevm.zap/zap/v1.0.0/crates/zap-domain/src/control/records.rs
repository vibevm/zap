use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, ContractDigest, ContractId, DeferralId, EvidenceId, JobId, ObligationId,
    OutcomeId, Revision, WorkId,
};

use crate::seams::{
    DeferralStatus, MaturityStage, ObligationDisposition, ObligationOwner, ObligationStatus,
    TaskContract, WorkKind, WorkState, WorkType, impl_canonical, impl_stored_record,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub struct ObligationRecord {
    pub obligation_id: ObligationId,
    pub created_for_outcome: OutcomeId,
    pub current_outcomes: Vec<OutcomeId>,
    pub statement: BoundedText<4096>,
    pub essential: bool,
    pub owners: Vec<ObligationOwner>,
    pub status: ObligationStatus,
    pub disposition: ObligationDisposition,
    pub successors: Vec<ObligationId>,
    pub unmet_portion: Option<BoundedText<4096>>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#ORTHOGONAL-AXES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub struct WorkRecord {
    pub work_id: WorkId,
    pub parent_id: Option<WorkId>,
    pub title: BoundedText<4096>,
    pub kind: WorkKind,
    pub work_type: WorkType,
    pub state: WorkState,
    pub order: u32,
    pub depends_on: Vec<WorkId>,
    pub acceptance: Vec<BoundedText<4096>>,
    pub required_stage: MaturityStage,
    pub validation_generation: u64,
    pub active_job: Option<JobId>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONTRACT")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#task-contract")]
pub struct TaskContractRecord {
    pub contract_id: ContractId,
    pub work_id: WorkId,
    pub version: Revision,
    pub contract_digest: ContractDigest,
    pub active: bool,
    pub contract: TaskContract,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle")]
pub struct DeferralRecord {
    pub deferral_id: DeferralId,
    pub outcome_id: OutcomeId,
    pub obligation_ids: Vec<ObligationId>,
    pub work_ids: Vec<WorkId>,
    pub scope: BoundedText<4096>,
    pub reason: BoundedText<4096>,
    pub current_guarantees: Vec<BoundedText<4096>>,
    pub responsible_party: BoundedText<4096>,
    pub closure_requirement: BoundedText<4096>,
    pub status: DeferralStatus,
    pub closure_evidence: Vec<EvidenceId>,
    pub revision: Revision,
}

impl_canonical!(ObligationRecord);
impl_canonical!(WorkRecord);
impl_canonical!(TaskContractRecord);
impl_canonical!(DeferralRecord);
impl_stored_record!(
    ObligationRecord,
    ObligationId,
    obligation_id,
    revision,
    "zap.domain.obligation",
    crate::viewer_indexes::obligation_index_rows
);
impl_stored_record!(
    WorkRecord,
    WorkId,
    work_id,
    revision,
    "zap.domain.work",
    crate::viewer_indexes::work_index_rows
);
impl_stored_record!(
    TaskContractRecord,
    ContractId,
    contract_id,
    version,
    "zap.domain.contract",
    crate::viewer_indexes::contract_index_rows
);
impl_stored_record!(
    DeferralRecord,
    DeferralId,
    deferral_id,
    revision,
    "zap.domain.deferral"
);
