specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ArtifactDigest, BoundedText, EvidenceId, FactId, ObservationRef, RelevantBasisDigest, ReviewId,
    Revision, SourceDigest, SourceId, SubjectRef, WorkId,
};

use crate::knowledge::{
    ClosureStatus, DependencyRelation, EpistemicStatus, FactAcceptanceStatus, FactOrigin,
    KnowledgeEdgeId, KnowledgeEndpoint, RegionId, RegionRelevance, RegionState,
    SemanticAssessmentRecord, SourceCaptureStatus, SourceKind, SourceScope,
};
use crate::seams::{impl_canonical, schema_tag};

schema_tag!(
    SourceRecordedSchema,
    "zap-domain/source-recorded/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture"
);
schema_tag!(
    NativeFactsRecordedSchema,
    "zap-domain/native-facts-recorded/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle"
);
schema_tag!(
    SourceRecapturedSchema,
    "zap-domain/source-recaptured/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture"
);
schema_tag!(
    SourceObservationProposedSchema,
    "zap-domain/source-observation-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-observations"
);
schema_tag!(
    SourceObservedSchema,
    "zap-domain/source-observed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-observations"
);
schema_tag!(
    DependencyRecordedSchema,
    "zap-domain/dependency-recorded/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations"
);
schema_tag!(
    ClosureAssessedSchema,
    "zap-domain/closure-assessed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
);
schema_tag!(
    ApplicabilityAssessedSchema,
    "zap-domain/applicability-assessed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
);
schema_tag!(
    RegionTransitionedSchema,
    "zap-domain/region-transitioned/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions"
);
schema_tag!(
    RegionRelevanceSetSchema,
    "zap-domain/region-relevance-set/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions"
);
schema_tag!(
    RegionSplitSchema,
    "zap-domain/region-split/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions"
);
schema_tag!(
    RegionMergedSchema,
    "zap-domain/region-merged/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions"
);
schema_tag!(
    SemanticAssessmentProposedSchema,
    "zap-domain/semantic-assessment-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#semantic-assessment"
);
schema_tag!(
    FactProposedSchema,
    "zap-domain/fact-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle"
);
schema_tag!(
    FactAdjudicatedSchema,
    "zap-domain/fact-adjudicated/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle"
);
schema_tag!(
    ReviewProposedSchema,
    "zap-domain/review-proposed/2",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review"
);
schema_tag!(
    ReviewAppliedSchema,
    "zap-domain/review-applied/2",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub struct SourceCaptureInput {
    pub source_id: SourceId,
    pub source_kind: SourceKind,
    pub locator: BoundedText<4096>,
    pub content_digest: SourceDigest,
    pub byte_len: u64,
    pub scope: SourceScope,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub struct SourceRecorded {
    pub schema: SourceRecordedSchema,
    pub source: SourceCaptureInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub struct NativeFactInput {
    pub fact_id: FactId,
    pub origin: FactOrigin,
    pub statement: BoundedText<16384>,
    pub address: BoundedText<4096>,
    pub normative_status: Option<BoundedText<256>>,
    pub subject_refs: Vec<SubjectRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub struct NativeFactsRecorded {
    pub schema: NativeFactsRecordedSchema,
    pub source: SourceCaptureInput,
    pub facts: Vec<NativeFactInput>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub struct SourceRecaptured {
    pub schema: SourceRecapturedSchema,
    pub previous_digest: SourceDigest,
    pub source: SourceCaptureInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-observations")]
pub struct SourceObservation {
    pub source_id: SourceId,
    pub status: SourceCaptureStatus,
    pub digest: Option<SourceDigest>,
    pub byte_len: Option<u64>,
    pub detail: Option<BoundedText<4096>>,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-observations")]
pub struct SourceObservationProposed {
    pub schema: SourceObservationProposedSchema,
    pub observed: SourceObservation,
    pub claim: BoundedText<4096>,
    pub artifacts: Vec<ArtifactDigest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-observations")]
pub struct SourceObserved {
    pub schema: SourceObservedSchema,
    pub observed: SourceObservation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations")]
pub struct DependencyRecorded {
    pub schema: DependencyRecordedSchema,
    pub edge_id: KnowledgeEdgeId,
    pub prerequisite: KnowledgeEndpoint,
    pub dependent: KnowledgeEndpoint,
    pub relation: DependencyRelation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub struct ClosureAssessed {
    pub schema: ClosureAssessedSchema,
    pub subject: KnowledgeEndpoint,
    pub basis_subjects: Vec<SubjectRef>,
    pub status: ClosureStatus,
    pub boundary: Vec<KnowledgeEndpoint>,
    pub missing: Vec<KnowledgeEndpoint>,
    pub evidence_refs: Vec<EvidenceId>,
    pub basis: RelevantBasisDigest,
    pub expected_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub struct ApplicabilityAssessed {
    pub schema: ApplicabilityAssessedSchema,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub status: crate::knowledge::SourceApplicabilityStatus,
    pub scope: SourceScope,
    pub evidence_refs: Vec<EvidenceId>,
    pub closure_status: ClosureStatus,
    pub basis: RelevantBasisDigest,
    pub expected_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub struct RegionTransitioned {
    pub schema: RegionTransitionedSchema,
    pub region_id: RegionId,
    pub subject_refs: Vec<SubjectRef>,
    pub work_refs: Vec<WorkId>,
    pub from: RegionState,
    pub to: RegionState,
    pub evidence_refs: Vec<EvidenceId>,
    pub basis: RelevantBasisDigest,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub struct RegionRelevanceSet {
    pub schema: RegionRelevanceSetSchema,
    pub region_id: RegionId,
    pub subject_refs: Vec<SubjectRef>,
    pub work_refs: Vec<WorkId>,
    pub relevance: RegionRelevance,
    pub evidence_refs: Vec<EvidenceId>,
    pub basis: RelevantBasisDigest,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub struct NewRegion {
    pub region_id: RegionId,
    pub subject_refs: Vec<SubjectRef>,
    pub question: BoundedText<4096>,
    pub work_refs: Vec<WorkId>,
    pub relevance: RegionRelevance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub struct RegionSplit {
    pub schema: RegionSplitSchema,
    pub region_id: RegionId,
    pub children: Vec<NewRegion>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub struct RegionMerged {
    pub schema: RegionMergedSchema,
    pub region_ids: Vec<RegionId>,
    pub merged: NewRegion,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#semantic-assessment")]
pub struct SemanticAssessmentProposed {
    pub schema: SemanticAssessmentProposedSchema,
    pub assessment: SemanticAssessmentRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub struct FactProposed {
    pub schema: FactProposedSchema,
    pub fact_id: FactId,
    pub origin: FactOrigin,
    pub statement: BoundedText<16384>,
    pub address: BoundedText<4096>,
    pub subject_refs: Vec<SubjectRef>,
    pub source_refs: Vec<SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub struct FactAdjudicated {
    pub schema: FactAdjudicatedSchema,
    pub fact_id: FactId,
    pub expected_revision: Revision,
    pub subject_refs: Vec<SubjectRef>,
    pub epistemic_status: EpistemicStatus,
    pub acceptance_status: FactAcceptanceStatus,
    pub evidence_refs: Vec<EvidenceId>,
    pub source_refs: Vec<SourceId>,
    pub basis: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct ReviewProposed {
    pub schema: ReviewProposedSchema,
    pub review: crate::knowledge::AdaptiveReviewRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct ReviewApplied {
    pub schema: ReviewAppliedSchema,
    pub review_id: ReviewId,
    pub expected_review_revision: Revision,
}

impl_canonical!(SourceRecorded);
impl_canonical!(NativeFactsRecorded);
impl_canonical!(SourceRecaptured);
impl_canonical!(SourceObservationProposed);
impl_canonical!(SourceObserved);
impl_canonical!(DependencyRecorded);
impl_canonical!(ClosureAssessed);
impl_canonical!(ApplicabilityAssessed);
impl_canonical!(RegionTransitioned);
impl_canonical!(RegionRelevanceSet);
impl_canonical!(RegionSplit);
impl_canonical!(RegionMerged);
impl_canonical!(SemanticAssessmentProposed);
impl_canonical!(FactProposed);
impl_canonical!(FactAdjudicated);
impl_canonical!(ReviewProposed);
impl_canonical!(ReviewApplied);
