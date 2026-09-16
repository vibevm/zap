use super::*;

#[test]
fn ordered_effects_keep_independent_local_basis_domains() -> Result<(), ZapError> {
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &42_u32)?;
    let first_id = EffectId::parse("effect.first")?;
    let first = EffectPreflightRequest::new(EffectPreflightRequestInput {
        effect_id: first_id.clone(),
        index: 0,
        kind: EventKind::parse("domain.review-applied")?,
        payload: payload.clone(),
        predecessors: Vec::new(),
        product_event_id: EventId::parse("event.first")?,
        basis: BasisRequest::new(crate::BasisRequestInput {
            purpose: crate::BasisPurpose::Mutation(EventKind::parse("domain.review-applied")?),
            roots: vec![SubjectRef::Review(zap_wire::ReviewId::parse("review.one")?)],
            policy: crate::ContextRequirement::Required,
            capacity: crate::ContextRequirement::NotApplicable,
            closure: crate::ClosureRequirement::KnownGraph,
        })?,
        declared_subjects: vec![SubjectRef::Review(zap_wire::ReviewId::parse("review.one")?)],
        relevant_before: RelevantBasisDigest::hash(b"review-before"),
        declared_relevant_after: RelevantBasisDigest::hash(b"review-after"),
    })?;
    let second = EffectPreflightRequest::new(EffectPreflightRequestInput {
        effect_id: EffectId::parse("effect.second")?,
        index: 1,
        kind: EventKind::parse("planning.lowering-applied")?,
        payload,
        predecessors: vec![first_id],
        product_event_id: EventId::parse("event.second")?,
        basis: BasisRequest::new(crate::BasisRequestInput {
            purpose: crate::BasisPurpose::Lowering(zap_wire::LoweringId::parse("lowering.one")?),
            roots: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
            policy: crate::ContextRequirement::Required,
            capacity: crate::ContextRequirement::NotApplicable,
            closure: crate::ClosureRequirement::KnownGraph,
        })?,
        declared_subjects: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
        relevant_before: RelevantBasisDigest::hash(b"lowering-before"),
        declared_relevant_after: RelevantBasisDigest::hash(b"lowering-after"),
    })?;
    let bundle = EffectBundleRequest::new(
        ChangeAlternativeId::parse("alternative.multi-kind")?,
        Vec::new(),
        RelevantBasisDigest::hash(b"comparison-basis"),
        vec![first, second],
    )?;
    assert_eq!(bundle.effects().len(), 2);
    assert_ne!(
        bundle.effects()[0].declared_relevant_after(),
        bundle.effects()[1].relevant_before()
    );
    Ok(())
}
