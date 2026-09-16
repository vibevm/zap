specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#METHOD-CAPABILITY-BOUNDARY"
);

use zap_core::{
    ActionImpactRequest, ActionImpactRule, CellRegistrationBuilder, CellSet, CompletionProviderSet,
    QuerySet, RecordSet, RouteRegistry,
};
use zap_wire::ZapError;

use crate::acceptance::{
    CampaignClosedCell, CandidateReviewRecord, ClosureRecord, EvidenceAdjudicationRecord,
    IntegrationAcceptanceRecord, PromotionRecord, StageAcceptanceRecord, WorkAcceptanceRecord,
};
use crate::control::{DeferralRecord, ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::{
    CharterActivatedCell, CharterAmendedCell, CharterDraftedCell, IntentAdoptedCell,
    IntentAdoptionBasisScope, IntentAdoptionEffectContract, IntentProposedCell, OutcomeAdoptedCell,
    OutcomeAdoptionBasisScope, OutcomeAdoptionEffectContract, OutcomeProposedCell,
};
use crate::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use crate::knowledge::{
    AdaptiveReviewRecord, FactRecord, KnowledgeClosureRecord, KnowledgeDependencyRecord,
    ProofReuseRecord, RegionRecord, SemanticAssessmentRecord, SourceApplicabilityRecord,
    SourceObservationCandidateRecord, SourceRecord,
};
use crate::seams::TypedActionImpact;

pub fn record_set() -> Result<RecordSet, ZapError> {
    let mut set = RecordSet::empty();
    set.register::<IntentRecord>()?;
    set.register::<CharterRecord>()?;
    set.register::<OutcomeRecord>()?;
    set.register::<ObligationRecord>()?;
    set.register::<WorkRecord>()?;
    set.register::<TaskContractRecord>()?;
    set.register::<CandidateReviewRecord>()?;
    set.register::<StageAcceptanceRecord>()?;
    set.register::<DeferralRecord>()?;
    set.register::<EvidenceAdjudicationRecord>()?;
    set.register::<IntegrationAcceptanceRecord>()?;
    set.register::<WorkAcceptanceRecord>()?;
    set.register::<PromotionRecord>()?;
    set.register::<ClosureRecord>()?;
    set.register::<SourceRecord>()?;
    set.register::<SourceObservationCandidateRecord>()?;
    set.register::<FactRecord>()?;
    set.register::<KnowledgeDependencyRecord>()?;
    set.register::<KnowledgeClosureRecord>()?;
    set.register::<SourceApplicabilityRecord>()?;
    set.register::<RegionRecord>()?;
    set.register::<SemanticAssessmentRecord>()?;
    set.register::<AdaptiveReviewRecord>()?;
    set.register::<ProofReuseRecord>()?;
    set.register::<crate::economics::ChangeBaselineRecord>()?;
    set.register::<crate::economics::ChangePolicyRecord>()?;
    set.register::<crate::economics::ChangeAssessmentRecord>()?;
    set.register::<crate::economics::CostForecastRecord>()?;
    set.register::<crate::economics::ChangeHoldRecord>()?;
    set.register::<crate::economics::ChangeAdmissionRecord>()?;
    set.register::<crate::owner_control::PauseRecord>()?;
    set.register::<crate::owner_control::StopRuleRecord>()?;
    set.register::<crate::owner_control::ActionExceptionRecord>()?;
    set.register::<crate::owner_control::ApproachEpochRecord>()?;
    set.register::<crate::owner_control::OwnerChangeDecisionRecord>()?;
    set.register::<crate::lowering::StrategicPlanRecord>()?;
    set.register::<crate::lowering::LoweringRecord>()?;
    set.register::<crate::lowering::WorkerPacketRecord>()?;
    set.register::<crate::lowering::ReviewReloweringRecord>()?;
    set.register::<crate::lowering::WeakBundleRecord>()?;
    set.register::<crate::lowering::EncounterRecord>()?;
    set.register::<crate::lowering::ReturnImportRecord>()?;
    set.register::<crate::lowering::FailedApproachRecord>()?;
    set.register::<crate::lowering::ReturnReassessmentRecord>()?;
    set.register::<crate::dreamer::DreamBranchRecord>()?;
    set.register::<crate::dreamer::DreamProjectionRecord>()?;
    set.register::<crate::dreamer::DreamApplicationRecord>()?;
    set.register::<crate::dreamer::DreamCombinedAuthorizationRecord>()?;
    set.register::<crate::legacy_projection::LegacyMandateRecord>()?;
    set.register::<crate::legacy_projection::LegacyNodeMetadataRecord>()?;
    set.register::<crate::legacy_projection::LegacyTaskConstraintRecord>()?;
    Ok(set)
}

pub fn cell_set() -> Result<CellSet, ZapError> {
    let mut sets = vec![
        CellSet::single(CharterDraftedCell)?,
        CellSet::single(CharterActivatedCell)?,
        CellSet::single(CharterAmendedCell)?,
        CellSet::single(IntentProposedCell)?,
        CellRegistrationBuilder::new(IntentAdoptedCell)
            .basis(IntentAdoptionBasisScope)?
            .effect_contract(IntentAdoptionEffectContract)?
            .action_impact(TypedActionImpact::new(
                |payload: &crate::intent::IntentAdopted| {
                    let subject = zap_wire::SubjectRef::Intent(payload.intent_id.clone());
                    ActionImpactRequest::new(
                        ActionImpactRule::InitialBaselineOrSemantic {
                            baseline_subject: subject.clone(),
                        },
                        Vec::new(),
                        vec![subject],
                    )
                },
            ))?
            .build()?,
        CellSet::single(OutcomeProposedCell)?,
        CellRegistrationBuilder::new(OutcomeAdoptedCell)
            .basis(OutcomeAdoptionBasisScope)?
            .effect_contract(OutcomeAdoptionEffectContract)?
            .action_impact(TypedActionImpact::new(
                |payload: &crate::intent::OutcomeAdopted| {
                    let subject = zap_wire::SubjectRef::Outcome(payload.outcome_id.clone());
                    ActionImpactRequest::new(
                        ActionImpactRule::InitialBaselineOrSemantic {
                            baseline_subject: subject.clone(),
                        },
                        Vec::new(),
                        vec![subject],
                    )
                },
            ))?
            .build()?,
        CellRegistrationBuilder::new(CampaignClosedCell)
            .action_impact(TypedActionImpact::new(
                |payload: &crate::acceptance::CampaignClosed| {
                    ActionImpactRequest::new(
                        ActionImpactRule::Proof,
                        Vec::new(),
                        vec![zap_wire::SubjectRef::Outcome(
                            payload.active_outcome_id.clone(),
                        )],
                    )
                },
            ))?
            .build()?,
    ];
    sets.extend(crate::control::cell_sets()?);
    sets.extend(crate::acceptance::cell_sets()?);
    sets.extend(crate::knowledge::cell_sets()?);
    sets.extend(crate::economics::cell_sets()?);
    sets.extend(crate::owner_control::cell_sets()?);
    sets.extend(crate::lowering::cell_sets()?);
    sets.extend(crate::dreamer::cell_sets()?);
    CellSet::compose(sets)
}

pub fn route_set() -> Result<RouteRegistry, ZapError> {
    let cells = cell_set()?;
    let routes = cells
        .kinds()
        .map(|kind| {
            let descriptor = cells.descriptor(kind).ok_or_else(|| {
                ZapError::from_static(
                    zap_wire::ErrorCode::InternalInvariant,
                    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
                    "domain cell disappeared while deriving its route registry",
                    zap_wire::FixSurface::Configuration,
                    zap_wire::ErrorDetail::None,
                )
            })?;
            Ok(RouteRegistry::single(
                kind.clone(),
                descriptor.route().clone(),
            ))
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    let routes = RouteRegistry::compose(routes)?;
    routes.validate_cells(&cells)?;
    Ok(routes)
}

pub fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::compose([
        crate::knowledge::query_set()?,
        crate::lowering::query_set()?,
        crate::dreamer::query_set()?,
        crate::viewer_queries::query_set()?,
        crate::legacy_projection::query_set()?,
    ])
}

pub fn completion_provider_set() -> Result<CompletionProviderSet, ZapError> {
    let mut set = CompletionProviderSet::empty();
    set.register(crate::queries::DomainCompletionProvider::new()?)?;
    set.register(crate::owner_control::ControlCompletionProvider::new()?)?;
    set.register(crate::economics::EconomicsCompletionProvider::new()?)?;
    Ok(set)
}

pub fn control_completion_provider_set() -> Result<CompletionProviderSet, ZapError> {
    let mut set = CompletionProviderSet::empty();
    set.register(crate::owner_control::ControlCompletionProvider::new()?)?;
    Ok(set)
}

pub fn economics_completion_provider_set() -> Result<CompletionProviderSet, ZapError> {
    let mut set = CompletionProviderSet::empty();
    set.register(crate::economics::EconomicsCompletionProvider::new()?)?;
    Ok(set)
}
