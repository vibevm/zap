use serde::Serialize;
use zap_wire::{CanonicalOutput, CodecEpoch, SubjectRef, WorkId, ZapError};

use crate::control::{TaskContractRecord, WorkRecord};
use crate::knowledge::{DependencyRelation, KnowledgeDependencyRecord, SourceScope};
use crate::lowering::{StrategicNode, StrategicPlanRecord};
use crate::seams::WorkKind;
use crate::viewer_queries::{ViewerDetail, ViewerNodeId};

use super::model::*;

pub(super) fn strategic_relationships(
    strategy: &StrategicPlanRecord,
    node: &StrategicNode,
) -> Result<Vec<MapRelationship>, ZapError> {
    let source = MapRelationshipSource::StrategicNode {
        strategy_id: strategy.strategic_revision_id.clone(),
        work_id: node.work_id.clone(),
    };
    let mut rows = vec![relation(
        MapObjectRef::Strategy(strategy.strategic_revision_id.clone()),
        work_ref(&node.work_id),
        MapRelationshipKind::Contains,
        None,
        source.clone(),
    )?];
    for dependency in &node.depends_on {
        rows.push(relation(
            work_ref(dependency),
            work_ref(&node.work_id),
            MapRelationshipKind::WorkPrerequisite,
            None,
            source.clone(),
        )?);
    }
    for obligation in &node.obligation_ids {
        rows.push(relation(
            work_ref(&node.work_id),
            MapObjectRef::Viewer(ViewerNodeId::Obligation(obligation.clone())),
            MapRelationshipKind::CoversObligation,
            None,
            source.clone(),
        )?);
    }
    Ok(rows)
}

pub(super) fn work_relationships(
    work: &WorkRecord,
    contract: Option<&TaskContractRecord>,
) -> Result<Vec<MapRelationship>, ZapError> {
    let work_source = MapRelationshipSource::WorkRecord {
        work_id: work.work_id.clone(),
        revision: work.revision,
    };
    let mut rows = Vec::new();
    if let Some(parent) = &work.parent_id {
        rows.push(relation(
            work_ref(parent),
            work_ref(&work.work_id),
            MapRelationshipKind::Contains,
            None,
            work_source.clone(),
        )?);
    }
    for dependency in &work.depends_on {
        rows.push(relation(
            work_ref(dependency),
            work_ref(&work.work_id),
            MapRelationshipKind::WorkPrerequisite,
            None,
            work_source.clone(),
        )?);
    }
    if let Some(contract) = contract {
        rows.extend(contract_relationships(contract)?);
    }
    Ok(rows)
}

fn contract_relationships(contract: &TaskContractRecord) -> Result<Vec<MapRelationship>, ZapError> {
    let source = MapRelationshipSource::Contract {
        contract_id: contract.contract_id.clone(),
        version: contract.version,
    };
    let contract_ref = MapObjectRef::Viewer(ViewerNodeId::Contract(contract.contract_id.clone()));
    let work = work_ref(&contract.work_id);
    let mut rows = vec![relation(
        contract_ref,
        work.clone(),
        MapRelationshipKind::ContractFor,
        None,
        source.clone(),
    )?];
    for id in &contract.contract.obligation_ids {
        rows.push(relation(
            work.clone(),
            MapObjectRef::Viewer(ViewerNodeId::Obligation(id.clone())),
            MapRelationshipKind::AcceptanceDuty,
            None,
            source.clone(),
        )?);
    }
    for id in &contract.contract.source_handles {
        rows.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Source(id.clone())),
            work.clone(),
            MapRelationshipKind::SourceReference,
            None,
            source.clone(),
        )?);
    }
    for id in &contract.contract.resources {
        rows.push(relation(
            work.clone(),
            MapObjectRef::Resource(id.clone()),
            MapRelationshipKind::ResourceUse,
            None,
            source.clone(),
        )?);
    }
    for subject in &contract.contract.read_subjects {
        if let Some(target) = object_from_subject(subject) {
            rows.push(relation(
                work.clone(),
                target,
                MapRelationshipKind::ReadSubject,
                None,
                source.clone(),
            )?);
        }
    }
    for subject in &contract.contract.write_subjects {
        if let Some(target) = object_from_subject(subject) {
            rows.push(relation(
                work.clone(),
                target,
                MapRelationshipKind::WriteSubject,
                None,
                source.clone(),
            )?);
        }
    }
    Ok(rows)
}

