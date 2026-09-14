use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, ReadAt, TransactionStore,
};
use zap_domain::knowledge::DomainBasisProvider;
use zap_domain::milestones::MilestoneCreated;
use zap_wire::{CanonicalDecode, CanonicalPayload, CodecEpoch, EventKind, QueryId, SubjectRef};

use crate::support::Harness;

pub(super) fn milestone_basis(
    harness: &Harness,
    payload: &MilestoneCreated,
) -> Result<zap_wire::RelevantBasisDigest, Box<dyn std::error::Error>> {
    let roots = vec![
        SubjectRef::Outcome(payload.definition.outcome_id.clone()),
        SubjectRef::Obligation(payload.definition.required_obligation_ids[0].clone()),
    ];
    mutation_basis(harness, "milestone.created", roots)
}

pub(super) fn mutation_basis(
    harness: &Harness,
    kind: &str,
    roots: Vec<SubjectRef>,
) -> Result<zap_wire::RelevantBasisDigest, Box<dyn std::error::Error>> {
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    Ok(DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest)
}

pub(super) fn checked<T>(
    stage: &'static str,
    result: Result<T, zap_wire::ZapError>,
) -> Result<T, Box<dyn std::error::Error>> {
    result.map_err(|error| std::io::Error::other(format!("{stage}: {error}")).into())
}

pub(super) fn rejected<T>(
    result: Result<T, zap_wire::ZapError>,
    accepted_message: &'static str,
) -> Result<zap_wire::ZapError, Box<dyn std::error::Error>> {
    match result {
        Err(error) => Ok(error),
        Ok(_) => Err(accepted_message.into()),
    }
}

pub(super) fn query<I, O>(harness: &Harness, id: &str, input: &I) -> Result<O, zap_wire::ZapError>
where
    I: Serialize,
    O: CanonicalDecode,
{
    let snapshot = harness.store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(CodecEpoch::CURRENT, input)?;
    let page = zap_domain::query_set()?.execute(&QueryId::parse(id)?, &snapshot, &input)?;
    let item = page.items.first().ok_or_else(|| {
        zap_wire::ZapError::from_static(
            zap_wire::ErrorCode::InternalInvariant,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-QUERIES",
            "milestone planning query returned no item",
            zap_wire::FixSurface::Configuration,
            zap_wire::ErrorDetail::None,
        )
    })?;
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item.as_bytes())?;
    O::decode_canonical(&payload)
}
use serde::Serialize;
