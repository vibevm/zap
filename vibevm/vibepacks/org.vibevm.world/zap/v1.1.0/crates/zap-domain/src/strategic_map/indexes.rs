use zap_core::{QuerySnapshot, StateReaderExt};
use zap_wire::{SubjectRef, WorkId, ZapError};

use crate::admission_indexes::{AdmissionIndexBudget, indexed_values};
use crate::control::{ObligationRecord, WorkRecord};
use crate::knowledge::{
    FactRecord, KnowledgeDependencyRecord, KnowledgeEndpoint, RegionRecord, SourceRecord,
};
use crate::viewer_queries::ViewerNodeId;

use super::common::index_corrupt;
use super::model::{MapObjectRef, MapRelationship, MapRelationshipKind, MapRelationshipSource};
use super::relationships::{knowledge_relationship, normalize_relationships, relation, work_ref};

pub(super) fn indexed_relationships(
    snapshot: &dyn QuerySnapshot,
    object: &MapObjectRef,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<MapRelationship>, ZapError> {
    let mut rows = incident_knowledge(snapshot, object, budget)?;
    if let MapObjectRef::Viewer(ViewerNodeId::Work(work_id)) = object {
        rows.extend(work_incident(snapshot, work_id, budget)?);
    }
    normalize_relationships(&mut rows);
    Ok(rows)
}

pub(super) fn missing_relationship_objects(
    snapshot: &dyn QuerySnapshot,
    strategy: &crate::lowering::StrategicPlanRecord,
    relationships: &[MapRelationship],
    operation_budget: u32,
) -> Result<Vec<super::model::MapRelationshipGap>, ZapError> {
    let objects = relationships
        .iter()
        .flat_map(|row| [&row.from, &row.to])
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if objects.len() > operation_budget as usize {
        return Err(super::common::closure_limit());
    }
    let mut gaps = Vec::new();
    for object in objects {
        let missing = match &object {
            MapObjectRef::Strategy(id) => id != &strategy.strategic_revision_id,
            MapObjectRef::Resource(_) => false,
            MapObjectRef::Viewer(id) => {
                crate::viewer_queries::load_current_node(snapshot, id)?.is_none()
            }
        };
        if missing {
            gaps.push(super::model::MapRelationshipGap::MissingReferencedObject { object });
        }
    }
    Ok(gaps)
}

fn incident_knowledge(
    snapshot: &dyn QuerySnapshot,
    object: &MapObjectRef,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<MapRelationship>, ZapError> {
    let Some(endpoint) = knowledge_endpoint(object) else {
        return Ok(Vec::new());
    };
    let mut edges = std::collections::BTreeMap::new();
    for family in [
        crate::viewer_indexes::KNOWLEDGE_OUTGOING_INDEX,
        crate::viewer_indexes::KNOWLEDGE_INCOMING_INDEX,
    ] {
        for edge in
            indexed_values::<KnowledgeDependencyRecord, _>(snapshot, family, &endpoint, budget)?
        {
            let valid = if family == crate::viewer_indexes::KNOWLEDGE_OUTGOING_INDEX {
                edge.prerequisite == endpoint
            } else {
                edge.dependent == endpoint
            };
            if !valid {
                return Err(index_corrupt());
            }
            if let Some(prior) = edges.insert(edge.edge_id.clone(), edge.clone())
                && prior != edge
            {
                return Err(index_corrupt());
            }
        }
    }
    edges
        .values()
        .filter_map(|edge| knowledge_relationship(edge).transpose())
        .collect()
}

fn work_incident(
    snapshot: &dyn QuerySnapshot,
    work_id: &WorkId,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<MapRelationship>, ZapError> {
    let work = work_ref(work_id);
    let mut rows = Vec::new();
    for child in indexed_values::<WorkId, _>(
        snapshot,
        crate::viewer_indexes::WORK_CHILD_INDEX,
        work_id,
        budget,
    )? {
        let record = snapshot
            .get_typed::<WorkRecord>(&child)?
            .ok_or_else(index_corrupt)?;
        if record.parent_id.as_ref() != Some(work_id) {
            return Err(index_corrupt());
        }
        rows.push(relation(
            work.clone(),
            work_ref(&child),
            MapRelationshipKind::Contains,
            None,
            MapRelationshipSource::WorkRecord {
                work_id: child,
                revision: record.revision,
            },
        )?);
    }
    for dependent in indexed_values::<WorkId, _>(
        snapshot,
        crate::viewer_indexes::WORK_DEPENDENT_INDEX,
        work_id,
        budget,
    )? {
        let record = snapshot
            .get_typed::<WorkRecord>(&dependent)?
            .ok_or_else(index_corrupt)?;
        if record.depends_on.binary_search(work_id).is_err() {
            return Err(index_corrupt());
        }
        rows.push(relation(
            work.clone(),
            work_ref(&dependent),
            MapRelationshipKind::WorkPrerequisite,
            None,
            MapRelationshipSource::WorkRecord {
                work_id: dependent,
                revision: record.revision,
            },
        )?);
    }
    for obligation_id in indexed_values::<zap_wire::ObligationId, _>(
        snapshot,
        crate::basis_indexes::OBLIGATION_OWNER_INDEX,
        work_id,
        budget,
    )? {
        let obligation = snapshot
            .get_typed::<ObligationRecord>(&obligation_id)?
            .ok_or_else(index_corrupt)?;
        if obligation.status != crate::seams::ObligationStatus::Active
            || !obligation
                .owners
                .iter()
                .any(|owner| &owner.work_id == work_id)
        {
            return Err(index_corrupt());
        }
        let source = MapRelationshipSource::Obligation {
            obligation_id: obligation_id.clone(),
            revision: obligation.revision,
        };
        for owner in obligation
            .owners
            .iter()
            .filter(|owner| &owner.work_id == work_id)
        {
            rows.push(relation(
                work.clone(),
                MapObjectRef::Viewer(ViewerNodeId::Obligation(obligation_id.clone())),
                MapRelationshipKind::OwnsObligation,
                Some(owner.role),
                source.clone(),
            )?);
        }
    }
    for source_id in indexed_values::<zap_wire::SourceId, _>(
        snapshot,
        crate::basis_indexes::SOURCE_SUBJECT_INDEX,
        &SubjectRef::Work(work_id.clone()),
        budget,
    )? {
        let source = snapshot
            .get_typed::<SourceRecord>(&source_id)?
            .ok_or_else(index_corrupt)?;
        if !matches!(
            &source.scope,
            crate::knowledge::SourceScope::Subjects(subjects)
                if subjects.binary_search(&SubjectRef::Work(work_id.clone())).is_ok()
        ) {
            return Err(index_corrupt());
        }
        rows.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Source(source_id.clone())),
            work.clone(),
            MapRelationshipKind::SourceReference,
            None,
            MapRelationshipSource::Source {
                source_id,
                revision: source.revision,
            },
        )?);
    }
    for fact_id in indexed_values::<zap_wire::FactId, _>(
        snapshot,
        crate::basis_indexes::FACT_SUBJECT_INDEX,
        &SubjectRef::Work(work_id.clone()),
        budget,
    )? {
        let fact = snapshot
            .get_typed::<FactRecord>(&fact_id)?
            .ok_or_else(index_corrupt)?;
        if fact
            .subject_refs
            .binary_search(&SubjectRef::Work(work_id.clone()))
            .is_err()
        {
            return Err(index_corrupt());
        }
        rows.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Fact(fact_id.clone())),
            work.clone(),
            MapRelationshipKind::AppliesTo,
            None,
            MapRelationshipSource::Fact {
                fact_id,
                revision: fact.revision,
            },
        )?);
    }
    for region_id in indexed_values::<crate::knowledge::RegionId, _>(
        snapshot,
        crate::basis_indexes::REGION_SUBJECT_INDEX,
        &SubjectRef::Work(work_id.clone()),
        budget,
    )? {
        let region = snapshot
            .get_typed::<RegionRecord>(&region_id)?
            .ok_or_else(index_corrupt)?;
        if region.work_refs.binary_search(work_id).is_err()
            && region
                .subject_refs
                .binary_search(&SubjectRef::Work(work_id.clone()))
                .is_err()
        {
            return Err(index_corrupt());
        }
        let source = MapRelationshipSource::Region {
            region_id: region_id.clone(),
            revision: region.revision,
        };
        if region
            .subject_refs
            .binary_search(&SubjectRef::Work(work_id.clone()))
            .is_ok()
        {
            rows.push(relation(
                MapObjectRef::Viewer(ViewerNodeId::Region(region_id.clone())),
                work.clone(),
                MapRelationshipKind::AppliesTo,
                None,
                source.clone(),
            )?);
        }
        if region.work_refs.binary_search(work_id).is_ok() {
            rows.push(relation(
                MapObjectRef::Viewer(ViewerNodeId::Region(region_id)),
                work.clone(),
                MapRelationshipKind::Affects,
                None,
                source,
            )?);
        }
    }
    for evidence_id in indexed_values::<zap_wire::EvidenceId, _>(
        snapshot,
        crate::basis_indexes::EVIDENCE_WORK_INDEX,
        work_id,
        budget,
    )? {
        let evidence = snapshot
            .get_typed::<crate::acceptance::EvidenceAdjudicationRecord>(&evidence_id)?
            .ok_or_else(index_corrupt)?;
        if evidence.applies_to.work_ids.binary_search(work_id).is_err() {
            return Err(index_corrupt());
        }
        rows.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Evidence(evidence_id.clone())),
            work.clone(),
            MapRelationshipKind::EvidenceReference,
            None,
            MapRelationshipSource::Evidence {
                evidence_id,
                revision: evidence.revision,
            },
        )?);
    }
    Ok(rows)
}

fn knowledge_endpoint(object: &MapObjectRef) -> Option<KnowledgeEndpoint> {
    let MapObjectRef::Viewer(viewer) = object else {
        return None;
    };
    Some(match viewer {
        ViewerNodeId::Work(id) => KnowledgeEndpoint::Work(id.clone()),
        ViewerNodeId::Obligation(id) => KnowledgeEndpoint::Obligation(id.clone()),
        ViewerNodeId::Outcome(id) => KnowledgeEndpoint::Outcome(id.clone()),
        ViewerNodeId::Source(id) => KnowledgeEndpoint::Source(id.clone()),
        ViewerNodeId::Fact(id) => KnowledgeEndpoint::Fact(id.clone()),
        ViewerNodeId::Evidence(id) => KnowledgeEndpoint::Evidence(id.clone()),
        ViewerNodeId::Decision(id) => KnowledgeEndpoint::Decision(id.clone()),
        ViewerNodeId::Contract(_)
        | ViewerNodeId::Region(_)
        | ViewerNodeId::Hold(_)
        | ViewerNodeId::CandidateReview(_) => return None,
    })
}