pub(super) fn detail_relationships(
    detail: &ViewerDetail,
) -> Result<Vec<MapRelationship>, ZapError> {
    let mut rows = Vec::new();
    match detail {
        ViewerDetail::Work(work) => rows.extend(work_relationships(work, None)?),
        ViewerDetail::Contract(contract) => rows.extend(contract_relationships(contract)?),
        ViewerDetail::Obligation(obligation) => {
            let source = MapRelationshipSource::Obligation {
                obligation_id: obligation.obligation_id.clone(),
                revision: obligation.revision,
            };
            let target =
                MapObjectRef::Viewer(ViewerNodeId::Obligation(obligation.obligation_id.clone()));
            for owner in &obligation.owners {
                rows.push(relation(
                    work_ref(&owner.work_id),
                    target.clone(),
                    MapRelationshipKind::OwnsObligation,
                    Some(owner.role),
                    source.clone(),
                )?);
            }
            for outcome in &obligation.current_outcomes {
                rows.push(relation(
                    MapObjectRef::Viewer(ViewerNodeId::Outcome(outcome.clone())),
                    target.clone(),
                    MapRelationshipKind::OutcomeScope,
                    None,
                    source.clone(),
                )?);
            }
            for successor in &obligation.successors {
                rows.push(relation(
                    target.clone(),
                    MapObjectRef::Viewer(ViewerNodeId::Obligation(successor.clone())),
                    MapRelationshipKind::Successor,
                    None,
                    source.clone(),
                )?);
            }
        }
        ViewerDetail::Outcome(outcome) => {
            let source = MapRelationshipSource::Outcome {
                outcome_id: outcome.outcome_id.clone(),
                revision: outcome.revision,
            };
            let target = MapObjectRef::Viewer(ViewerNodeId::Outcome(outcome.outcome_id.clone()));
            for evidence in &outcome.required_final_gate_evidence_ids {
                rows.push(relation(
                    MapObjectRef::Viewer(ViewerNodeId::Evidence(evidence.clone())),
                    target.clone(),
                    MapRelationshipKind::AcceptanceDuty,
                    None,
                    source.clone(),
                )?);
            }
        }
        ViewerDetail::Source(source_record) => {
            if let SourceScope::Subjects(subjects) = &source_record.scope {
                for subject in subjects {
                    if let Some(target) = object_from_subject(subject) {
                        rows.push(relation(
                            MapObjectRef::Viewer(ViewerNodeId::Source(
                                source_record.source_id.clone(),
                            )),
                            target,
                            MapRelationshipKind::SourceReference,
                            None,
                            MapRelationshipSource::Source {
                                source_id: source_record.source_id.clone(),
                                revision: source_record.revision,
                            },
                        )?);
                    }
                }
            }
        }
        ViewerDetail::Fact(fact) => {
            let source = MapRelationshipSource::Fact {
                fact_id: fact.fact_id.clone(),
                revision: fact.revision,
            };
            let target = MapObjectRef::Viewer(ViewerNodeId::Fact(fact.fact_id.clone()));
            for id in &fact.source_refs {
                rows.push(relation(
                    MapObjectRef::Viewer(ViewerNodeId::Source(id.clone())),
                    target.clone(),
                    MapRelationshipKind::SourceReference,
                    None,
                    source.clone(),
                )?);
            }
            for id in &fact.evidence_refs {
                rows.push(relation(
                    MapObjectRef::Viewer(ViewerNodeId::Evidence(id.clone())),
                    target.clone(),
                    MapRelationshipKind::EvidenceReference,
                    None,
                    source.clone(),
                )?);
            }
            for subject in &fact.subject_refs {
                if let Some(subject) = object_from_subject(subject) {
                    rows.push(relation(
                        target.clone(),
                        subject,
                        MapRelationshipKind::AppliesTo,
                        None,
                        source.clone(),
                    )?);
                }
            }
        }
        ViewerDetail::Region(region) => {
            let source = MapRelationshipSource::Region {
                region_id: region.region_id.clone(),
                revision: region.revision,
            };
            let target = MapObjectRef::Viewer(ViewerNodeId::Region(region.region_id.clone()));
            for parent in &region.parents {
                rows.push(relation(
                    MapObjectRef::Viewer(ViewerNodeId::Region(parent.clone())),
                    target.clone(),
                    MapRelationshipKind::Contains,
                    None,
                    source.clone(),
                )?);
            }
            for work in &region.work_refs {
                rows.push(relation(
                    target.clone(),
                    work_ref(work),
                    MapRelationshipKind::Affects,
                    None,
                    source.clone(),
                )?);
            }
            for child in &region.children {
                rows.push(relation(
                    target.clone(),
                    MapObjectRef::Viewer(ViewerNodeId::Region(child.clone())),
                    MapRelationshipKind::Contains,
                    None,
                    source.clone(),
                )?);
            }
            for subject in &region.subject_refs {
                if let Some(subject) = object_from_subject(subject) {
                    rows.push(relation(
                        target.clone(),
                        subject,
                        MapRelationshipKind::AppliesTo,
                        None,
                        source.clone(),
                    )?);
                }
            }
            for evidence in &region.evidence_refs {
                rows.push(relation(
                    MapObjectRef::Viewer(ViewerNodeId::Evidence(evidence.clone())),
                    target.clone(),
                    MapRelationshipKind::EvidenceReference,
                    None,
                    source.clone(),
                )?);
            }
        }
        _ => {}
    }
    Ok(rows)
}

