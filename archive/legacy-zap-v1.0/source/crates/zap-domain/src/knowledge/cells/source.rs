use std::collections::{BTreeMap, BTreeSet};

use zap_core::{CellSet, ChangeSet, StateReader, StateReaderExt, StoredRecord, ValidatedCommand};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, RouteClass, ZapError};

use super::{Operation, OperationCell};
use crate::acceptance::EvidenceAdjudicationRecord;
use crate::knowledge::{
    FactRecord, KnowledgeClosureRecord, KnowledgeDependencyRecord, KnowledgeEndpoint,
    NativeFactsRecorded, SourceApplicabilityRecord, SourceApplicabilityStatus,
    SourceObservationCandidateRecord, SourceObservationProposed, SourceObserved, SourceRecaptured,
    SourceRecord, SourceRecorded, invalidate_evidence, invalidate_facts, invalidation_closure,
    observe_source, recapture_source, record_native_facts, record_observation_candidate,
    record_source,
};
use crate::seams::{impl_command_payload, scan_all};

const SOURCE_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#SOURCE-HANDLES";
const INVALIDATION_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#DEPENDENT-INVALIDATION";

impl_command_payload!(SourceRecorded, "knowledge.source-recorded");
impl_command_payload!(NativeFactsRecorded, "knowledge.native-facts-recorded");
impl_command_payload!(SourceRecaptured, "knowledge.source-recaptured");
impl_command_payload!(
    SourceObservationProposed,
    "knowledge.source-observation-recorded"
);
impl_command_payload!(SourceObserved, "knowledge.source-observed");

pub(super) struct RecordSource;
impl Operation for RecordSource {
    type Payload = SourceRecorded;
    const REQUIREMENT: &'static str = SOURCE_REQ;
    const FAMILIES: &'static [&'static str] = &[SourceRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::TrustedObservation)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let source = record_source(&command.payload().source)?;
        if state
            .get_typed::<SourceRecord>(&source.source_id)?
            .is_some()
        {
            return Err(duplicate_source());
        }
        changes.insert(source)
    }
}

pub(super) struct RecordNativeFacts;
impl Operation for RecordNativeFacts {
    type Payload = NativeFactsRecorded;
    const REQUIREMENT: &'static str = SOURCE_REQ;
    const FAMILIES: &'static [&'static str] = &[
        FactRecord::FAMILY,
        KnowledgeDependencyRecord::FAMILY,
        SourceRecord::FAMILY,
    ];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::TrustedObservation)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let source = record_source(&command.payload().source)?;
        if state
            .get_typed::<SourceRecord>(&source.source_id)?
            .is_some()
        {
            return Err(duplicate_source());
        }
        let (facts, dependencies) = record_native_facts(&source, &command.payload().facts)?;
        for fact in &facts {
            if state.get_typed::<FactRecord>(&fact.fact_id)?.is_some() {
                return Err(ZapError::from_static(
                    ErrorCode::DuplicateIdentity,
                    SOURCE_REQ,
                    "native fact identity already exists",
                    FixSurface::Payload,
                    ErrorDetail::None,
                ));
            }
        }
        changes.insert(source)?;
        for fact in facts {
            changes.insert(fact)?;
        }
        for dependency in dependencies {
            changes.insert(dependency)?;
        }
        Ok(())
    }
}

pub(super) struct RecaptureSource;
impl Operation for RecaptureSource {
    type Payload = SourceRecaptured;
    const REQUIREMENT: &'static str = INVALIDATION_REQ;
    const FAMILIES: &'static [&'static str] = &[
        EvidenceAdjudicationRecord::FAMILY,
        FactRecord::FAMILY,
        KnowledgeClosureRecord::FAMILY,
        SourceApplicabilityRecord::FAMILY,
        SourceRecord::FAMILY,
    ];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::TrustedObservation)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let current = state
            .get_typed::<SourceRecord>(&payload.source.source_id)?
            .ok_or_else(missing_source)?;
        let (next, changed) = recapture_source(&current, payload.previous_digest, &payload.source)?;
        changes.replace(current.revision, next)?;
        if changed {
            apply_invalidation(state, &current.source_id, changes)?;
        }
        Ok(())
    }
}

