use super::*;

pub(super) fn milestone_card(
    snapshot: &dyn QuerySnapshot,
    id: &zap_wire::MilestoneId,
) -> Result<SemanticCard, ZapError> {
    use crate::milestones::{MilestoneContribution, MilestoneDependencyKind};
    let head = snapshot
        .get_typed::<crate::milestones::MilestoneRecord>(id)?
        .ok_or_else(object_missing)?;
    let revision = snapshot
        .get_typed::<crate::milestones::MilestoneRevisionRecord>(&head.current_revision_id)?
        .ok_or_else(object_missing)?;
    if revision.milestone_id != head.milestone_id
        || !crate::milestones::milestone_revision_is_self_consistent(&revision)?
    {
        return Err(map_error(
            ErrorCode::CorruptStore,
            "milestone head and revision disagree",
        ));
    }
    let object = MapObjectRef::Milestone(id.clone());
    let source = MapRelationshipSource::MilestoneRevision {
        milestone_id: id.clone(),
        revision_id: revision.revision_id.clone(),
        revision: revision.revision,
    };
    let mut relationships = vec![relation(
        object.clone(),
        MapObjectRef::Viewer(ViewerNodeId::Outcome(
            revision.definition.outcome_id.clone(),
        )),
        MapRelationshipKind::OutcomeScope,
        None,
        source.clone(),
    )?];
    let mut gaps = Vec::new();
    for obligation in &revision.definition.required_obligation_ids {
        relationships.push(relation(
            object.clone(),
            MapObjectRef::Viewer(ViewerNodeId::Obligation(obligation.clone())),
            MapRelationshipKind::CoversObligation,
            None,
            source.clone(),
        )?);
    }
    for consumer in &revision.definition.consumers {
        if let Some(target) = object_from_subject(consumer) {
            relationships.push(relation(
                object.clone(),
                target,
                MapRelationshipKind::Supports,
                None,
                source.clone(),
            )?);
        } else {
            gaps.push(MapRelationshipGap::UnrepresentableSubject {
                subject: consumer.clone(),
            });
        }
    }
    for contribution in &revision.definition.contributions {
        let from = match contribution {
            MilestoneContribution::Work { work_id } => work_ref(work_id),
            MilestoneContribution::Evidence { evidence_id } => {
                MapObjectRef::Viewer(ViewerNodeId::Evidence(evidence_id.clone()))
            }
            MilestoneContribution::Milestone { milestone_id, .. } => {
                MapObjectRef::Milestone(milestone_id.clone())
            }
        };
        relationships.push(relation(
            from,
            object.clone(),
            MapRelationshipKind::ContributesTo,
            None,
            source.clone(),
        )?);
    }
    for dependency in &revision.definition.dependencies {
        relationships.push(relation(
            MapObjectRef::Milestone(dependency.milestone_id.clone()),
            object.clone(),
            match dependency.kind {
                MilestoneDependencyKind::PreparationPrerequisite => {
                    MapRelationshipKind::MilestonePreparationPrerequisite
                }
                MilestoneDependencyKind::AchievementPrerequisite => {
                    MapRelationshipKind::MilestoneAchievementPrerequisite
                }
            },
            None,
            source.clone(),
        )?);
    }
    normalize_relationships(&mut relationships);
    let relationships_complete = gaps.is_empty();
    Ok(SemanticCard {
        object,
        semantic_type: MapSemanticType::Milestone,
        canonical_name: revision.definition.name.clone(),
        description: available(
            format!(
                "canonical outcome boundary for {}",
                revision.definition.outcome_id.as_str()
            ),
            MapTextSource::MilestoneRecord,
        )?,
        purpose: available(
            revision.definition.purpose.as_str().to_owned(),
            MapTextSource::MilestoneRecord,
        )?,
        expected_result: available(
            revision.definition.result_criterion.as_str().to_owned(),
            MapTextSource::MilestoneRecord,
        )?,
        source_state: MapSourceState::Materialized {
            record_revision: revision.revision,
        },
        acceptance: MapAcceptanceView::MilestoneState {
            lifecycle: revision.definition.lifecycle,
            latest_achievement_id: head.latest_achievement_id.clone(),
        },
        reasons: revision
            .definition
            .retirement_reason
            .iter()
            .cloned()
            .collect(),
        blockers: blockers_not_evaluated()?,
        sources: Vec::new(),
        evidence: revision.definition.evidence_ids(),
        work_kind: None,
        work_type: None,
        landmark: None,
        relationships,
        relationship_gaps: gaps,
        relationships_complete,
        assessment_source_fingerprint: Some(revision.semantic_fingerprint),
        assessment_state: MapAssessmentState::NotApplicable,
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: Some(MapUnderlyingDetail::Milestone {
            head,
            current_revision: Box::new(revision),
        }),
    })
}

