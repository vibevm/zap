//! Canonical outcome boundaries whose identity survives work decomposition.

mod achievement;
mod achievement_core;
mod cells;
mod fingerprints;
mod model;
mod payloads;
mod queries;
mod records;
mod revision_cells;
mod transform_cell;
mod transform_helpers;
mod transform_mapping;
mod transform_model;
mod transform_query;
mod transform_validation;
mod validation;

pub use cells::{
    MILESTONE_ACHIEVEMENT_ACCEPTED_KIND, MILESTONE_CREATED_KIND, MILESTONE_REVISED_KIND,
};
pub use fingerprints::{
    milestone_proof_fingerprint, milestone_revision_is_self_consistent,
    milestone_semantic_fingerprint,
};
pub use model::*;
pub use payloads::*;
pub use queries::{
    MilestoneAchievementInput, MilestoneAchievementQuery, MilestoneAchievementView,
    MilestoneProofEvaluationCost, MilestoneReadInput, MilestoneReadQuery, MilestoneRevisionInput,
    MilestoneRevisionQuery, MilestoneRevisionView, MilestoneView, milestone_achievement_validity,
};
pub use records::*;
pub use transform_cell::MILESTONE_TRANSFORM_APPLIED_KIND;
pub use transform_model::*;
pub use transform_query::{
    MilestoneTransformInput, MilestoneTransformPreview, MilestoneTransformPreviewInput,
    MilestoneTransformPreviewQuery, MilestoneTransformQuery, MilestoneTransformView,
};
pub use validation::load_current_milestone_revision;

pub(crate) use cells::cell_sets;
pub(crate) use queries::query_set;