pub(super) struct ProposeSourceObservation;
impl Operation for ProposeSourceObservation {
    type Payload = SourceObservationProposed;
    const REQUIREMENT: &'static str = SOURCE_REQ;
    const FAMILIES: &'static [&'static str] = &[SourceObservationCandidateRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::DataProposal)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let candidate = record_observation_candidate(command.payload())?;
        if state
            .get_typed::<SourceRecord>(&candidate.source_id)?
            .is_none()
        {
            return Err(missing_source());
        }
        changes.insert(candidate)
    }
}

pub(super) struct ObserveSource;
impl Operation for ObserveSource {
    type Payload = SourceObserved;
    const REQUIREMENT: &'static str = INVALIDATION_REQ;
    const FAMILIES: &'static [&'static str] = &[
        EvidenceAdjudicationRecord::FAMILY,
        FactRecord::FAMILY,
        KnowledgeClosureRecord::FAMILY,
        SourceApplicabilityRecord::FAMILY,
        SourceRecord::FAMILY,
    ];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::TrustedObservation)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let current = state
            .get_typed::<SourceRecord>(&payload.observed.source_id)?
            .ok_or_else(missing_source)?;
        let (next, changed) = observe_source(&current, &payload.observed)?;
        changes.replace(current.revision, next)?;
        if changed {
            apply_invalidation(state, &current.source_id, changes)?;
        }
        Ok(())
    }
}

fn apply_invalidation(
    state: &dyn StateReader,
    source_id: &zap_wire::SourceId,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let edges = scan_all::<KnowledgeDependencyRecord>(state)?;
    let original_evidence = scan_all::<EvidenceAdjudicationRecord>(state)?;
    let original_facts = scan_all::<FactRecord>(state)?;
    let closures: BTreeMap<_, _> = scan_all::<KnowledgeClosureRecord>(state)?
        .into_iter()
        .map(|row| (row.subject.clone(), row))
        .collect();
    let mut roots = vec![KnowledgeEndpoint::Source(source_id.clone())];
    roots.extend(
        original_evidence
            .iter()
            .filter(|row| {
                row.source_captures
                    .iter()
                    .any(|capture| &capture.source_id == source_id)
            })
            .map(|row| KnowledgeEndpoint::Evidence(row.evidence_id.clone())),
    );
    roots.extend(
        original_facts
            .iter()
            .filter(|row| row.source_refs.binary_search(source_id).is_ok())
            .map(|row| KnowledgeEndpoint::Fact(row.fact_id.clone())),
    );
    roots.sort();
    roots.dedup();
    let closure = invalidation_closure(&edges, &closures, &roots)?;
    let affected: BTreeSet<_> = closure.affected.into_iter().collect();

    let mut evidence = original_evidence.clone();
    invalidate_evidence(&affected, &mut evidence)?;
    for (before, after) in original_evidence.iter().zip(evidence) {
        if before != &after {
            changes.replace(before.revision, after)?;
        }
    }
    let mut facts = original_facts.clone();
    invalidate_facts(&affected, &mut facts)?;
    for (before, after) in original_facts.iter().zip(facts) {
        if before != &after {
            changes.replace(before.revision, after)?;
        }
    }
    if let Some(mut applicability) = state.get_typed::<SourceApplicabilityRecord>(source_id)? {
        let expected = applicability.revision;
        applicability.status = SourceApplicabilityStatus::Stale;
        applicability.revision = applicability.revision.checked_next()?;
        changes.replace(expected, applicability)?;
    }
    for mut record in closures.into_values() {
        if affected.contains(&record.subject)
            && record.status == crate::knowledge::ClosureStatus::Complete
        {
            let expected = record.revision;
            record.status = crate::knowledge::ClosureStatus::Unknown;
            record.revision = record.revision.checked_next()?;
            changes.replace(expected, record)?;
        }
    }
    Ok(())
}

fn duplicate_source() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        SOURCE_REQ,
        "source identity already exists",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn missing_source() -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        SOURCE_REQ,
        "captured source is missing",
        FixSurface::SourceCapture,
        ErrorDetail::None,
    )
}

pub(super) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellSet::single(OperationCell::<RecordSource>::new())?,
        CellSet::single(OperationCell::<RecordNativeFacts>::new())?,
        CellSet::single(OperationCell::<RecaptureSource>::new())?,
        CellSet::single(OperationCell::<ProposeSourceObservation>::new())?,
        CellSet::single(OperationCell::<ObserveSource>::new())?,
    ])
}
