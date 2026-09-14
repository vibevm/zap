#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ProbeArtifactLocator {
    artifact: ArtifactDigest,
    byte_len: u64,
    path: std::path::PathBuf,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ProbeNativeInput {
    schema: String,
    lane: String,
    instruction_boundary: String,
    claim: RuntimeJobClaimRecord,
    primary_source: ProbeArtifactLocator,
    material_artifacts: Vec<ProbeArtifactLocator>,
    workspace_manifest: ProbeArtifactLocator,
    candidate_output: std::path::PathBuf,
    acceptance_contract: Vec<AcceptanceCriterion>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ProbeLaneManifest {
    lane: String,
    job_id: JobId,
    attempt_id: AttemptId,
    dispatch_id: DispatchId,
    packet_id: PacketId,
    work_id: WorkId,
    contract_id: ContractId,
    claim_digest: PacketResolutionDigest,
    native_input: std::path::PathBuf,
    ready_receipt: std::path::PathBuf,
    start_receipt: std::path::PathBuf,
    candidate_output: std::path::PathBuf,
    final_observation: std::path::PathBuf,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ProbeManifest {
    schema: String,
    identity: StoreIdentity,
    store_path: std::path::PathBuf,
    requested_model: String,
    requested_effort: String,
    dataset_seed: u64,
    dataset_generator: String,
    launch_operations: Vec<String>,
    lanes: Vec<ProbeLaneManifest>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ProbePhase {
    schema: String,
    phase: String,
    store_revision: Revision,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeStartReceipt {
    schema: String,
    lane: String,
    task_id: String,
    configured_model: String,
    configured_effort: String,
    native_handle: String,
    started: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeFinalObservation {
    schema: String,
    lane: String,
    task_id: String,
    state: String,
    capacity: zap_api::NativeSlotCapacityObservation,
}

struct ProbeContext {
    root: std::path::PathBuf,
    lane: String,
    task_id: Option<String>,
    expected_work: String,
    expected_contract: String,
    expected_field: String,
    expected_value: u64,
}

fn lane_manifest(lane: &ProbeContext, claim: &RuntimeJobClaimRecord) -> ProbeLaneManifest {
    ProbeLaneManifest {
        lane: lane.lane.clone(),
        job_id: claim.job_id.clone(),
        attempt_id: claim.attempt_id.clone(),
        dispatch_id: claim.dispatch_id.clone(),
        packet_id: claim.identity.packet_id.clone(),
        work_id: claim.work.work_id.clone(),
        contract_id: claim.work.contract_id.clone(),
        claim_digest: claim.digest,
        native_input: lane.root.join("native-input.json"),
        ready_receipt: lane.root.join("READY-TO-INVOKE.json"),
        start_receipt: lane.root.join("start-receipt.json"),
        candidate_output: lane.root.join("candidate.patch"),
        final_observation: lane.root.join("executor-observation.json"),
    }
}

fn write_probe_manifest(
    first: &ProbeContext,
    store: &RedbStore,
    identity: &StoreIdentity,
    first_claim: &RuntimeJobClaimRecord,
    second_claim: &RuntimeJobClaimRecord,
    second: &ProbeContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = first
        .root
        .parent()
        .ok_or("native probe lane has no common root")?;
    if second.root.parent() != Some(root) {
        return Err("native probe lanes do not share one campaign root".into());
    }
    let manifest = ProbeManifest {
        schema: "zap-r16-native-probe-manifest/1".to_owned(),
        identity: identity.clone(),
        store_path: store.path().to_path_buf(),
        requested_model: "gpt-5.6-sol".to_owned(),
        requested_effort: "medium".to_owned(),
        dataset_seed: super::fixtures::NATIVE_PROBE_DATASET_SEED,
        dataset_generator: "lcg64-96-lines-mod-10000/v1".to_owned(),
        launch_operations: vec![
            "command.pause.native-pickup".to_owned(),
            "command.resume.native-pickup".to_owned(),
            "runtime-dispatch-consume".to_owned(),
        ],
        lanes: vec![
            lane_manifest(first, first_claim),
            lane_manifest(second, second_claim),
        ],
    };
    let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &manifest)?;
    write_exact(&root.join("PROBE-MANIFEST.json"), encoded.as_bytes())
}

fn write_probe_phase(
    lane: &ProbeContext,
    store: &RedbStore,
    phase: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = lane
        .root
        .parent()
        .ok_or("native probe lane has no common root")?;
    let marker = ProbePhase {
        schema: "zap-r16-native-probe-phase/1".to_owned(),
        phase: phase.to_owned(),
        store_revision: store.head()?,
    };
    let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &marker)?;
    write_exact(
        &root.join(format!("phase-{phase}.json")),
        encoded.as_bytes(),
    )
}

fn prepare_native_probe(
    claim: &RuntimeJobClaimRecord,
    artifacts: &ArtifactStore,
    lane: &str,
) -> Result<Option<ProbeContext>, Box<dyn std::error::Error>> {
    let Some(root) = std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR") else {
        return Ok(None);
    };
    if !matches!(lane, "alpha" | "beta") {
        return Err("native probe lane must be alpha or beta".into());
    }
    let root = std::path::PathBuf::from(root).join(lane);
    std::fs::create_dir_all(&root)?;
    let mut material_artifacts = Vec::new();
    let first_source = claim
        .sources
        .first()
        .ok_or("native probe has no delivered source")?;
    let first_source_artifact = artifacts.open_verified(
        first_source.material.artifact,
        first_source.material.byte_len,
    )?;
    let first_source_bytes = std::fs::read(first_source_artifact.path())?;
    for captured in claim
        .sources
        .iter()
        .map(|row| &row.material)
        .chain(claim.rules.iter().map(|row| &row.material))
        .chain(claim.forks.iter().map(|row| &row.material))
    {
        let published = artifacts.open_verified(captured.artifact, captured.byte_len)?;
        material_artifacts.push(ProbeArtifactLocator {
            artifact: captured.artifact,
            byte_len: captured.byte_len,
            path: published.path().to_path_buf(),
        });
    }
    let workspace =
        artifacts.open_verified(claim.workspace.manifest_artifact, claim.workspace.byte_len)?;
    let candidate_output = root.join("candidate.patch");
    let (expected_field, expected_value) = if claim.work.work_id.as_str() == "work.second" {
        let weighted =
            first_source_bytes
                .iter()
                .enumerate()
                .try_fold(0_u64, |sum, (index, byte)| {
                    let position = u64::try_from(index + 1)
                        .map_err(|_| "source is too large for weighted byte sum")?;
                    let product = position
                        .checked_mul(u64::from(*byte))
                        .ok_or("weighted byte product overflow")?;
                    sum.checked_add(product).ok_or("weighted byte sum overflow")
                })?;
        ("POSITION_WEIGHTED_BYTE_SUM", weighted)
    } else {
        let sum = first_source_bytes.iter().try_fold(0_u64, |sum, byte| {
            sum.checked_add(u64::from(*byte)).ok_or("byte sum overflow")
        })?;
        ("BYTE_VALUE_SUM", sum)
    };
    let input = ProbeNativeInput {
        schema: "zap-r16-native-input/2".to_owned(),
        lane: lane.to_owned(),
        instruction_boundary: "##subagent-quiet-clause\nUse only this product-generated input and its immutable locators. Do not load the repository boot lane. Read the delivered artifacts, perform exactly claim.work, obey the captured rules and acceptance contract, write only candidate_output, and infer no hidden requirements.".to_owned(),
        claim: claim.clone(),
        primary_source: ProbeArtifactLocator {
            artifact: first_source.material.artifact,
            byte_len: first_source.material.byte_len,
            path: first_source_artifact.path().to_path_buf(),
        },
        material_artifacts,
        workspace_manifest: ProbeArtifactLocator {
            artifact: claim.workspace.manifest_artifact,
            byte_len: claim.workspace.byte_len,
            path: workspace.path().to_path_buf(),
        },
        candidate_output,
        acceptance_contract: claim.work.acceptance.clone(),
    };
    let packet = CanonicalOutput::encode_json(CodecEpoch::CURRENT, claim)?;
    let native = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &input)?;
    write_exact(&root.join("packet.json"), packet.as_bytes())?;
    write_exact(&root.join("native-input.json"), native.as_bytes())?;
    let ready = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &(
            "zap-r16-native-probe/1",
            lane,
            &claim.job_id,
            &claim.dispatch_id,
            &claim.resolved_profile.desired.provider,
            &claim.resolved_profile.desired.model,
            &claim.resolved_profile.desired.effort,
        ),
    )?;
    write_exact(&root.join("READY-TO-INVOKE.json"), ready.as_bytes())?;
    Ok(Some(ProbeContext {
        root,
        lane: lane.to_owned(),
        task_id: None,
        expected_work: claim.work.work_id.as_str().to_owned(),
        expected_contract: claim.work.contract_id.as_str().to_owned(),
        expected_field: expected_field.to_owned(),
        expected_value,
    }))
}

