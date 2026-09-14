use tempfile::tempdir;
use zap_core::TransactionStore;
use zap_domain::economics::{HoursInterval, HoursMicros};
use zap_domain::map_assessment::{MapAssessmentGrade, work_assessment_basis};
use zap_domain::seams::WorkKind;
use zap_wire::{BoundedText, ErrorCode, EvidenceId, Revision, WorkId};

use super::support::{Harness, TestMutation, proposal, valid_content, work};

#[test]
fn malformed_descriptive_metadata_refuses_after_decode() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("invalid.redb"))?;
    let work_id = WorkId::parse("work.map.invalid")?;
    harness.internal(
        &TestMutation::Seed {
            work: vec![work(
                "work.map.invalid",
                "Validation work",
                WorkKind::Horizon,
            )?],
            contracts: Vec::new(),
        },
        Revision::GENESIS,
        "command.map.invalid-seed",
    )?;
    let basis = work_assessment_basis(&harness.store.read(zap_core::ReadAt::Current)?, &work_id)?;

    let mut cases = Vec::new();
    let mut invalid_range = valid_content()?;
    invalid_range
        .remaining_agent_hours
        .as_mut()
        .ok_or("agent estimate missing")?
        .range = HoursInterval {
        low: HoursMicros::new(9),
        high: Some(HoursMicros::new(8)),
    };
    cases.push(("range", invalid_range));

    let mut complexity = valid_content()?;
    complexity.complexity.rationale = None;
    cases.push(("complexity", complexity));

    let mut difficulty = valid_content()?;
    difficulty.difficulty.executor_assumptions.clear();
    cases.push(("difficulty", difficulty));

    let mut estimate_assumptions = valid_content()?;
    estimate_assumptions
        .remaining_elapsed
        .as_mut()
        .ok_or("elapsed estimate missing")?
        .assumptions
        .clear();
    cases.push(("estimate-assumptions", estimate_assumptions));

    let mut whitespace = valid_content()?;
    whitespace.display_label = Some(BoundedText::parse("  \t")?);
    cases.push(("whitespace", whitespace));

    let mut clock = valid_content()?;
    let elapsed = clock
        .remaining_elapsed
        .as_ref()
        .ok_or("elapsed estimate missing")?
        .range;
    let wait = HoursInterval::new(
        HoursMicros::new(7_000_000),
        Some(HoursMicros::new(8_000_000)),
    )?;
    assert!(HoursInterval::new(elapsed.low, elapsed.high).is_ok());
    assert!(HoursInterval::new(wait.low, wait.high).is_ok());
    clock
        .remaining_passive_wait
        .as_mut()
        .ok_or("wait estimate missing")?
        .range = wait;
    cases.push(("clock", clock));

    let mut references = valid_content()?;
    references.evidence_refs = (0..129)
        .map(|index| EvidenceId::parse(&format!("evidence.map.{index:03}")))
        .collect::<Result<Vec<_>, _>>()?;
    cases.push(("reference-bound", references));

    for (name, content) in cases {
        let payload = proposal(&work_id, None, basis, content);
        let frame = harness.frame(
            &payload,
            Revision::new(1),
            &format!("command.map.invalid.{name}"),
        )?;
        assert_eq!(
            harness.data(frame).err().map(|error| error.code),
            Some(ErrorCode::InvalidValue),
            "case {name}"
        );
        assert_eq!(harness.store.head()?, Revision::new(1));
    }

    let mut missing_evidence = valid_content()?;
    missing_evidence.evidence_refs = vec![EvidenceId::parse("evidence.map.missing")?];
    let payload = proposal(&work_id, None, basis, missing_evidence);
    let frame = harness.frame(
        &payload,
        Revision::new(1),
        "command.map.invalid.missing-evidence",
    )?;
    assert_eq!(
        harness.data(frame).err().map(|error| error.code),
        Some(ErrorCode::MissingReference)
    );
    assert_eq!(harness.store.head()?, Revision::new(1));
    Ok(())
}

#[test]
fn unassessed_context_is_retained_and_record_cas_is_exact() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("cas.redb"))?;
    let work_id = WorkId::parse("work.map.cas")?;
    let other_id = WorkId::parse("work.map.cas-other")?;
    harness.internal(
        &TestMutation::Seed {
            work: vec![
                work("work.map.cas", "CAS work", WorkKind::Phase)?,
                work("work.map.cas-other", "CAS other", WorkKind::Workstream)?,
            ],
            contracts: Vec::new(),
        },
        Revision::GENESIS,
        "command.map.cas-seed",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let basis = work_assessment_basis(&snapshot, &work_id)?;
    let other_basis = work_assessment_basis(&snapshot, &other_id)?;
    let mut content = valid_content()?;
    content.complexity.grade = MapAssessmentGrade::Unassessed;
    content.complexity.rationale = Some(BoundedText::parse("Waiting for dependency evidence")?);
    content.difficulty.grade = MapAssessmentGrade::Unassessed;
    content.difficulty.rationale = None;
    content.difficulty.knowledge_assumptions.clear();
    content.uncertainty.confidence =
        zap_domain::map_assessment::MapAssessmentConfidence::Unassessed;
    content.uncertainty.rationale = Some(BoundedText::parse("Confidence is not yet graded")?);
    let create = proposal(&work_id, None, basis, content);
    let frame = harness.frame(&create, Revision::new(1), "command.map.cas-create")?;
    harness.data(frame)?;

    let duplicate = proposal(&work_id, None, basis, valid_content()?);
    let frame = harness.frame(&duplicate, Revision::new(2), "command.map.cas-duplicate")?;
    assert_eq!(
        harness.data(frame).err().map(|error| error.code),
        Some(ErrorCode::DuplicateIdentity)
    );
    let stale = proposal(&work_id, Some(Revision::new(1)), basis, valid_content()?);
    let frame = harness.frame(&stale, Revision::new(2), "command.map.cas-stale")?;
    assert_eq!(
        harness.data(frame).err().map(|error| error.code),
        Some(ErrorCode::StaleRevision)
    );
    let missing = proposal(
        &other_id,
        Some(Revision::new(1)),
        other_basis,
        valid_content()?,
    );
    let frame = harness.frame(&missing, Revision::new(2), "command.map.cas-missing")?;
    assert_eq!(
        harness.data(frame).err().map(|error| error.code),
        Some(ErrorCode::MissingReference)
    );
    assert_eq!(harness.store.head()?, Revision::new(2));
    Ok(())
}