pub(super) fn contract_relationship_gaps(contract: &TaskContractRecord) -> Vec<MapRelationshipGap> {
    contract
        .contract
        .read_subjects
        .iter()
        .chain(&contract.contract.write_subjects)
        .filter(|subject| object_from_subject(subject).is_none())
        .cloned()
        .map(|subject| MapRelationshipGap::UnrepresentableSubject { subject })
        .collect()
}

pub(super) fn detail_relationship_gaps(detail: &ViewerDetail) -> Vec<MapRelationshipGap> {
    let mut gaps = Vec::new();
    match detail {
        ViewerDetail::Contract(contract) => gaps.extend(contract_relationship_gaps(contract)),
        ViewerDetail::Source(source) => match &source.scope {
            SourceScope::Project => gaps.push(MapRelationshipGap::ProjectScopedSourcesOmitted),
            SourceScope::Unassessed => gaps.push(MapRelationshipGap::UnassessedSourceScope),
            SourceScope::Subjects(subjects) => gaps.extend(
                subjects
                    .iter()
                    .filter(|subject| object_from_subject(subject).is_none())
                    .cloned()
                    .map(|subject| MapRelationshipGap::UnrepresentableSubject { subject }),
            ),
        },
        ViewerDetail::Fact(fact) => gaps.extend(
            fact.subject_refs
                .iter()
                .filter(|subject| object_from_subject(subject).is_none())
                .cloned()
                .map(|subject| MapRelationshipGap::UnrepresentableSubject { subject }),
        ),
        ViewerDetail::Region(region) => gaps.extend(
            region
                .subject_refs
                .iter()
                .filter(|subject| object_from_subject(subject).is_none())
                .cloned()
                .map(|subject| MapRelationshipGap::UnrepresentableSubject { subject }),
        ),
        ViewerDetail::Evidence(_) => gaps.push(MapRelationshipGap::RecordRelationsNotProjected {
            semantic_type: MapSemanticType::Evidence,
        }),
        ViewerDetail::Hold(_) => gaps.push(MapRelationshipGap::RecordRelationsNotProjected {
            semantic_type: MapSemanticType::Hold,
        }),
        ViewerDetail::Decision(_) => gaps.push(MapRelationshipGap::RecordRelationsNotProjected {
            semantic_type: MapSemanticType::Decision,
        }),
        ViewerDetail::CandidateReview(_) => {
            gaps.push(MapRelationshipGap::RecordRelationsNotProjected {
                semantic_type: MapSemanticType::CandidateReview,
            });
        }
        ViewerDetail::Work(_) | ViewerDetail::Obligation(_) | ViewerDetail::Outcome(_) => {}
    }
    gaps
}

pub(super) fn knowledge_relationship(
    edge: &KnowledgeDependencyRecord,
) -> Result<Option<MapRelationship>, ZapError> {
    let Some(from) = object_from_endpoint(&edge.prerequisite) else {
        return Ok(None);
    };
    let Some(to) = object_from_endpoint(&edge.dependent) else {
        return Ok(None);
    };
    let kind = match edge.relation {
        DependencyRelation::DependsOn => MapRelationshipKind::WorkPrerequisite,
        DependencyRelation::DerivedFrom => MapRelationshipKind::DerivedFrom,
        DependencyRelation::Supports => MapRelationshipKind::Supports,
        DependencyRelation::Verifies => MapRelationshipKind::Verifies,
        DependencyRelation::Affects => MapRelationshipKind::Affects,
        DependencyRelation::Consumes => MapRelationshipKind::Consumes,
    };
    relation(
        from,
        to,
        kind,
        None,
        MapRelationshipSource::Knowledge {
            edge_id: edge.edge_id.clone(),
            revision: edge.revision,
        },
    )
    .map(Some)
}

