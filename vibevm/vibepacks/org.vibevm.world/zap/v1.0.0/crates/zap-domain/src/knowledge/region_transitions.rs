use std::collections::BTreeSet;

use zap_wire::{ErrorCode, ErrorDetail, FixSurface, Revision, ZapError};

use crate::knowledge::{
    KnowledgeEndpoint, NewRegion, RegionMerged, RegionRecord, RegionRelevanceSet, RegionSplit,
    RegionState, RegionTransitioned, SemanticAssessmentProposed, SemanticAssessmentRecord,
};
use crate::seams::{refuse, sorted_unique, sorted_unique_nonempty};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#FOG-RECOMPUTATION");

const KNOWLEDGE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE";

pub fn transition_region(
    current: &RegionRecord,
    payload: &RegionTransitioned,
    evidence: Option<&crate::knowledge::ScopedEvidenceWitness>,
) -> Result<RegionRecord, ZapError> {
    if current.region_id != payload.region_id
        || current.subject_refs != payload.subject_refs
        || current.work_refs != payload.work_refs
        || current.state != payload.from
        || payload.from == payload.to
        || !sorted_unique(&payload.evidence_refs)
        || evidence.is_some_and(|row| !row.matches(&payload.evidence_refs, payload.basis))
        || !payload.evidence_refs.is_empty() && evidence.is_none()
        || payload.to == RegionState::Evidenced && evidence.is_none()
    {
        return refuse(
            ErrorCode::Conflict,
            KNOWLEDGE_REQ,
            "region transition is stale or claims evidence without evidence references",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.state = payload.to;
    next.evidence_refs = payload.evidence_refs.clone();
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

pub fn set_region_relevance(
    current: &RegionRecord,
    payload: &RegionRelevanceSet,
    evidence: Option<&crate::knowledge::ScopedEvidenceWitness>,
) -> Result<RegionRecord, ZapError> {
    if current.region_id != payload.region_id
        || current.subject_refs != payload.subject_refs
        || current.work_refs != payload.work_refs
        || !sorted_unique(&payload.evidence_refs)
        || evidence.is_some_and(|row| !row.matches(&payload.evidence_refs, payload.basis))
        || !payload.evidence_refs.is_empty() && evidence.is_none()
        || payload.relevance == crate::knowledge::RegionRelevance::Irrelevant && evidence.is_none()
    {
        return refuse(
            ErrorCode::StaleBasis,
            KNOWLEDGE_REQ,
            "region relevance change names a different region",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.relevance = payload.relevance;
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

fn new_region(
    value: &NewRegion,
    parents: Vec<crate::knowledge::RegionId>,
) -> Result<RegionRecord, ZapError> {
    if !sorted_unique_nonempty(&value.subject_refs)
        || !sorted_unique(&value.work_refs)
        || value.relevance != crate::knowledge::RegionRelevance::Unknown
    {
        return refuse(
            ErrorCode::InvalidValue,
            KNOWLEDGE_REQ,
            "region work references must be sorted and unique",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(RegionRecord {
        region_id: value.region_id.clone(),
        question: value.question.clone(),
        subject_refs: value.subject_refs.clone(),
        work_refs: value.work_refs.clone(),
        state: RegionState::Unexamined,
        relevance: crate::knowledge::RegionRelevance::Unknown,
        parents,
        children: Vec::new(),
        evidence_refs: Vec::new(),
        revision: Revision::new(1),
    })
}

pub fn split_region(
    current: &RegionRecord,
    payload: &RegionSplit,
) -> Result<(RegionRecord, Vec<RegionRecord>), ZapError> {
    if current.region_id != payload.region_id || payload.children.len() < 2 {
        return refuse(
            ErrorCode::Conflict,
            KNOWLEDGE_REQ,
            "region split requires the exact parent and at least two children",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut ids = BTreeSet::new();
    let children = payload
        .children
        .iter()
        .map(|child| {
            if !ids.insert(child.region_id.clone()) {
                return refuse(
                    ErrorCode::DuplicateIdentity,
                    KNOWLEDGE_REQ,
                    "region split child is duplicated",
                    FixSurface::Payload,
                    ErrorDetail::None,
                );
            }
            new_region(child, vec![current.region_id.clone()])
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut parent = current.clone();
    parent.state = RegionState::Invalidated;
    parent
        .children
        .extend(children.iter().map(|child| child.region_id.clone()));
    parent.children.sort();
    parent.children.dedup();
    parent.revision = parent.revision.checked_next()?;
    Ok((parent, children))
}

pub fn merge_regions(
    current: &[RegionRecord],
    payload: &RegionMerged,
) -> Result<(Vec<RegionRecord>, RegionRecord), ZapError> {
    if current.len() < 2
        || !sorted_unique_nonempty(&payload.region_ids)
        || current
            .iter()
            .map(|row| &row.region_id)
            .ne(payload.region_ids.iter())
    {
        return refuse(
            ErrorCode::Conflict,
            KNOWLEDGE_REQ,
            "region merge requires the exact sorted distinct origin set",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let merged = new_region(&payload.merged, payload.region_ids.clone())?;
    let mut origins = current.to_vec();
    for origin in &mut origins {
        origin.state = RegionState::Invalidated;
        origin.children.push(merged.region_id.clone());
        origin.children.sort();
        origin.children.dedup();
        origin.revision = origin.revision.checked_next()?;
    }
    Ok((origins, merged))
}

pub fn propose_semantic_assessment(
    payload: &SemanticAssessmentProposed,
    known_endpoints: &BTreeSet<KnowledgeEndpoint>,
) -> Result<SemanticAssessmentRecord, ZapError> {
    let row = &payload.assessment;
    if !row.proposed
        || !sorted_unique_nonempty(&row.premises)
        || row
            .premises
            .iter()
            .any(|premise| !known_endpoints.contains(premise))
        || !sorted_unique(&row.evidence_refs)
    {
        return refuse(
            ErrorCode::MissingReference,
            KNOWLEDGE_REQ,
            "semantic assessment must remain proposed and bind known sorted premises",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(row.clone())
}
