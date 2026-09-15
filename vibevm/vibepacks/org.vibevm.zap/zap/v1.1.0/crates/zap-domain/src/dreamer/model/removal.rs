specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{ArtifactDigest, DeferralId, EvidenceId, JobId, ObligationId, WorkId};

use crate::seams::MaturityStage;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct ObligationRemovalDisposition {
    pub obligation_id: ObligationId,
    pub successor_work_id: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct DependentRemovalDisposition {
    pub dependent_work_id: WorkId,
    pub replacement_prerequisite_id: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct EvidenceRemovalDisposition {
    pub evidence_id: EvidenceId,
    pub disposition: DreamRetention,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct ArtifactRemovalDisposition {
    pub artifact: ArtifactDigest,
    pub disposition: DreamRetention,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub enum DreamRetention {
    Retain,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct StageDebtRemovalDisposition {
    pub lowering_id: zap_wire::LoweringId,
    pub stage: MaturityStage,
    pub successor_work_id: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct DeferralRemovalDisposition {
    pub deferral_id: DeferralId,
    pub successor_work_id: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct ExternalEffectRemovalDisposition {
    pub job_id: JobId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-REMOVAL-CONSERVATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-removal")]
pub struct DreamRemovalPlan {
    pub removed_work_id: WorkId,
    pub obligations: Vec<ObligationRemovalDisposition>,
    pub dependents: Vec<DependentRemovalDisposition>,
    pub evidence: Vec<EvidenceRemovalDisposition>,
    pub artifacts: Vec<ArtifactRemovalDisposition>,
    pub stage_debt: Vec<StageDebtRemovalDisposition>,
    pub deferrals: Vec<DeferralRemovalDisposition>,
    pub external_effects: Vec<ExternalEffectRemovalDisposition>,
}