pub(super) fn relation(
    from: MapObjectRef,
    to: MapObjectRef,
    kind: MapRelationshipKind,
    ownership_role: Option<crate::seams::OwnershipRole>,
    source: MapRelationshipSource,
) -> Result<MapRelationship, ZapError> {
    #[derive(Serialize)]
    struct Identity<'a> {
        from: &'a MapObjectRef,
        to: &'a MapObjectRef,
        kind: MapRelationshipKind,
        ownership_role: Option<crate::seams::OwnershipRole>,
        source: &'a MapRelationshipSource,
    }
    let id = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &Identity {
            from: &from,
            to: &to,
            kind,
            ownership_role,
            source: &source,
        },
    )?
    .digest();
    Ok(MapRelationship {
        id,
        from,
        to,
        kind,
        ownership_role,
        source,
    })
}

pub(super) fn normalize_relationships(rows: &mut Vec<MapRelationship>) {
    rows.sort_by_key(|row| row.id);
    rows.dedup_by_key(|row| row.id);
}

pub(super) fn landmark(kind: WorkKind) -> Option<MapLandmarkFacet> {
    match kind {
        WorkKind::Portfolio => Some(MapLandmarkFacet::Portfolio),
        WorkKind::Campaign => Some(MapLandmarkFacet::Campaign),
        WorkKind::Phase => Some(MapLandmarkFacet::Phase),
        WorkKind::Workstream => Some(MapLandmarkFacet::Workstream),
        WorkKind::Group => Some(MapLandmarkFacet::Group),
        WorkKind::Gate => Some(MapLandmarkFacet::Gate),
        WorkKind::Horizon => Some(MapLandmarkFacet::Horizon),
        _ => None,
    }
}

pub(super) fn object_from_endpoint(
    endpoint: &crate::knowledge::KnowledgeEndpoint,
) -> Option<MapObjectRef> {
    use crate::knowledge::KnowledgeEndpoint;
    Some(MapObjectRef::Viewer(match endpoint {
        KnowledgeEndpoint::Source(id) => ViewerNodeId::Source(id.clone()),
        KnowledgeEndpoint::Fact(id) => ViewerNodeId::Fact(id.clone()),
        KnowledgeEndpoint::Evidence(id) => ViewerNodeId::Evidence(id.clone()),
        KnowledgeEndpoint::Decision(id) => ViewerNodeId::Decision(id.clone()),
        KnowledgeEndpoint::Work(id) => ViewerNodeId::Work(id.clone()),
        KnowledgeEndpoint::Obligation(id) => ViewerNodeId::Obligation(id.clone()),
        KnowledgeEndpoint::Outcome(id) => ViewerNodeId::Outcome(id.clone()),
    }))
}

pub(super) fn object_from_subject(subject: &SubjectRef) -> Option<MapObjectRef> {
    Some(match subject {
        SubjectRef::Work(id) => work_ref(id),
        SubjectRef::Obligation(id) => MapObjectRef::Viewer(ViewerNodeId::Obligation(id.clone())),
        SubjectRef::Outcome(id) => MapObjectRef::Viewer(ViewerNodeId::Outcome(id.clone())),
        SubjectRef::Contract(id) => MapObjectRef::Viewer(ViewerNodeId::Contract(id.clone())),
        SubjectRef::Source(id) => MapObjectRef::Viewer(ViewerNodeId::Source(id.clone())),
        SubjectRef::Evidence(id) => MapObjectRef::Viewer(ViewerNodeId::Evidence(id.clone())),
        SubjectRef::Decision(id) => MapObjectRef::Viewer(ViewerNodeId::Decision(id.clone())),
        SubjectRef::Hold(id) => MapObjectRef::Viewer(ViewerNodeId::Hold(id.clone())),
        SubjectRef::Resource(id) => MapObjectRef::Resource(id.clone()),
        _ => return None,
    })
}

pub(super) fn work_ref(id: &WorkId) -> MapObjectRef {
    MapObjectRef::Viewer(ViewerNodeId::Work(id.clone()))
}
