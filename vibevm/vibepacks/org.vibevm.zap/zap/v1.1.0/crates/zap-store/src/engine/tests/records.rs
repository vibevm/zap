#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureRecord {
    work_id: WorkId,
    revision: Revision,
    value: u64,
}

impl CanonicalEncode for FixtureRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, zap_wire::ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for FixtureRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, zap_wire::ZapError> {
        payload.decode_json()
    }
}

impl StoredRecord for FixtureRecord {
    type Key = WorkId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.test.fixture";

    fn key(&self) -> Self::Key {
        self.work_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, zap_wire::ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }

    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, zap_wire::ZapError> {
        Ok(vec![RecordIndexRow::new(
            IndexFamily::parse("zap.test.by-value")?,
            &self.value,
            &self.work_id,
        )?])
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionMarkerRecord {
    work_id: WorkId,
    revision: Revision,
}

impl CanonicalEncode for AdmissionMarkerRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, zap_wire::ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for AdmissionMarkerRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, zap_wire::ZapError> {
        payload.decode_json()
    }
}

impl StoredRecord for AdmissionMarkerRecord {
    type Key = WorkId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.test.admission-marker";

    fn key(&self) -> Self::Key {
        self.work_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, zap_wire::ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PutFixture {
    work_id: WorkId,
    value: u64,
    artifact: Option<ArtifactDigest>,
}

impl CanonicalEncode for PutFixture {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, zap_wire::ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for PutFixture {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, zap_wire::ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for PutFixture {
    const KIND: &'static str = "fixture.put";
}

struct FixtureArtifactScope;

impl PayloadArtifacts<PutFixture> for FixtureArtifactScope {
    fn artifacts(&self, payload: &PutFixture) -> Result<Vec<ArtifactDigest>, zap_wire::ZapError> {
        Ok(payload.artifact.into_iter().collect())
    }
}

struct PutFixtureCell;

impl TransitionCell for PutFixtureCell {
    type Payload = PutFixture;
    type Output = PutFixture;

    fn descriptor(&self) -> Result<CellDescriptor, zap_wire::ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(PutFixture::KIND)?,
            route: RouteClass::OwnerControl(ControlClass::CharterActivate),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(FixtureRecord::FAMILY)?],
            affected_indexes: vec![IndexFamily::parse("zap.test.by-value")?],
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, zap_wire::ZapError> {
        if state
            .get_typed::<FixtureRecord>(&command.payload().work_id)?
            .is_some()
        {
            return Err(zap_wire::ZapError::from_static(
                ErrorCode::Conflict,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
                "fixture record already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            ));
        }
        changes.insert(FixtureRecord {
            work_id: command.payload().work_id.clone(),
            revision: Revision::new(1),
            value: command.payload().value,
        })?;
        Ok(command.payload().clone())
    }
}

struct WrongPutFixtureCell;

impl TransitionCell for WrongPutFixtureCell {
    type Payload = PutFixture;
    type Output = PutFixture;

    fn descriptor(&self) -> Result<CellDescriptor, zap_wire::ZapError> {
        PutFixtureCell.descriptor()
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, zap_wire::ZapError> {
        let mut output = PutFixtureCell.apply(state, command, changes)?;
        output.value = output
            .value
            .checked_add(1)
            .ok_or_else(|| test_error("overflow"))?;
        Ok(output)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceFixture {
    work_id: WorkId,
    expected: Revision,
    value: u64,
}

impl CanonicalEncode for ReplaceFixture {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, zap_wire::ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for ReplaceFixture {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, zap_wire::ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for ReplaceFixture {
    const KIND: &'static str = "fixture.replace";
}

struct ReplaceFixtureCell;

impl TransitionCell for ReplaceFixtureCell {
    type Payload = ReplaceFixture;
    type Output = ReplaceFixture;

    fn descriptor(&self) -> Result<CellDescriptor, zap_wire::ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(ReplaceFixture::KIND)?,
            route: RouteClass::OwnerControl(ControlClass::CharterAmend),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(FixtureRecord::FAMILY)?],
            affected_indexes: vec![IndexFamily::parse("zap.test.by-value")?],
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, zap_wire::ZapError> {
        let current = state
            .get_typed::<FixtureRecord>(&command.payload().work_id)?
            .ok_or_else(|| test_error("fixture record is missing"))?;
        if current.revision != command.payload().expected {
            return Err(test_error(
                "fixture version differs from replace expectation",
            ));
        }
        changes.replace(
            current.revision,
            FixtureRecord {
                work_id: current.work_id,
                revision: current.revision.checked_next()?,
                value: command.payload().value,
            },
        )?;
        Ok(command.payload().clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveFixture {
    work_id: WorkId,
    expected: Revision,
}

impl CanonicalEncode for RemoveFixture {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, zap_wire::ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RemoveFixture {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, zap_wire::ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for RemoveFixture {
    const KIND: &'static str = "fixture.remove";
}

struct RemoveFixtureCell;

impl TransitionCell for RemoveFixtureCell {
    type Payload = RemoveFixture;
    type Output = RemoveFixture;

    fn descriptor(&self) -> Result<CellDescriptor, zap_wire::ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(RemoveFixture::KIND)?,
            route: RouteClass::OwnerControl(ControlClass::CharterAmend),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(FixtureRecord::FAMILY)?],
            affected_indexes: vec![IndexFamily::parse("zap.test.by-value")?],
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, zap_wire::ZapError> {
        let current = state
            .get_typed::<FixtureRecord>(&command.payload().work_id)?
            .ok_or_else(|| test_error("fixture record is missing"))?;
        changes.remove::<FixtureRecord>(current.work_id, command.payload().expected)?;
        Ok(command.payload().clone())
    }
}
