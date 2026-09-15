use serde::Serialize;
use zap_core::*;
use zap_domain::economics::*;
use zap_wire::*;

use crate::support::Harness;

include!("../../lowering_semantic_service/economics.rs");

pub(super) fn establish_baseline(
    harness: &Harness,
    name: &str,
) -> Result<ChangeBaselineId, Box<dyn std::error::Error>> {
    let baseline_id = ChangeBaselineId::parse(&format!("baseline.{name}"))?;
    let head = harness.store.head()?;
    harness.internal(
        &BaselineEstablished {
            baseline: ChangeBaselineRecord {
                baseline_id: baseline_id.clone(),
                base_digest: BaseDigest::hash(name.as_bytes()),
                committed_prefix_digest: PayloadDigest::hash(name.as_bytes()),
                committed_sequence: head,
                active_charter_digest: PayloadDigest::hash(b"active-charter"),
                active_intent_id: IntentId::parse("intent.one")?,
                active_outcome_id: OutcomeId::parse("outcome.one")?,
                active_outcome_digest: PayloadDigest::hash(b"active-outcome"),
                observed_plan_digest: PayloadDigest::hash(b"milestone-plan"),
                change_policy_revision: Revision::new(1),
                revision: head.checked_next()?,
            },
        },
        head,
        BasisBinding::NotApplicable,
        &format!("command-baseline-{name}"),
    )?;
    Ok(baseline_id)
}

pub(super) fn admit_semantic_effect<P>(
    harness: &Harness,
    payload: &P,
    impact: ActionImpactRequest,
    kind: EventKind,
    command_id: CommandId,
    name: &str,
    baseline_id: ChangeBaselineId,
) -> Result<(), Box<dyn std::error::Error>>
where
    P: CommandPayload + Serialize,
{
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?;
    let product_event_id = EventId::parse(&format!("event:{}", command_id.as_str()))?;
    let alternative_id = ChangeAlternativeId::parse(&format!("alternative.{name}"))?;
    let assessment_id = ChangeAssessmentId::parse(&format!("assessment.{name}"))?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            assessment_id,
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    EffectId::parse(&format!("effect.{name}"))?,
                    0,
                    kind.clone(),
                    product_payload.clone(),
                    Vec::new(),
                    product_event_id.clone(),
                )?],
                None,
            )?],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = &comparison.alternatives()[0];
    let request = &prepared.request().effects()[0];
    let view = &prepared.view().effects[0];
    let effect = ChangeEffect {
        effect_id: request.effect_id().clone(),
        index: request.index(),
        kind: request.kind().clone(),
        payload: request.payload().as_bytes().to_vec(),
        payload_digest: request.payload().digest(),
        subjects: request.declared_subjects().to_vec(),
        predecessors: request.predecessors().to_vec(),
        basis: request.basis().clone(),
        relevant_before: request.relevant_before(),
        relevant_after: request.declared_relevant_after(),
        product_event_id: request.product_event_id().clone(),
        preflight_digest: None,
    };
    let scope_request = AffectedScopeRequest::new(
        effect.subjects.clone(),
        effect
            .subjects
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let scope = DomainAffectedScopeProvider.derive(&snapshot, &scope_request)?;
    drop(snapshot);
    let proposal = assessment(
        name,
        baseline_id,
        alternative_id.clone(),
        effect,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope,
    )?;
    harness.data_with_basis(
        &ChangeAssessmentProposed {
            assessment: proposal.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(proposal.comparison_basis_digest),
        &format!("command-assessment-{name}"),
    )?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: proposal.assessment_id.clone(),
            hold_id: None,
            drain_job_ids: Vec::new(),
            independence_basis: proposal.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(proposal.comparison_basis_digest),
        &format!("command-adjudicate-{name}"),
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get_typed::<ChangeAssessmentRecord>(&proposal.assessment_id)?
        .ok_or("assessment missing")?;
    let admitted_effect = adjudicated.alternatives[0].effects[0].clone();
    drop(snapshot);
    let impact_digest = ActionImpactView::new(
        impact.request_digest(),
        ActionClass::parse("plan.lower")?,
        kind,
        product_event_id.clone(),
        product_payload.digest(),
        harness.store.head()?,
        ActionImpactClass::SemanticChange,
        Some(admitted_effect.relevant_before),
    )?
    .digest;
    harness.internal(
        &ChangeAdmissionPrepared {
            admission: ChangeAdmissionRecord {
                change_id: adjudicated.change_id.clone(),
                assessment_id: adjudicated.assessment_id.clone(),
                assessment_digest: assessment_digest(&adjudicated)?,
                forecast_id: None,
                forecast_digest: None,
                decision_id: None,
                alternative_id,
                effect_id: admitted_effect.effect_id.clone(),
                effect_index: 0,
                effect_fingerprint: admitted_effect.fingerprint()?,
                relevant_before: admitted_effect.relevant_before,
                action: ActionClass::parse("plan.lower")?,
                command_id: command_id.clone(),
                impact_digest,
                effect_item_digest: view.stable_digest,
                effect_preflight_digest: prepared.view().digest,
                payload_digest: product_payload.digest(),
                product_event_id,
                exception_id: None,
                hold_id: None,
                final_effect: true,
                applied_effect_ids: Vec::new(),
                applied: false,
                revision: Revision::new(1),
            },
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        &format!("command-admission-{name}"),
    )?;
    harness.privileged(
        payload,
        harness.store.head()?,
        BasisBinding::Exact(admitted_effect.relevant_before),
        command_id.as_str(),
    )?;
    Ok(())
}
