use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_wire::{
    CanonicalEncode, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, Revision,
    ZapError,
};

use crate::control::ObligationRecord;
use crate::intent::{
    CharterAmended, CharterDrafted, IntentProposed, IntentRecord, OutcomeAdopted, OutcomeProposed,
    OutcomeRecord,
};
use crate::intent::{CharterRecord, ProposedObligation};
use crate::seams::{
    CharterBinding, LifecycleStatus, ObligationDisposition, ObligationStatus,
    actions_are_sorted_unique, refuse, sorted_unique, sorted_unique_nonempty,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-ACTIVATION");

const CHARTER_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-ACTIVATION";
const INTENT_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-AUTHORITY";
const OUTCOME_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS";

pub fn draft_charter(payload: &CharterDrafted) -> Result<CharterRecord, ZapError> {
    let charter = &payload.charter;
    let valid = charter.status == LifecycleStatus::Proposed
        && actions_are_sorted_unique(&charter.allowed_actions)
        && sorted_unique(&charter.mutable_obligations)
        && sorted_unique(&charter.essential_obligations)
        && sorted_unique(&charter.allowed_dispositions)
        && charter
            .essential_obligations
            .iter()
            .all(|id| !charter.mutable_obligations.contains(id));
    if !valid {
        return refuse(
            ErrorCode::InvalidValue,
            CHARTER_REQ,
            "charter delegation is unordered, duplicated, mutable where essential, or not proposed",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(charter.clone())
}

pub fn activate_charter(
    draft: &CharterRecord,
    expected_digest: PayloadDigest,
) -> Result<CharterRecord, ZapError> {
    if draft.status != LifecycleStatus::Proposed || draft.digest != expected_digest {
        return refuse(
            ErrorCode::StaleBasis,
            CHARTER_REQ,
            "charter activation must bind the exact proposed revision and digest",
            FixSurface::Command,
            ErrorDetail::None,
        );
    }
    let mut active = draft.clone();
    active.status = LifecycleStatus::Active;
    Ok(active)
}

pub fn amend_charter(
    current: &CharterRecord,
    payload: &CharterAmended,
) -> Result<(CharterRecord, CharterRecord), ZapError> {
    let next = draft_charter(&CharterDrafted {
        schema: crate::intent::CharterDraftedSchema::V1,
        charter: payload.charter.clone(),
    })?;
    if current.status != LifecycleStatus::Active
        || current.revision != payload.expected_active_revision
        || current.digest != payload.expected_active_digest
        || next.revision.get() != current.revision.get().saturating_add(1)
        || next.parent_digest != Some(current.digest)
        || next.campaign_id != current.campaign_id
    {
        return refuse(
            ErrorCode::StaleRevision,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-AMENDMENT",
            "charter amendment must be the exact consecutive child of the active charter",
            FixSurface::Command,
            ErrorDetail::StaleRevision {
                expected: payload.expected_active_revision,
                actual: current.revision,
            },
        );
    }
    let mut old = current.clone();
    old.status = LifecycleStatus::Superseded;
    let mut active = next;
    active.status = LifecycleStatus::Active;
    Ok((old, active))
}

pub fn propose_intent(
    payload: &IntentProposed,
    previous: Option<&IntentRecord>,
) -> Result<IntentRecord, ZapError> {
    let revision_valid = match (payload.previous_intent_id.as_ref(), previous) {
        (None, None) => payload.revision.get() == 1,
        (Some(id), Some(record)) => {
            id == &record.intent_id
                && record.status == LifecycleStatus::Active
                && payload.revision.get() == record.revision.get().saturating_add(1)
        }
        _ => false,
    };
    if !revision_valid
        || !sorted_unique_nonempty(&payload.beneficiaries)
        || !sorted_unique_nonempty(&payload.values)
        || !sorted_unique(&payload.constraints)
        || !sorted_unique(&payload.source_refs)
    {
        return refuse(
            ErrorCode::InvalidValue,
            INTENT_REQ,
            "intent proposal must be consecutive and use sorted unique required values",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let bytes = payload.encode_canonical(CodecEpoch::CURRENT)?;
    Ok(IntentRecord {
        intent_id: payload.intent_id.clone(),
        revision: payload.revision,
        previous_intent_id: payload.previous_intent_id.clone(),
        summary: payload.summary.clone(),
        beneficiaries: payload.beneficiaries.clone(),
        values: payload.values.clone(),
        constraints: payload.constraints.clone(),
        source_refs: payload.source_refs.clone(),
        status: LifecycleStatus::Proposed,
        fingerprint: PayloadDigest::hash(bytes.as_bytes()),
        owner_binding: None,
    })
}

pub fn adopt_intent(
    proposal: &IntentRecord,
    active_charter: &CharterRecord,
    active_intent: Option<&IntentRecord>,
    active_outcome_exists: bool,
) -> Result<(Option<IntentRecord>, IntentRecord), ZapError> {
    let predecessor_matches =
        active_intent.map(|record| &record.intent_id) == proposal.previous_intent_id.as_ref();
    let binding_matches = active_charter.status == LifecycleStatus::Active
        && active_charter.intent_id == proposal.intent_id
        && active_charter.intent_digest == proposal.fingerprint;
    let amended_for_revision = active_intent.is_none_or(|record| {
        active_charter.revision
            > record
                .owner_binding
                .as_ref()
                .map_or(Revision::GENESIS, |b| Revision::new(b.charter_revision))
    });
    if active_outcome_exists
        || proposal.status != LifecycleStatus::Proposed
        || !predecessor_matches
        || !binding_matches
        || !amended_for_revision
    {
        return refuse(
            ErrorCode::Unauthorized,
            INTENT_REQ,
            "intent adoption requires the exact active Owner charter binding and no separate active outcome",
            FixSurface::Authority,
            ErrorDetail::None,
        );
    }
    let mut old = active_intent.cloned();
    if let Some(record) = &mut old {
        record.status = LifecycleStatus::Superseded;
    }
    let mut active = proposal.clone();
    active.status = LifecycleStatus::Active;
    active.owner_binding = Some(CharterBinding {
        charter_id: active_charter.charter_id.clone(),
        charter_revision: active_charter.revision.get(),
        charter_digest: active_charter.digest,
        intent_id: active.intent_id.clone(),
        intent_digest: active.fingerprint,
    });
    Ok((old, active))
}

pub fn propose_outcome(
    payload: &OutcomeProposed,
    previous: Option<&OutcomeRecord>,
) -> Result<OutcomeRecord, ZapError> {
    let revision_valid = match (payload.previous_outcome_id.as_ref(), previous) {
        (None, None) => payload.revision.get() == 1,
        (Some(id), Some(record)) => {
            id == &record.outcome_id
                && record.status == LifecycleStatus::Active
                && payload.revision.get() == record.revision.get().saturating_add(1)
        }
        _ => false,
    };
    let mut obligation_ids = BTreeSet::new();
    let obligations_valid = payload.obligations.iter().all(|row| {
        obligation_ids.insert(row.obligation_id.clone())
            && sorted_unique(&row.source_refs)
            && sorted_unique_nonempty(&row.owners)
    });
    if !revision_valid
        || !obligations_valid
        || !sorted_unique_nonempty(&payload.benefits)
        || !sorted_unique_nonempty(&payload.guarantees)
        || !sorted_unique(&payload.tradeoffs)
        || !sorted_unique(&payload.required_final_gate_evidence_ids)
        || !sorted_unique(&payload.required_promotions)
        || !payload
            .final_gate_disposition
            .matches_items(payload.required_final_gate_evidence_ids.len())
        || !payload
            .promotion_disposition
            .matches_items(payload.required_promotions.len())
    {
        return refuse(
            ErrorCode::InvalidValue,
            OUTCOME_REQ,
            "outcome proposal must be consecutive and have unique typed obligations and guarantees",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(OutcomeRecord {
        outcome_id: payload.outcome_id.clone(),
        revision: payload.revision,
        previous_outcome_id: payload.previous_outcome_id.clone(),
        intent_id: payload.intent_id.clone(),
        summary: payload.summary.clone(),
        benefits: payload.benefits.clone(),
        guarantees: payload.guarantees.clone(),
        tradeoffs: payload.tradeoffs.clone(),
        proposed_obligations: payload.obligations.clone(),
        required_final_gate_evidence_ids: payload.required_final_gate_evidence_ids.clone(),
        required_promotions: payload.required_promotions.clone(),
        final_gate_disposition: payload.final_gate_disposition.clone(),
        promotion_disposition: payload.promotion_disposition.clone(),
        status: LifecycleStatus::Proposed,
        dispositions: Vec::new(),
    })
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption")]
pub struct OutcomeAdoption {
    pub previous: Option<OutcomeRecord>,
    pub active: OutcomeRecord,
    pub prior_obligations: Vec<ObligationRecord>,
    pub created_obligations: Vec<ObligationRecord>,
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION")]
pub fn adopt_outcome(
    proposal: &OutcomeRecord,
    active_intent: &IntentRecord,
    current: Option<&OutcomeRecord>,
    current_obligations: &[ObligationRecord],
    payload: &OutcomeAdopted,
    charter: &CharterRecord,
) -> Result<OutcomeAdoption, ZapError> {
    if proposal.status != LifecycleStatus::Proposed
        || proposal.intent_id != active_intent.intent_id
        || proposal.previous_outcome_id.as_ref() != current.map(|row| &row.outcome_id)
        || payload.outcome_id != proposal.outcome_id
        || !proposal
            .final_gate_disposition
            .authorized_by(&crate::seams::CharterRecordView {
                charter_id: &charter.charter_id,
                revision: charter.revision.get(),
                digest: charter.digest,
                no_duty_allowed: charter.completion_duty_policy.final_gate.allows_no_duty(),
            })
        || !proposal
            .promotion_disposition
            .authorized_by(&crate::seams::CharterRecordView {
                charter_id: &charter.charter_id,
                revision: charter.revision.get(),
                digest: charter.digest,
                no_duty_allowed: charter.completion_duty_policy.promotion.allows_no_duty(),
            })
    {
        return refuse(
            ErrorCode::StaleBasis,
            OUTCOME_REQ,
            "outcome adoption is not based on the active intent and outcome",
            FixSurface::Command,
            ErrorDetail::None,
        );
    }
    let active_prior: BTreeMap<_, _> = current_obligations
        .iter()
        .filter(|row| row.status == ObligationStatus::Active)
        .map(|row| (row.obligation_id.clone(), row))
        .collect();
    let dispositions: BTreeMap<_, _> = payload
        .obligation_dispositions
        .iter()
        .map(|row| (row.obligation_id.clone(), row))
        .collect();
    if current.is_none() && !dispositions.is_empty()
        || current.is_some()
            && (dispositions.len() != payload.obligation_dispositions.len()
                || dispositions.keys().ne(active_prior.keys()))
    {
        return refuse(
            ErrorCode::Conflict,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION",
            "every active obligation must receive exactly one explicit disposition",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let proposed_ids: BTreeSet<_> = proposal
        .proposed_obligations
        .iter()
        .map(|row| row.obligation_id.clone())
        .collect();
    if current_obligations
        .iter()
        .any(|row| proposed_ids.contains(&row.obligation_id))
    {
        return refuse(
            ErrorCode::DuplicateIdentity,
            OUTCOME_REQ,
            "new outcome obligation identity already exists in history",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut prior = Vec::new();
    for obligation in active_prior.values() {
        let Some(disposition) = dispositions.get(&obligation.obligation_id) else {
            if current.is_none() {
                let mut retained = (*obligation).clone();
                retained.current_outcomes.push(proposal.outcome_id.clone());
                prior.push(retained);
                continue;
            }
            return refuse(
                ErrorCode::Conflict,
                OUTCOME_REQ,
                "active obligation disposition is missing",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        };
        if !charter
            .allowed_dispositions
            .contains(&disposition.disposition)
            || (obligation.essential
                || !charter
                    .mutable_obligations
                    .contains(&obligation.obligation_id))
                && disposition.disposition != ObligationDisposition::Retained
            || disposition.disposition == ObligationDisposition::Replaced
                && (disposition.successor_ids.is_empty()
                    || disposition
                        .successor_ids
                        .iter()
                        .any(|id| !proposed_ids.contains(id)))
            || disposition.disposition != ObligationDisposition::Replaced
                && !disposition.successor_ids.is_empty()
        {
            return refuse(
                ErrorCode::Unauthorized,
                OUTCOME_REQ,
                "obligation disposition exceeds the active charter or lacks valid successors",
                FixSurface::Authority,
                ErrorDetail::None,
            );
        }
        let mut updated = (*obligation).clone();
        updated.disposition = disposition.disposition;
        updated.successors = disposition.successor_ids.clone();
        updated.unmet_portion = disposition.unmet_portion.clone();
        updated.status = match disposition.disposition {
            ObligationDisposition::Retained => {
                updated.current_outcomes.push(proposal.outcome_id.clone());
                ObligationStatus::Active
            }
            ObligationDisposition::Replaced => ObligationStatus::Replaced,
            ObligationDisposition::Excluded => ObligationStatus::Excluded,
            ObligationDisposition::Unattainable => ObligationStatus::Unattainable,
        };
        updated.revision = updated.revision.checked_next()?;
        prior.push(updated);
    }
    let created = proposal
        .proposed_obligations
        .iter()
        .map(|row| new_obligation(row, &proposal.outcome_id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut previous = current.cloned();
    if let Some(row) = &mut previous {
        row.status = LifecycleStatus::Superseded;
    }
    let mut active = proposal.clone();
    active.status = LifecycleStatus::Active;
    active.dispositions = payload.obligation_dispositions.clone();
    Ok(OutcomeAdoption {
        previous,
        active,
        prior_obligations: prior,
        created_obligations: created,
    })
}

fn new_obligation(
    row: &ProposedObligation,
    outcome_id: &zap_wire::OutcomeId,
) -> Result<ObligationRecord, ZapError> {
    if row.owners.is_empty() {
        return refuse(
            ErrorCode::InvalidValue,
            OUTCOME_REQ,
            "new obligation must have at least one typed owner",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(ObligationRecord {
        obligation_id: row.obligation_id.clone(),
        created_for_outcome: outcome_id.clone(),
        current_outcomes: vec![outcome_id.clone()],
        statement: row.statement.clone(),
        essential: row.essential,
        owners: row.owners.clone(),
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: Some(row.statement.clone()),
        revision: Revision::new(1),
    })
}
