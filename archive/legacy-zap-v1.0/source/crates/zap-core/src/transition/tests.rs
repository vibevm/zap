use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zap_wire::{
    BaseId, BoundedText, CampaignId, CanonicalCommandFrame, CodecEpoch, CommandHeaderInput,
    CommandId, CommandReasonInput, EventId, PrincipalId, ProtocolEpoch, ReducerEpoch, Revision,
    StoreEpoch, StoreId,
};

use super::*;
use crate::{
    ActorRef, CellDescriptorInput, EncodedKeyRange, EncodedRecordKey, ErasedRecord,
    ErasedRecordPage, OperationRef, PageLimit, PrincipalRole, RecordCompleteness, RecordFamily,
    RequirementRef, StoreIdentity,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EchoPayload {
    value: u64,
}

impl CanonicalEncode for EchoPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for EchoPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for EchoPayload {
    const KIND: &'static str = "fixture.echo";
}

struct EchoCell;

impl TransitionCell for EchoCell {
    type Payload = EchoPayload;
    type Output = EchoPayload;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(EchoPayload::KIND)?,
            route: RouteClass::DataProposal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: Vec::new(),
            affected_indexes: Vec::new(),
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        _changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        Ok(command.payload().clone())
    }
}

struct PrivilegedEchoCell;

impl TransitionCell for PrivilegedEchoCell {
    type Payload = EchoPayload;
    type Output = EchoPayload;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(EchoPayload::KIND)?,
            route: RouteClass::Privileged(zap_wire::ActionClass::parse("fixture.echo")?),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: Vec::new(),
            affected_indexes: Vec::new(),
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SEMANTIC-CHANGE-BOUNDARY",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        EchoCell.apply(state, command, changes)
    }
}

struct EchoImpact;

impl PayloadActionImpact<EchoPayload> for EchoImpact {
    fn request(&self, _payload: &EchoPayload) -> Result<ActionImpactRequest, ZapError> {
        ActionImpactRequest::new(crate::ActionImpactRule::Progress, Vec::new(), Vec::new())
    }
}

#[test]
fn typed_action_impact_registration_erases_only_after_payload_decode() -> Result<(), ZapError> {
    let cells = CellRegistrationBuilder::new(PrivilegedEchoCell)
        .action_impact(EchoImpact)?
        .build()?;
    let kind = EventKind::parse(EchoPayload::KIND)?;
    let cell = cells.cell(&kind).ok_or_else(registry_invariant)?;
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &EchoPayload { value: 7 })?;
    let decoded = cell.decode_payload(&payload)?;
    let request = cell
        .action_impact_request(decoded.as_ref())?
        .ok_or_else(registry_invariant)?;
    assert_eq!(request.rule(), &crate::ActionImpactRule::Progress);
    assert!(request.work_ids().is_empty());
    assert!(request.subjects().is_empty());
    let duplicate = CellRegistrationBuilder::new(PrivilegedEchoCell)
        .action_impact(EchoImpact)?
        .action_impact(EchoImpact)
        .err()
        .map(|error| error.code);
    assert_eq!(duplicate, Some(ErrorCode::DuplicateIdentity));
    Ok(())
}

struct EmptyState {
    identity: StoreIdentity,
}

impl StateReader for EmptyState {
    fn identity(&self) -> StoreIdentity {
        self.identity.clone()
    }

    fn revision(&self) -> Revision {
        Revision::GENESIS
    }

    fn get_erased(
        &self,
        _family: &RecordFamily,
        _key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        Ok(None)
    }

    fn scan_erased(
        &self,
        _family: &RecordFamily,
        _range: EncodedKeyRange,
        _limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        Ok(ErasedRecordPage {
            items: Vec::new(),
            completeness: RecordCompleteness::Complete,
            last_key: None,
        })
    }
}

struct MemoryHarness {
    cells: CellSet,
    state: EmptyState,
    seen: BTreeMap<CommandId, (CommandDigest, CanonicalOutput)>,
    applications: usize,
}

impl MemoryHarness {
    fn execute(
        &mut self,
        frame: &CanonicalCommandFrame,
    ) -> Result<(CanonicalOutput, bool), ZapError> {
        let command_id = frame.header().command_id().clone();
        if let Some((digest, output)) = self.seen.get(&command_id) {
            if digest == &frame.digest() {
                return Ok((output.clone(), true));
            }
            return Err(ZapError::from_static(
                ErrorCode::IdempotencyConflict,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
                "command ID was reused with different canonical bytes",
                FixSurface::Command,
                ErrorDetail::ConflictingIds {
                    command_id,
                    event_id: frame.header().event_id().clone(),
                },
            ));
        }
        let cell = self
            .cells
            .cell(frame.header().kind())
            .ok_or_else(ZapError::unsupported_operation)?;
        let actor = ActorRef {
            principal_id: PrincipalId::parse("principal-fixture")?,
            operation: OperationRef::Command(command_id.clone()),
            role: PrincipalRole::Worker,
        };
        let header = ValidatedHeader::new(
            frame.header().clone(),
            frame.digest(),
            AdmittedAuthority::agent_data(actor),
            None,
            None,
            None,
            ValidatedCommandPreflight::empty(Arc::new(())),
        );
        let mut changes = ChangeSet::new();
        let decoded = cell.decode_payload(frame.payload())?;
        let output = cell.validate_apply_decoded(
            &self.state,
            &header,
            frame.reason(),
            decoded,
            &mut changes,
        )?;
        self.seen
            .insert(command_id, (frame.digest(), output.clone()));
        self.applications += 1;
        Ok((output, false))
    }
}

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-fixture")?,
        campaign_id: CampaignId::parse("campaign-fixture")?,
        base_id: BaseId::parse("base-fixture")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn frame(value: u64) -> Result<CanonicalCommandFrame, ZapError> {
    let kind = EventKind::parse(EchoPayload::KIND)?;
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: StoreId::parse("store-fixture")?,
        campaign_id: CampaignId::parse("campaign-fixture")?,
        base_id: BaseId::parse("base-fixture")?,
        command_id: CommandId::parse("command-fixture")?,
        event_id: EventId::parse("event-fixture")?,
        expected_revision: Revision::GENESIS,
        kind,
        causes: Vec::new(),
        basis: zap_wire::BasisBinding::NotApplicable,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("fixture transition")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &EchoPayload { value })?;
    CanonicalCommandFrame::new(header, reason, payload)
}

#[test]
fn typed_cell_registry_and_exact_retry_are_deterministic() -> Result<(), ZapError> {
    let cells = CellSet::single(EchoCell)?;
    let routes = RouteRegistry::single(
        EventKind::parse(EchoPayload::KIND)?,
        RouteClass::DataProposal,
    );
    routes.validate_cells(&cells)?;
    let duplicate = CellSet::compose([cells.clone(), cells.clone()]).map_err(|error| error.code);
    assert!(matches!(duplicate, Err(ErrorCode::DuplicateIdentity)));

    let mut harness = MemoryHarness {
        cells,
        state: EmptyState {
            identity: identity()?,
        },
        seen: BTreeMap::new(),
        applications: 0,
    };
    let first = frame(7)?;
    let (first_output, first_retry) = harness.execute(&first)?;
    let (retry_output, exact_retry) = harness.execute(&first)?;
    assert!(!first_retry);
    assert!(exact_retry);
    assert_eq!(first_output, retry_output);
    assert_eq!(harness.applications, 1);
    assert_eq!(
        harness.execute(&frame(8)?).map_err(|error| error.code),
        Err(ErrorCode::IdempotencyConflict)
    );
    assert_eq!(harness.applications, 1);
    Ok(())
}