pub(super) fn information_card(
    snapshot: &dyn QuerySnapshot,
    id: &zap_wire::InformationOpportunityId,
) -> Result<SemanticCard, ZapError> {
    let opportunity = snapshot
        .get_typed::<crate::information::InformationOpportunityRecord>(id)?
        .ok_or_else(object_missing)?;
    let selection = match opportunity.selection_id.as_ref() {
        Some(id) => Some(
            snapshot
                .get_typed::<crate::information::InformationSelectionRecord>(id)?
                .ok_or_else(|| {
                    map_error(
                        ErrorCode::CorruptStore,
                        "information selection link is missing",
                    )
                })?,
        ),
        None => None,
    };
    if selection.as_ref().is_some_and(|row| {
        row.opportunity_id != opportunity.opportunity_id
            || row.opportunity_revision != opportunity.revision
            || row.opportunity_fingerprint != opportunity.semantic_fingerprint
            || row.basis_fingerprint != opportunity.basis_fingerprint
    }) {
        return Err(map_error(
            ErrorCode::CorruptStore,
            "information selection ownership or source binding is inconsistent",
        ));
    }
    let freshness = crate::information::information_opportunity_freshness(snapshot, &opportunity)?;
    let object = MapObjectRef::InformationOpportunity(id.clone());
    let source = MapRelationshipSource::InformationOpportunity {
        opportunity_id: id.clone(),
        revision: opportunity.revision,
    };
    let mut gaps = Vec::new();
    let decision_object = match &opportunity.content.decision_basis {
        crate::information::InformationDecisionBasis::OwnerDecision { decision_id, .. } => Some(
            MapObjectRef::Viewer(ViewerNodeId::Decision(decision_id.clone())),
        ),
        crate::information::InformationDecisionBasis::StrategicFork {
            strategy_id,
            fork_id,
            ..
        } => Some(MapObjectRef::StrategicFork {
            strategy_id: strategy_id.clone(),
            fork_id: fork_id.clone(),
        }),
        crate::information::InformationDecisionBasis::Region { region_id, .. } => Some(
            MapObjectRef::Viewer(ViewerNodeId::Region(region_id.clone())),
        ),
        crate::information::InformationDecisionBasis::Outcome { outcome_id, .. } => Some(
            MapObjectRef::Viewer(ViewerNodeId::Outcome(outcome_id.clone())),
        ),
        crate::information::InformationDecisionBasis::Unresolved { .. } => {
            gaps.push(MapRelationshipGap::UnsupportedInformationDecisionBasis);
            None
        }
    };
    let mut relationships = Vec::new();
    if let Some(decision_object) = decision_object {
        relationships.push(relation(
            object.clone(),
            decision_object,
            MapRelationshipKind::DecisionSupport,
            None,
            source.clone(),
        )?);
    }
    for binding in &opportunity.content.sources {
        relationships.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Source(binding.capture.source_id.clone())),
            object.clone(),
            MapRelationshipKind::SourceReference,
            None,
            source.clone(),
        )?);
    }
    for region in &opportunity.content.regions {
        relationships.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Region(region.clone())),
            object.clone(),
            MapRelationshipKind::AppliesTo,
            None,
            source.clone(),
        )?);
    }
    for horizon in &opportunity.content.horizons {
        if let Some(target) = object_from_subject(&horizon.subject) {
            relationships.push(relation(
                target,
                object.clone(),
                MapRelationshipKind::AppliesTo,
                None,
                source.clone(),
            )?);
        } else {
            gaps.push(MapRelationshipGap::UnrepresentableSubject {
                subject: horizon.subject.clone(),
            });
        }
    }
    for evidence in &opportunity.content.satisfying_evidence_ids {
        relationships.push(relation(
            MapObjectRef::Viewer(ViewerNodeId::Evidence(evidence.clone())),
            object.clone(),
            MapRelationshipKind::EvidenceReference,
            None,
            source.clone(),
        )?);
    }
    if let Some(selected) = &selection {
        relationships.push(relation(
            object.clone(),
            work_ref(&selected.candidate_work_id),
            MapRelationshipKind::SelectedInformationWork,
            None,
            source.clone(),
        )?);
    }
    normalize_relationships(&mut relationships);
    Ok(SemanticCard {
        object,
        semantic_type: MapSemanticType::InformationOpportunity,
        canonical_name: opportunity.content.name.clone(),
        description: available(
            opportunity.content.observation_sought.as_str().to_owned(),
            MapTextSource::InformationOpportunityRecord,
        )?,
        purpose: available(
            format!(
                "supports decision {}",
                opportunity.content.decision_id.as_str()
            ),
            MapTextSource::InformationOpportunityRecord,
        )?,
        expected_result: available(
            opportunity.content.observation_sought.as_str().to_owned(),
            MapTextSource::InformationOpportunityRecord,
        )?,
        source_state: MapSourceState::Materialized {
            record_revision: opportunity.revision,
        },
        acceptance: MapAcceptanceView::InformationOpportunityState {
            freshness,
            selected_work_id: selection.as_ref().map(|row| row.candidate_work_id.clone()),
        },
        reasons: vec![opportunity.content.stop_rule.explanation.clone()],
        blockers: MapBlockerView::NotApplicable {
            reason: text("optional opportunity existence is not a completion blocker")?,
        },
        sources: opportunity.content.source_ids(),
        evidence: opportunity.content.satisfying_evidence_ids.clone(),
        work_kind: None,
        work_type: None,
        landmark: None,
        relationships,
        relationships_complete: gaps.is_empty(),
        relationship_gaps: gaps,
        assessment_source_fingerprint: Some(opportunity.basis_fingerprint),
        assessment_state: MapAssessmentState::NotApplicable,
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: Some(MapUnderlyingDetail::InformationOpportunity {
            opportunity: Box::new(opportunity),
            selection: selection.map(Box::new),
        }),
    })
}