fn wait_for_start_receipt(probe: &mut ProbeContext) -> Result<String, Box<dyn std::error::Error>> {
    let path = probe.root.join("start-receipt.json");
    wait_for(&path)?;
    let receipt: ProbeStartReceipt = serde_json::from_slice(&std::fs::read(path)?)?;
    if receipt.schema != "zap-r16-native-start/2"
        || receipt.lane != probe.lane
        || receipt.task_id.is_empty()
        || receipt.configured_model != "gpt-5.6-sol"
        || receipt.configured_effort != "medium"
        || receipt.native_handle.is_empty()
        || !receipt.started
    {
        return Err("native start receipt does not match the requested probe".into());
    }
    probe.task_id = Some(receipt.task_id);
    Ok(receipt.native_handle)
}

fn deterministic_fixture_candidate(
    claim: &RuntimeJobClaimRecord,
    artifacts: &ArtifactStore,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let source = claim
        .sources
        .first()
        .ok_or("deterministic fixture claim has no source")?;
    let mut artifact =
        artifacts.open_verified(source.material.artifact, source.material.byte_len)?;
    let bytes = artifact.read_verified()?;
    let (field, produced) = if claim.work.work_id.as_str() == "work.second" {
        let mut value = 0_u64;
        let mut position = 1_u64;
        for byte in &bytes {
            value = value
                .checked_add(
                    position
                        .checked_mul(u64::from(*byte))
                        .ok_or("producer product overflow")?,
                )
                .ok_or("producer weighted sum overflow")?;
            position = position
                .checked_add(1)
                .ok_or("producer position overflow")?;
        }
        ("POSITION_WEIGHTED_BYTE_SUM", value)
    } else {
        (
            "BYTE_VALUE_SUM",
            bytes.iter().map(|byte| u64::from(*byte)).sum(),
        )
    };
    let candidate = format!(
        "{} {} {field}={produced}",
        claim.work.work_id, claim.work.contract_id
    )
    .into_bytes();
    let expected = if claim.work.work_id.as_str() == "work.second" {
        bytes
            .iter()
            .enumerate()
            .try_fold(0_u64, |sum, (index, byte)| {
                let position =
                    u64::try_from(index + 1).map_err(|_| "oracle source is too large")?;
                let product = position
                    .checked_mul(u64::from(*byte))
                    .ok_or("oracle product overflow")?;
                sum.checked_add(product).ok_or("oracle sum overflow")
            })?
    } else {
        bytes.iter().try_fold(0_u64, |sum, byte| {
            sum.checked_add(u64::from(*byte))
                .ok_or("oracle sum overflow")
        })?
    };
    validate_candidate_bytes(
        &candidate,
        claim.work.work_id.as_str(),
        claim.work.contract_id.as_str(),
        field,
        expected,
    )?;
    Ok(candidate)
}

