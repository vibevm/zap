use super::*;

#[test]
fn query_reports_cheaper_decisive_unknown_cost_and_source_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("decision.redb"))?;
    seed_fixture(&harness)?;
    let cases = [
        (
            "information.expensive",
            opportunity("Run full probe", 2_000_000, 8_000_000, false)?,
        ),
        (
            "information.cheap",
            opportunity("Read manifest", 100_000, 8_000_000, false)?,
        ),
        (
            "information.unknown",
            opportunity("Ask vendor", 100_000, 8_000_000, true)?,
        ),
    ];
    let mut revision = Revision::new(1);
    for (index, (id, content)) in cases.into_iter().enumerate() {
        let basis = information_opportunity_basis(
            &harness.store.read(zap_core::ReadAt::Current)?,
            &content,
        )?;
        harness.commit(
            &InformationOpportunityProposed {
                schema: InformationOpportunityProposedSchema::V1,
                opportunity_id: InformationOpportunityId::parse(id)?,
                expected_opportunity_revision: None,
                expected_basis_fingerprint: basis,
                content,
            },
            revision,
            &format!("command.information.case.{index}"),
        )?;
        revision = revision.checked_next()?;
    }
    for index in 0..20 {
        let mut content = opportunity(
            &format!("Unrelated observation {index}"),
            100_000,
            8_000_000,
            false,
        )?;
        content.decision_id = DecisionId::parse("decision.unrelated")?;
        let basis = information_opportunity_basis(
            &harness.store.read(zap_core::ReadAt::Current)?,
            &content,
        )?;
        harness.commit(
            &InformationOpportunityProposed {
                schema: InformationOpportunityProposedSchema::V1,
                opportunity_id: InformationOpportunityId::parse(&format!(
                    "information.unrelated.{index:02}"
                ))?,
                expected_opportunity_revision: None,
                expected_basis_fingerprint: basis,
                content,
            },
            revision,
            &format!("command.information.unrelated.{index:02}"),
        )?;
        revision = revision.checked_next()?;
    }
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let page = InformationOpportunityQuery.execute(
        &snapshot,
        &InformationOpportunityQueryInput {
            decision_id: DecisionId::parse("decision.compatibility")?,
            cursor: None,
            limit: 3,
            operation_budget: 8,
        },
    )?;
    let by_id = page.items[0]
        .recommendations
        .iter()
        .map(|row| (row.opportunity_id.as_str(), row))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        by_id["information.expensive"].kind,
        InformationRecommendationKind::CheaperDecisive
    );
    assert_eq!(
        by_id["information.cheap"].kind,
        InformationRecommendationKind::Worthwhile
    );
    assert_eq!(
        by_id["information.unknown"].kind,
        InformationRecommendationKind::Unknown
    );
    drop(snapshot);

    let mut source = source_record()?;
    source.current.digest = SourceDigest::hash(b"changed source");
    source.capture_status = SourceCaptureStatus::Changed;
    source.revision = revision.checked_next()?;
    harness.seed(
        &SeedMutation::Drift { source },
        revision,
        "command.information.drift",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let page = InformationOpportunityQuery.execute(
        &snapshot,
        &InformationOpportunityQueryInput {
            decision_id: DecisionId::parse("decision.compatibility")?,
            cursor: None,
            limit: 8,
            operation_budget: 32,
        },
    )?;
    assert!(
        page.items[0]
            .recommendations
            .iter()
            .all(|row| row.kind == InformationRecommendationKind::Unknown)
    );
    Ok(())
}

#[test]
fn honest_unknown_benefit_can_be_selected_without_creating_work()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("unknown-selection.redb"))?;
    seed_fixture(&harness)?;
    let mut content = opportunity("Inspect uncertain compatibility", 100_000, 1, false)?;
    content.decision_id = DecisionId::parse("decision.uncertain")?;
    content.benefit = DecisionBenefit::Unknown {
        question: BoundedText::parse("How much rework could this prevent?")?,
        resolution_action: BoundedText::parse("Inspect the bounded compatibility evidence")?,
    };
    let basis =
        information_opportunity_basis(&harness.store.read(zap_core::ReadAt::Current)?, &content)?;
    harness.commit(
        &InformationOpportunityProposed {
            schema: InformationOpportunityProposedSchema::V1,
            opportunity_id: InformationOpportunityId::parse("information.uncertain")?,
            expected_opportunity_revision: None,
            expected_basis_fingerprint: basis,
            content,
        },
        Revision::new(1),
        "command.information.uncertain",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let page = InformationOpportunityQuery.execute(
        &snapshot,
        &InformationOpportunityQueryInput {
            decision_id: DecisionId::parse("decision.uncertain")?,
            cursor: None,
            limit: 1,
            operation_budget: 4,
        },
    )?;
    let recommendation = page.items[0].recommendations[0].clone();
    assert_eq!(recommendation.kind, InformationRecommendationKind::Unknown);
    drop(snapshot);
    let work_id = WorkId::parse("work.information.uncertain")?;
    harness.commit(
        &InformationSelectionProposed {
            schema: InformationSelectionProposedSchema::V1,
            selection_id: InformationSelectionId::parse("selection.uncertain")?,
            expected_selection_revision: None,
            opportunity_id: recommendation.opportunity_id,
            expected_opportunity_revision: recommendation.opportunity_revision,
            expected_opportunity_fingerprint: recommendation.opportunity_fingerprint,
            expected_basis_fingerprint: recommendation.basis_fingerprint,
            candidate_work_id: work_id.clone(),
            work_type: WorkType::Decision,
            recommendation: InformationRecommendationKind::Unknown,
            rationale: BoundedText::parse(
                "The uncertainty is material enough to propose bounded decision work",
            )?,
        },
        Revision::new(2),
        "command.information.select-uncertain",
    )?;
    assert!(
        harness
            .store
            .read(zap_core::ReadAt::Current)?
            .get_typed::<zap_domain::control::WorkRecord>(&work_id)?
            .is_none()
    );
    Ok(())
}