pub(super) fn strategic_fork_card(
    snapshot: &dyn QuerySnapshot,
    strategy_id: &zap_wire::StrategicRevisionId,
    fork_id: &zap_wire::ForkId,
) -> Result<SemanticCard, ZapError> {
    let strategy = super::super::common::load_strategy(snapshot, strategy_id)?;
    let fork = strategy
        .forks
        .iter()
        .find(|row| &row.fork_id == fork_id)
        .ok_or_else(object_missing)?;
    Ok(SemanticCard {
        object: MapObjectRef::StrategicFork {
            strategy_id: strategy_id.clone(),
            fork_id: fork_id.clone(),
        },
        semantic_type: MapSemanticType::StrategicFork,
        canonical_name: text(fork.fork_id.as_str())?,
        description: available(
            fork.problem.as_str().to_owned(),
            MapTextSource::StrategicNode,
        )?,
        purpose: available(
            fork.diagnostic_action.as_str().to_owned(),
            MapTextSource::StrategicNode,
        )?,
        expected_result: available(
            format!("recommended alternative {}", fork.recommendation.as_str()),
            MapTextSource::StrategicNode,
        )?,
        source_state: MapSourceState::Materialized {
            record_revision: strategy.revision,
        },
        acceptance: MapAcceptanceView::NotApplicable {
            reason: text("a strategic fork is a decision surface, not accepted work")?,
        },
        reasons: Vec::new(),
        blockers: blockers_not_evaluated()?,
        sources: Vec::new(),
        evidence: Vec::new(),
        work_kind: None,
        work_type: None,
        landmark: None,
        relationships: Vec::new(),
        relationship_gaps: Vec::new(),
        relationships_complete: true,
        assessment_source_fingerprint: Some(strategy.semantic_digest),
        assessment_state: MapAssessmentState::NotApplicable,
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: None,
    })
}

pub(super) fn resource_card(
    snapshot: &dyn QuerySnapshot,
    id: &zap_wire::ResourceId,
) -> Result<SemanticCard, ZapError> {
    Ok(SemanticCard {
        object: MapObjectRef::Resource(id.clone()),
        semantic_type: MapSemanticType::ExecutionResource,
        canonical_name: text(id.as_str())?,
        description: missing("resource description is not materialized"),
        purpose: missing("resource purpose is not materialized"),
        expected_result: missing("resource capacity and availability are unknown"),
        source_state: MapSourceState::ReferenceOnly {
            reason: text("resource is a typed contract reference without a resource record")?,
        },
        acceptance: MapAcceptanceView::NotApplicable {
            reason: text("execution resources have no work acceptance state")?,
        },
        reasons: vec![text(
            "a resource reference does not establish capacity, availability, or occupancy",
        )?],
        blockers: MapBlockerView::NotApplicable {
            reason: text("execution resources do not have Work readiness blockers")?,
        },
        sources: Vec::new(),
        evidence: Vec::new(),
        work_kind: None,
        work_type: None,
        landmark: None,
        relationships: Vec::new(),
        relationship_gaps: vec![MapRelationshipGap::ResourceReverseIndexUnavailable],
        relationships_complete: false,
        assessment_source_fingerprint: None,
        assessment_state: MapAssessmentState::NotApplicable,
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: None,
    })
}
