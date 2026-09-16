pub fn deterministic_two_job_campaign_completes_through_product_routes()
-> Result<(), Box<dyn std::error::Error>> {
    enable_r16_deterministic_fixture();
    real_lowered_packet_seals_runtime_claim_and_replays_captured_material()
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct NativeProbeJobReconciliation {
    job_id: JobId,
    attempt_id: AttemptId,
    dispatch_id: DispatchId,
    packet_id: PacketId,
    receipt: Option<DispatchReceipt>,
    execution: ExecutionState,
    collection: CollectionState,
    safe: SafeState,
    authorization: Option<PreEffectAuthorizationRecord>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct NativeProbeReconciliation {
    schema: String,
    identity: StoreIdentity,
    observed_revision: Revision,
    jobs: Vec<NativeProbeJobReconciliation>,
}

pub fn inspect_native_campaign_probe_without_replay() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR")
        .ok_or("ZAP_R16_NATIVE_PROBE_DIR must name the preserved probe directory")?;
    let path = std::path::PathBuf::from(root)
        .join("campaign")
        .join("packet-resolution.redb");
    if !path.exists() {
        return Err("preserved native campaign store is missing".into());
    }
    let composition = foundation_composition()?;
    let store = RedbStore::open(path)?.with_records(composition.records, QueryEpoch::new(1)?);
    let snapshot = store.read(ReadAt::Current)?;
    let mut jobs = snapshot
        .scan_typed::<RuntimeJobRecord>(
            KeyRange {
                start: std::ops::Bound::Unbounded,
                end: std::ops::Bound::Unbounded,
            },
            PageLimit::within(16, 4096)?,
        )?
        .items;
    jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
    let jobs = jobs
        .into_iter()
        .map(|job| {
            Ok(NativeProbeJobReconciliation {
                authorization: snapshot
                    .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?,
                job_id: job.job_id,
                attempt_id: job.attempt_id,
                dispatch_id: job.dispatch_id,
                packet_id: job.packet_id,
                receipt: job.receipt,
                execution: job.execution,
                collection: job.collection,
                safe: job.safe,
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    let view = NativeProbeReconciliation {
        schema: "zap-r16-native-reconciliation/1".to_owned(),
        identity: StateReader::identity(&snapshot),
        observed_revision: StateReader::revision(&snapshot),
        jobs,
    };
    let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &view)?;
    println!("{}", std::str::from_utf8(encoded.as_bytes())?);
    Ok(())
}

fn send_machine(
    address: SocketAddr,
    path: &str,
    credential_id: &str,
    secret: &str,
    request: &MachineRequest,
) -> Result<MachineResponse, Box<dyn std::error::Error>> {
    let body = serde_json::to_vec(request)?;
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: {credential_id}\r\nAuthorization: Bearer {secret}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.shutdown(Shutdown::Write)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    if !response.starts_with(b"HTTP/1.1 200") {
        return Err(format!("route failed: {}", String::from_utf8_lossy(&response)).into());
    }
    let offset = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("route response body is missing")?
        + 4;
    Ok(serde_json::from_slice(&response[offset..])?)
}

fn frame<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    payload: &P,
    revision: Revision,
    command_id: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    frame_with_basis(
        identity,
        payload,
        revision,
        BasisBinding::NotApplicable,
        command_id,
    )
}

fn frame_with_basis<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    payload: &P,
    revision: Revision,
    basis: BasisBinding,
    command_id: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse(command_id)?,
            event_id: EventId::parse(&command_id.replace("command", "event"))?,
            expected_revision: revision,
            kind: EventKind::parse(P::KIND)?,
            causes: Vec::new(),
            basis,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("packet resolution service journey")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT",
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn unavailable(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT",
        message,
        FixSurface::SourceCapture,
        ErrorDetail::None,
    )
}
