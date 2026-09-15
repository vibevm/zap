fn task_contract(
    work_id: &WorkId,
    obligation_id: &ObligationId,
    source: &zap_domain::knowledge::SourceRecord,
) -> Result<TaskContract, ZapError> {
    let native_probe = r16_two_job_fixture_enabled();
    Ok(TaskContract {
        contract_id: ContractId::parse("contract.one")?,
        work_id: work_id.clone(),
        title: text("Implement executable lowering")?,
        goal: text("Read the delivered source and report the sum of all byte values")?,
        read_subjects: vec![SubjectRef::Source(source.source_id.clone())],
        write_subjects: vec![SubjectRef::Work(work_id.clone())],
        resources: vec![ResourceId::parse("resource.workspace")?],
        steps: vec![
            text("Read every byte from the immutable first source artifact")?,
            text("Sum the unsigned byte values as a decimal integer")?,
            text("Write exactly: work.leaf contract.one BYTE_VALUE_SUM=<decimal result>")?,
            text(
                "Do not run the controller verification method; it validates candidate_output after delivery",
            )?,
        ],
        positive_cases: vec![text("Computed byte sum matches the delivered bytes")?],
        negative_cases: vec![text("A copied length or digest value is refused")?],
        checks: vec![if native_probe {
            VerificationMethod {
                argv: vec![
                    text("r16-fixture-driver")?,
                    text("byte-value-sum")?,
                    text("source.one")?,
                    text("BYTE_VALUE_SUM")?,
                ],
                target: text("candidate_output")?,
                toolchain: text("rust-test-driver")?,
                environment: text(
                    "fixture-controller-after-native-delivery; worker-does-not-invoke",
                )?,
                subjects: vec![
                    SubjectRef::Work(work_id.clone()),
                    SubjectRef::Source(source.source_id.clone()),
                ],
                cases: vec![text("R16-NATIVE-BYTE-SUM")?],
            }
        } else {
            VerificationMethod {
                argv: vec![text("cargo")?, text("test")?],
                target: text("lowering_service")?,
                toolchain: text("rust")?,
                environment: text("fixture")?,
                subjects: vec![SubjectRef::Work(work_id.clone())],
                cases: vec![text("LOWERING-COVERAGE-CHECK")?],
            }
        }],
        acceptance: vec![text(
            "Candidate contains work.leaf, contract.one, and BYTE_VALUE_SUM with the decimal sum of every delivered source byte",
        )?],
        safe_stop: text("Persist the candidate and stop")?,
        integration_owner: work_id.clone(),
        delivery_route: DeliveryRoute::Direct,
        required_stage: MaturityStage::Functional,
        source_handles: vec![source.source_id.clone()],
        obligation_ids: vec![obligation_id.clone()],
    })
}

fn task_contract_two(
    work_id: &WorkId,
    obligation_id: &ObligationId,
    source: &zap_domain::knowledge::SourceRecord,
) -> Result<TaskContract, ZapError> {
    Ok(TaskContract {
        contract_id: ContractId::parse("contract.two")?,
        work_id: work_id.clone(),
        title: text("Derive the second deterministic result")?,
        goal: text("Read the delivered source and report its position-weighted byte sum")?,
        read_subjects: vec![SubjectRef::Source(source.source_id.clone())],
        write_subjects: vec![SubjectRef::Work(work_id.clone())],
        resources: vec![ResourceId::parse("resource.second-workspace")?],
        steps: vec![
            text("Read the immutable source artifact")?,
            text("Multiply each byte by its one-based position and sum the products")?,
            text(
                "Write exactly: work.second contract.two POSITION_WEIGHTED_BYTE_SUM=<decimal result>",
            )?,
            text(
                "Do not run the controller verification method; it validates candidate_output after delivery",
            )?,
        ],
        positive_cases: vec![text("Derived weighted sum matches the delivered bytes")?],
        negative_cases: vec![text(
            "A copied expected token without derivation is refused",
        )?],
        checks: vec![VerificationMethod {
            argv: vec![
                text("r16-fixture-driver")?,
                text("position-weighted-byte-sum")?,
                text("source.one")?,
                text("POSITION_WEIGHTED_BYTE_SUM")?,
            ],
            target: text("candidate_output")?,
            toolchain: text("rust-test-driver")?,
            environment: text("fixture-controller-after-native-delivery; worker-does-not-invoke")?,
            subjects: vec![
                SubjectRef::Work(work_id.clone()),
                SubjectRef::Source(source.source_id.clone()),
            ],
            cases: vec![text("R16-NATIVE-WEIGHTED-BYTE-SUM")?],
        }],
        acceptance: vec![text(
            "Candidate contains work.second, contract.two, and POSITION_WEIGHTED_BYTE_SUM with the decimal result derived from every delivered source byte",
        )?],
        safe_stop: text("Persist the derived result and stop")?,
        integration_owner: work_id.clone(),
        delivery_route: DeliveryRoute::Direct,
        required_stage: MaturityStage::Functional,
        source_handles: vec![source.source_id.clone()],
        obligation_ids: vec![obligation_id.clone()],
    })
}

