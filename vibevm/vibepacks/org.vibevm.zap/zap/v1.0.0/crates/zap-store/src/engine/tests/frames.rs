fn service_frame(value: u64) -> Result<CanonicalCommandFrame, zap_wire::ZapError> {
    service_frame_named(
        value,
        "work-1",
        "command-service-1",
        "event-service-1",
        Revision::GENESIS,
    )
}

fn service_frame_with_artifact(
    value: u64,
    artifact: ArtifactDigest,
) -> Result<CanonicalCommandFrame, zap_wire::ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: StoreId::parse("store-1")?,
        campaign_id: CampaignId::parse("campaign-1")?,
        base_id: BaseId::parse("base-1")?,
        command_id: CommandId::parse("command-service-1")?,
        event_id: EventId::parse("event-service-1")?,
        expected_revision: Revision::GENESIS,
        kind: EventKind::parse(PutFixture::KIND)?,
        causes: Vec::new(),
        basis: BasisBinding::NotApplicable,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("owner fixture insert")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let payload = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &PutFixture {
            work_id: WorkId::parse("work-1")?,
            value,
            artifact: Some(artifact),
        },
    )?;
    CanonicalCommandFrame::new(header, reason, payload)
}

fn service_frame_named(
    value: u64,
    work_id: &str,
    command_id: &str,
    event_id: &str,
    expected_revision: Revision,
) -> Result<CanonicalCommandFrame, zap_wire::ZapError> {
    service_frame_bound(
        value,
        work_id,
        command_id,
        event_id,
        expected_revision,
        ProtocolEpoch::new(1)?,
        StoreId::parse("store-1")?,
        CampaignId::parse("campaign-1")?,
        BaseId::parse("base-1")?,
        BasisBinding::NotApplicable,
    )
}

#[allow(clippy::too_many_arguments)]
fn service_frame_bound(
    value: u64,
    work_id: &str,
    command_id: &str,
    event_id: &str,
    expected_revision: Revision,
    protocol: ProtocolEpoch,
    store_id: StoreId,
    campaign_id: CampaignId,
    base_id: BaseId,
    basis: BasisBinding,
) -> Result<CanonicalCommandFrame, zap_wire::ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol,
        store_id,
        campaign_id,
        base_id,
        command_id: CommandId::parse(command_id)?,
        event_id: EventId::parse(event_id)?,
        expected_revision,
        kind: EventKind::parse(PutFixture::KIND)?,
        causes: Vec::new(),
        basis,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("owner fixture insert")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let payload = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &PutFixture {
            work_id: WorkId::parse(work_id)?,
            value,
            artifact: None,
        },
    )?;
    CanonicalCommandFrame::new(header, reason, payload)
}

fn replace_frame(
    work_id: &str,
    expected: Revision,
    value: u64,
    store_revision: Revision,
) -> Result<CanonicalCommandFrame, zap_wire::ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: StoreId::parse("store-1")?,
        campaign_id: CampaignId::parse("campaign-1")?,
        base_id: BaseId::parse("base-1")?,
        command_id: CommandId::parse("command-service-replace")?,
        event_id: EventId::parse("event-service-replace")?,
        expected_revision: store_revision,
        kind: EventKind::parse(ReplaceFixture::KIND)?,
        causes: Vec::new(),
        basis: BasisBinding::NotApplicable,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("owner fixture replace")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let payload = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &ReplaceFixture {
            work_id: WorkId::parse(work_id)?,
            expected,
            value,
        },
    )?;
    CanonicalCommandFrame::new(header, reason, payload)
}

fn remove_frame(
    work_id: &str,
    expected: Revision,
    store_revision: Revision,
    command_id: &str,
    event_id: &str,
) -> Result<CanonicalCommandFrame, zap_wire::ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: StoreId::parse("store-1")?,
        campaign_id: CampaignId::parse("campaign-1")?,
        base_id: BaseId::parse("base-1")?,
        command_id: CommandId::parse(command_id)?,
        event_id: EventId::parse(event_id)?,
        expected_revision: store_revision,
        kind: EventKind::parse(RemoveFixture::KIND)?,
        causes: Vec::new(),
        basis: BasisBinding::NotApplicable,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("owner fixture remove")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let payload = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &RemoveFixture {
            work_id: WorkId::parse(work_id)?,
            expected,
        },
    )?;
    CanonicalCommandFrame::new(header, reason, payload)
}