fn finish_native_probe(
    probe: Option<&ProbeContext>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let Some(probe) = probe else {
        let bytes = b"work.leaf contract.one BYTE_VALUE_SUM=1024".to_vec();
        validate_candidate_bytes(&bytes, "work.leaf", "contract.one", "BYTE_VALUE_SUM", 1024)?;
        return Ok(bytes);
    };
    let candidate = probe.root.join("candidate.patch");
    let observation = probe.root.join("executor-observation.json");
    wait_for(&candidate)?;
    wait_for(&observation)?;
    let bytes = std::fs::read(candidate)?;
    validate_candidate_bytes(
        &bytes,
        &probe.expected_work,
        &probe.expected_contract,
        &probe.expected_field,
        probe.expected_value,
    )?;
    let observed: ProbeFinalObservation = serde_json::from_slice(&std::fs::read(observation)?)?;
    if observed.schema != "zap-r16-native-final/3"
        || observed.lane != probe.lane
        || probe.task_id.as_deref() != Some(observed.task_id.as_str())
        || observed.state != "completed"
    {
        return Err("native final observation is incomplete or mismatched".into());
    }
    let _capacity_is_observed_not_inferred = observed.capacity;
    Ok(bytes)
}

fn wait_for(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
    while !path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if path.exists() {
        Ok(())
    } else {
        Err(format!("native probe timed out waiting for {}", path.display()).into())
    }
}

fn validate_candidate_bytes(
    bytes: &[u8],
    expected_work: &str,
    expected_contract: &str,
    expected_field: &str,
    expected_value: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::str::from_utf8(bytes)?;
    let tokens = text.split_ascii_whitespace().collect::<Vec<_>>();
    if tokens.len() != 3
        || tokens
            .iter()
            .filter(|token| **token == expected_work)
            .count()
            != 1
        || tokens
            .iter()
            .filter(|token| **token == expected_contract)
            .count()
            != 1
    {
        return Err("candidate must contain exactly one work, contract, and result field".into());
    }
    let prefix = format!("{expected_field}=");
    let values = tokens
        .iter()
        .filter_map(|token| token.strip_prefix(&prefix))
        .collect::<Vec<_>>();
    if values.len() != 1 || values[0].parse::<u64>()? != expected_value {
        return Err("candidate numeric result does not match the delivered source".into());
    }
    Ok(())
}

#[test]
fn candidate_verifier_rejects_numeric_prefix_and_duplicate_field() {
    assert!(
        validate_candidate_bytes(
            b"work.leaf contract.one BYTE_VALUE_SUM=10240",
            "work.leaf",
            "contract.one",
            "BYTE_VALUE_SUM",
            1024,
        )
        .is_err()
    );
    assert!(
        validate_candidate_bytes(
            b"work.leaf contract.one BYTE_VALUE_SUM=1024 BYTE_VALUE_SUM=1024",
            "work.leaf",
            "contract.one",
            "BYTE_VALUE_SUM",
            1024,
        )
        .is_err()
    );
}

fn write_exact(path: &std::path::Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write as _;
            file.write_all(bytes)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(path)? == bytes {
                Ok(())
            } else {
                Err("native probe path already contains different bytes".into())
            }
        }
        Err(error) => Err(error.into()),
    }
}