fn verification(
    work_id: &WorkId,
    source: &zap_domain::knowledge::SourceRecord,
) -> Result<VerificationPlan, ZapError> {
    let native_probe = r16_two_job_fixture_enabled();
    Ok(VerificationPlan {
        verification_id: VerificationId::parse("verification.one")?,
        program: text(if native_probe {
            "r16-fixture-driver"
        } else {
            "cargo"
        })?,
        arguments: if native_probe {
            vec![
                text("byte-value-sum")?,
                text("source.one")?,
                text("BYTE_VALUE_SUM")?,
            ]
        } else {
            vec![text("test")?]
        },
        working_directory: ResourceId::parse("resource.workspace")?,
        target: text(if native_probe {
            "candidate_output"
        } else {
            "lowering_service"
        })?,
        toolchain: text(if native_probe {
            "rust-test-driver"
        } else {
            "rust"
        })?,
        environment: text(if native_probe {
            "fixture-controller-after-native-delivery; worker-does-not-invoke"
        } else {
            "fixture"
        })?,
        subjects: if native_probe {
            vec![
                SubjectRef::Work(work_id.clone()),
                SubjectRef::Source(source.source_id.clone()),
            ]
        } else {
            vec![SubjectRef::Work(work_id.clone())]
        },
        cases: vec![requirement()?],
        sources: vec![SourceFingerprint {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
    })
}

fn verification_two(
    work_id: &WorkId,
    source: &zap_domain::knowledge::SourceRecord,
) -> Result<VerificationPlan, ZapError> {
    Ok(VerificationPlan {
        verification_id: VerificationId::parse("verification.two")?,
        program: text("r16-fixture-driver")?,
        arguments: vec![
            text("position-weighted-byte-sum")?,
            text("source.one")?,
            text("POSITION_WEIGHTED_BYTE_SUM")?,
        ],
        working_directory: ResourceId::parse("resource.second-workspace")?,
        target: text("candidate_output")?,
        toolchain: text("rust-test-driver")?,
        environment: text("fixture-controller-after-native-delivery; worker-does-not-invoke")?,
        subjects: vec![
            SubjectRef::Work(work_id.clone()),
            SubjectRef::Source(source.source_id.clone()),
        ],
        cases: vec![requirement()?],
        sources: vec![SourceFingerprint {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
    })
}

fn contract_digest(contract: &TaskContract) -> Result<ContractDigest, ZapError> {
    let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, contract)?;
    Ok(ContractDigest::hash(encoded.as_bytes()))
}

fn requirement() -> Result<RequirementRef, ZapError> {
    RequirementRef::parse(
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK",
    )
}

pub fn text<const N: usize>(value: &str) -> Result<BoundedText<N>, ZapError> {
    BoundedText::parse(value)
}
