use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_wire::{
    AttemptId, BoundedText, DeferralId, EvidenceId, JobId, ObligationId, OutcomeId,
    ReconciliationRequestId, ReviewId, Revision, WorkId,
};

use crate::control::{TaskContractRecord, WorkRecord};
use crate::seams::{ObligationAssignment, WorkState, impl_canonical, schema_tag};

schema_tag!(
    ContractReplacedSchema,
    "zap-domain/task-contract-replaced/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#task-contract"
);
schema_tag!(
    RevalidationReadiedSchema,
    "zap-domain/work-revalidation-readied/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#revalidation-ready"
);
schema_tag!(
    WorkRenamedSchema,
    "zap-domain/work-renamed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions"
);
schema_tag!(
    WorkTransitionedSchema,
    "zap-domain/work-transitioned/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions"
);
schema_tag!(
    WorkDispatchedSchema,
    "zap-domain/work-dispatched/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions"
);
schema_tag!(
    PlanLoweredSchema,
    "zap-domain/plan-lowered/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage"
);
schema_tag!(
    DeferralCreatedSchema,
    "zap-domain/deferral-created/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle"
);
schema_tag!(
    DeferralTransferredSchema,
    "zap-domain/deferral-transferred/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle"
);
schema_tag!(
    DeferralClosedSchema,
    "zap-domain/deferral-closed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle"
);
schema_tag!(
    DeferralInapplicableSchema,
    "zap-domain/deferral-inapplicable/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#PLAN-RECONCILIATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#task-contract")]
pub struct TaskContractReplaced {
    pub schema: ContractReplacedSchema,
    pub work_id: WorkId,
    pub expected_version: Revision,
    pub contract: TaskContractRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#PLAN-RECONCILIATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#revalidation-ready")]
pub struct WorkRevalidationReadied {
    pub schema: RevalidationReadiedSchema,
    pub work_id: WorkId,
    pub review_id: ReviewId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub request_id: ReconciliationRequestId,
    pub from_generation: u64,
    pub expected_state: WorkState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#PLAN-RECONCILIATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions")]
pub struct WorkRenamed {
    pub schema: WorkRenamedSchema,
    pub work_id: WorkId,
    pub expected_title: BoundedText<4096>,
    pub new_title: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#PLAN-RECONCILIATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions")]
pub struct WorkTransitioned {
    pub schema: WorkTransitionedSchema,
    pub work_id: WorkId,
    pub from_state: WorkState,
    pub to_state: WorkState,
    pub successor_ids: Vec<WorkId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#LIVE-WORK-RECONCILIATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions")]
pub struct WorkDispatched {
    pub schema: WorkDispatchedSchema,
    pub work_id: WorkId,
    pub from_state: WorkState,
    pub job_id: JobId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub struct PlanLowered {
    pub schema: PlanLoweredSchema,
    pub parent_id: WorkId,
    pub nodes: Vec<WorkRecord>,
    pub coverage: Vec<ObligationAssignment>,
    pub contracts: Vec<TaskContractRecord>,
    pub integration_owner: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle")]
pub struct DeferralCreated {
    pub schema: DeferralCreatedSchema,
    pub deferral_id: DeferralId,
    pub outcome_id: OutcomeId,
    pub obligation_ids: Vec<ObligationId>,
    pub work_ids: Vec<WorkId>,
    pub scope: BoundedText<4096>,
    pub reason: BoundedText<4096>,
    pub current_guarantees: Vec<BoundedText<4096>>,
    pub responsible_party: BoundedText<4096>,
    pub closure_requirement: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle")]
pub struct DeferralTransferred {
    pub schema: DeferralTransferredSchema,
    pub deferral_id: DeferralId,
    pub from_responsible_party: BoundedText<4096>,
    pub to_responsible_party: BoundedText<4096>,
    pub to_work_ids: Vec<WorkId>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle")]
pub struct DeferralClosed {
    pub schema: DeferralClosedSchema,
    pub deferral_id: DeferralId,
    pub evidence_ids: Vec<EvidenceId>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle")]
pub struct DeferralInapplicable {
    pub schema: DeferralInapplicableSchema,
    pub deferral_id: DeferralId,
    pub outcome_id: OutcomeId,
    pub reason: BoundedText<4096>,
}

impl_canonical!(TaskContractReplaced);
impl_canonical!(WorkRevalidationReadied);
impl_canonical!(WorkRenamed);
impl_canonical!(WorkTransitioned);
impl_canonical!(WorkDispatched);
impl_canonical!(PlanLowered);
impl_canonical!(DeferralCreated);
impl_canonical!(DeferralTransferred);
impl_canonical!(DeferralClosed);
impl_canonical!(DeferralInapplicable);
