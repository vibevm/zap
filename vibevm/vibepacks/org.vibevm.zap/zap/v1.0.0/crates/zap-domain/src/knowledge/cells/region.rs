use std::collections::BTreeSet;

use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, PayloadBasisScope, StateReader, StateReaderExt, StoredRecord,
    ValidatedCommand,
};
use zap_wire::{
    ActionClass, BasisBinding, ErrorCode, ErrorDetail, EventKind, FixSurface, RouteClass,
    SubjectRef, ZapError,
};

use super::{Operation, OperationCell};
use crate::control::WorkRecord;
use crate::knowledge::{
    RegionMerged, RegionRecord, RegionRelevanceSet, RegionSplit, RegionTransitioned, merge_regions,
    set_region_relevance, split_region, transition_region,
};
use crate::seams::TypedActionImpact;
use crate::seams::{impl_command_payload, scan_all};

const REGION_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#FOG-RECOMPUTATION";

impl_command_payload!(RegionTransitioned, "knowledge.region-transitioned");
impl_command_payload!(RegionRelevanceSet, "knowledge.region-relevance-set");
impl_command_payload!(RegionSplit, "knowledge.region-split");
impl_command_payload!(RegionMerged, "knowledge.region-merged");

pub(super) struct TransitionRegion;
impl Operation for TransitionRegion {
    type Payload = RegionTransitioned;
    const REQUIREMENT: &'static str = REGION_REQ;
    const FAMILIES: &'static [&'static str] = &[RegionRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::Privileged(ActionClass::parse(
            "evidence.adjudicate",
        )?))
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let current = region(state, &payload.region_id)?;
        let basis = command_basis(command)?;
        if payload.basis != basis
            || payload.subject_refs != current.subject_refs
            || payload.work_refs != current.work_refs
        {
            return Err(invalid_region());
        }
        let evidence = region_evidence(
            state,
            &payload.evidence_refs,
            basis,
            &current.subject_refs,
            &current.work_refs,
        )?;
        changes.replace(
            current.revision,
            transition_region(&current, payload, evidence.as_ref())?,
        )
    }
}

pub(super) struct SetRegionRelevance;
impl Operation for SetRegionRelevance {
    type Payload = RegionRelevanceSet;
    const REQUIREMENT: &'static str = REGION_REQ;
    const FAMILIES: &'static [&'static str] = &[RegionRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::Privileged(ActionClass::parse(
            "evidence.adjudicate",
        )?))
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let current = region(state, &payload.region_id)?;
        let basis = command_basis(command)?;
        if payload.basis != basis
            || payload.subject_refs != current.subject_refs
            || payload.work_refs != current.work_refs
        {
            return Err(invalid_region());
        }
        let evidence = region_evidence(
            state,
            &payload.evidence_refs,
            basis,
            &current.subject_refs,
            &current.work_refs,
        )?;
        changes.replace(
            current.revision,
            set_region_relevance(&current, payload, evidence.as_ref())?,
        )
    }
}

pub(super) struct SplitRegion;
impl Operation for SplitRegion {
    type Payload = RegionSplit;
    const REQUIREMENT: &'static str = REGION_REQ;
    const FAMILIES: &'static [&'static str] = &[RegionRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::DataProposal)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let current = region(state, &payload.region_id)?;
        let known_work: BTreeSet<_> = scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| row.work_id)
            .collect();
        for child in &payload.children {
            if state.get_typed::<RegionRecord>(&child.region_id)?.is_some()
                || child.work_refs.iter().any(|id| !known_work.contains(id))
            {
                return Err(invalid_region());
            }
        }
        let (parent, children) = split_region(&current, payload)?;
        changes.replace(current.revision, parent)?;
        for child in children {
            changes.insert(child)?;
        }
        Ok(())
    }
}

