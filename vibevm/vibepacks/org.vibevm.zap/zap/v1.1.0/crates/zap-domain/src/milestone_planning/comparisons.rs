use std::collections::BTreeSet;

use zap_wire::{EvidenceId, ObligationId, SubjectRef, WorkId, ZapError};

use super::{
    ExistingEvidenceDisposition, ExistingWorkDisposition, WorkLineage, WorkMaterializationRationale,
};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::knowledge::CurrentProofSet;
use crate::lowering::{LoweredGraph, LoweringRecord};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION");

pub(crate) fn work_candidates(
    graph: &LoweredGraph,
    rationale: &WorkMaterializationRationale,
    obligations: &[ObligationRecord],
    existing: &[WorkRecord],
    lowerings: &[LoweringRecord],
) -> BTreeSet<WorkId> {
    let obligation_ids = rationale
        .cause
        .obligation_ids()
        .iter()
        .collect::<BTreeSet<_>>();
    let existing_ids = existing
        .iter()
        .map(|row| &row.work_id)
        .collect::<BTreeSet<_>>();
    let mut ids = obligations
        .iter()
        .filter(|row| obligation_ids.contains(&row.obligation_id))
        .flat_map(|row| row.owners.iter().map(|owner| owner.work_id.clone()))
        .filter(|id| existing_ids.contains(id))
        .collect::<BTreeSet<_>>();
    ids.extend(
        existing
            .iter()
            .filter(|row| row.parent_id.as_ref() == Some(&graph.parent_id))
            .map(|row| row.work_id.clone()),
    );
    ids.extend(
        lowerings
            .iter()
            .filter(|row| row.target == graph.parent_id)
            .flat_map(|row| row.work.iter().map(|binding| binding.work_id.clone())),
    );
    ids.remove(&rationale.work_id);
    ids
}

pub(crate) fn validate_work_comparisons(
    work: &WorkRecord,
    contract: Option<&TaskContractRecord>,
    rationale: &WorkMaterializationRationale,
    expected: &BTreeSet<WorkId>,
) -> Result<(), ZapError> {
    if !sorted_unique_by(&rationale.existing_work, |row| &row.work_id)
        || rationale
            .existing_work
            .iter()
            .map(|row| &row.work_id)
            .ne(expected.iter())
    {
        return Err(invalid(
            "existing Work comparison does not cover the exact candidate set",
        ));
    }
    for row in &rationale.existing_work {
        match &row.disposition {
            ExistingWorkDisposition::ReusedAsInput { reason } => {
                let contract_reads = contract.is_some_and(|contract| {
                    contract
                        .contract
                        .read_subjects
                        .binary_search(&SubjectRef::Work(row.work_id.clone()))
                        .is_ok()
                });
                if blank(reason) || (!work.depends_on.contains(&row.work_id) && !contract_reads) {
                    return Err(invalid(
                        "reused Work must be an explicit dependency or input",
                    ));
                }
            }
            ExistingWorkDisposition::DistinctContribution { semantic_rationale }
                if !blank(semantic_rationale) => {}
            ExistingWorkDisposition::DistinctContribution { .. } => {
                return Err(invalid("semantic distinctness rationale is blank"));
            }
        }
    }
    Ok(())
}

pub(crate) fn evidence_candidates(
    outcome: &OutcomeRecord,
    obligation_ids: &[ObligationId],
    proofs: &CurrentProofSet,
) -> BTreeSet<EvidenceId> {
    proofs
        .values()
        .filter(|row| {
            row.applies_to.outcome_id == outcome.outcome_id
                && row
                    .applies_to
                    .obligation_ids
                    .iter()
                    .any(|id| obligation_ids.binary_search(id).is_ok())
        })
        .map(|row| row.evidence_id.clone())
        .collect()
}

pub(crate) fn validate_evidence_comparisons(
    lowering: &LoweringRecord,
    rationale: &WorkMaterializationRationale,
    expected: &BTreeSet<EvidenceId>,
) -> Result<(), ZapError> {
    if !sorted_unique_by(&rationale.existing_evidence, |row| &row.evidence_id)
        || rationale
            .existing_evidence
            .iter()
            .map(|row| &row.evidence_id)
            .ne(expected.iter())
    {
        return Err(invalid(
            "existing evidence comparison does not cover current proof",
        ));
    }
    for row in &rationale.existing_evidence {
        match &row.disposition {
            ExistingEvidenceDisposition::Reused
                if lowering
                    .verification
                    .reused_evidence
                    .binary_search(&row.evidence_id)
                    .is_ok() => {}
            ExistingEvidenceDisposition::Insufficient { semantic_rationale }
                if !blank(semantic_rationale) => {}
            _ => {
                return Err(invalid(
                    "evidence reuse or insufficiency disposition is invalid",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_lineage(
    rationale: &WorkMaterializationRationale,
    candidates: &BTreeSet<WorkId>,
) -> Result<(), ZapError> {
    match &rationale.lineage {
        WorkLineage::New { reason } if !blank(reason) => Ok(()),
        WorkLineage::Successor {
            predecessor_work_ids,
            reason,
        } if !predecessor_work_ids.is_empty()
            && sorted_unique(predecessor_work_ids)
            && predecessor_work_ids
                .iter()
                .all(|id| candidates.contains(id))
            && !blank(reason) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "new Work lineage is blank, duplicate, or outside comparisons",
        )),
    }
}

fn blank<const N: usize>(text: &zap_wire::BoundedText<N>) -> bool {
    !text
        .as_str()
        .chars()
        .any(|character| !character.is_whitespace())
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn invalid(message: &'static str) -> ZapError {
    super::validation::error(zap_wire::ErrorCode::InvalidValue, message)
}