pub(super) struct MergeRegions;
impl Operation for MergeRegions {
    type Payload = RegionMerged;
    const REQUIREMENT: &'static str = REGION_REQ;
    const FAMILIES: &'static [&'static str] = &[RegionRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::DataProposal)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        if state
            .get_typed::<RegionRecord>(&payload.merged.region_id)?
            .is_some()
        {
            return Err(invalid_region());
        }
        let origins = payload
            .region_ids
            .iter()
            .map(|id| region(state, id))
            .collect::<Result<Vec<_>, _>>()?;
        let known_work: BTreeSet<_> = scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| row.work_id)
            .collect();
        if payload
            .merged
            .work_refs
            .iter()
            .any(|id| !known_work.contains(id))
        {
            return Err(invalid_region());
        }
        let expected: Vec<_> = origins.iter().map(|row| row.revision).collect();
        let (origins, merged) = merge_regions(&origins, payload)?;
        for (expected, after) in expected.into_iter().zip(origins) {
            changes.replace(expected, after)?;
        }
        changes.insert(merged)
    }
}

fn region(
    state: &dyn StateReader,
    id: &crate::knowledge::RegionId,
) -> Result<RegionRecord, ZapError> {
    state
        .get_typed::<RegionRecord>(id)?
        .ok_or_else(invalid_region)
}

fn invalid_region() -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        REGION_REQ,
        "region or one of its typed work references is missing",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn command_basis<P: zap_core::CommandPayload>(
    command: &ValidatedCommand<P>,
) -> Result<zap_wire::RelevantBasisDigest, ZapError> {
    match command.header().basis() {
        BasisBinding::Exact(basis) => Ok(*basis),
        BasisBinding::NotApplicable => Err(invalid_region()),
    }
}

fn region_evidence(
    state: &dyn StateReader,
    evidence_ids: &[zap_wire::EvidenceId],
    basis: zap_wire::RelevantBasisDigest,
    subject_refs: &[SubjectRef],
    work_refs: &[zap_wire::WorkId],
) -> Result<Option<crate::knowledge::ScopedEvidenceWitness>, ZapError> {
    if evidence_ids.is_empty() {
        return Ok(None);
    }
    let mut required_subjects: BTreeSet<_> = subject_refs.iter().cloned().collect();
    required_subjects.extend(work_refs.iter().cloned().map(SubjectRef::Work));
    let required_sources = required_subjects
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Source(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    Ok(Some(crate::knowledge::proof::scoped_evidence_witness(
        state,
        evidence_ids,
        basis,
        &required_subjects,
        &required_sources,
    )?))
}

fn region_basis_request(
    kind: &'static str,
    subject_refs: &[SubjectRef],
    work_refs: &[zap_wire::WorkId],
) -> Result<BasisRequest, ZapError> {
    if !subject_refs.windows(2).all(|pair| pair[0] < pair[1])
        || !work_refs.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(invalid_region());
    }
    let mut roots: BTreeSet<_> = subject_refs.iter().cloned().collect();
    roots.extend(work_refs.iter().cloned().map(SubjectRef::Work));
    if roots.is_empty() {
        return Err(invalid_region());
    }
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots: roots.into_iter().collect(),
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

pub(super) struct RegionTransitionBasisScope;
impl PayloadBasisScope<RegionTransitioned> for RegionTransitionBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &RegionTransitioned,
    ) -> Result<BasisRequest, ZapError> {
        region_basis_request(
            RegionTransitioned::KIND,
            &payload.subject_refs,
            &payload.work_refs,
        )
    }
}

pub(super) struct RegionRelevanceBasisScope;
impl PayloadBasisScope<RegionRelevanceSet> for RegionRelevanceBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &RegionRelevanceSet,
    ) -> Result<BasisRequest, ZapError> {
        region_basis_request(
            RegionRelevanceSet::KIND,
            &payload.subject_refs,
            &payload.work_refs,
        )
    }
}

pub(super) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(OperationCell::<TransitionRegion>::new())
            .basis(RegionTransitionBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &RegionTransitioned| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    payload.work_refs.clone(),
                    payload.subject_refs.clone(),
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<SetRegionRelevance>::new())
            .basis(RegionRelevanceBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &RegionRelevanceSet| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    payload.work_refs.clone(),
                    payload.subject_refs.clone(),
                )
            }))?
            .build()?,
        CellSet::single(OperationCell::<SplitRegion>::new())?,
        CellSet::single(OperationCell::<MergeRegions>::new())?,
    ])
}
