use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::legacy_projection::{
    LegacyMandateRecord, LegacyNodeMetadataRecord, LegacyProjectionBundle,
    LegacyTaskConstraintRecord,
};
use zap_domain::seams::{
    DeliveryRoute, MaturityStage, ObligationDisposition, ObligationOwner, ObligationStatus,
    OwnershipRole, TaskContract, VerificationMethod, WorkState, WorkType,
};
use zap_legacy::{
    ImportIdMap, ImportMapBuilder, LegacyId, LegacyKind, LegacyValue, SubjectTarget, packed, unpack,
};
use zap_wire::{
    BoundedText, ContractDigest, Digest32, ObligationId, OutcomeId, PayloadDigest, Revision,
    SubjectRef, WorkId, ZapError,
};

mod support;
use support::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

pub(super) struct TranslatedLegacy {
    pub campaign_id: zap_wire::CampaignId,
    pub base_id: zap_wire::BaseId,
    pub bundle: LegacyProjectionBundle,
    pub mappings: Vec<ImportIdMap>,
}

pub(super) fn translate_base(base_raw: &[u8]) -> Result<TranslatedLegacy, ZapError> {
    let body = base_raw.strip_suffix(b"\n").ok_or_else(import_error)?;
    let base = unpack(body).map_err(|_| import_error())?;
    if text(field(&base, "schema")?)? != "zap/1" {
        return Err(import_error());
    }
    let plan = field(&base, "plan")?;
    let plan_id = text(field(plan, "plan_id")?)?;
    let campaign_id = zap_wire::CampaignId::parse(plan_id)?;
    let base_id = zap_wire::BaseId::parse(&format!(
        "legacy-base:{}",
        Digest32::hash(base_raw).to_hex()
    ))?;
    let mut mapping = ImportMapBuilder::new();
    let task_sources = task_sources(&base, &mut mapping)
        .map_err(|_| translation_stage("legacy task-source capture validation failed"))?;
    let nodes = array(field(plan, "node")?)?;
    let mandates = array(field(plan, "mandate")?)?;
    let minimum_order = nodes
        .iter()
        .map(|node| field(node, "order").and_then(exact_i64))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .unwrap_or(0);
    let order_offset = minimum_order.checked_neg().unwrap_or(0).max(0);
    let outcome_id = OutcomeId::parse(&format!("legacy-outcome:{plan_id}"))?;
    let mut work = Vec::with_capacity(nodes.len());
    let mut node_metadata = Vec::with_capacity(nodes.len());
    let mut obligations = Vec::new();
    let mut obligations_by_work = BTreeMap::<WorkId, Vec<ObligationId>>::new();

    let node_result = (|| -> Result<(), ZapError> {
        for node in nodes {
            let legacy_id = text(field(node, "id")?)?;
            let work_id = mapped_work(&mut mapping, LegacyKind::Node, legacy_id)?;
            let parent = text(field(node, "parent")?)?;
            let parent_id = (!parent.is_empty())
                .then(|| WorkId::parse(parent))
                .transpose()?;
            let depends_on = ids::<WorkId>(field(node, "depends_on")?)?;
            let acceptance = bounded_strings::<4096>(field(node, "acceptance")?)?;
            let legacy_kind = text(field(node, "kind")?)?;
            let legacy_state = text(field(node, "state")?)?;
            let mandate_ids = ids::<ObligationId>(field(node, "mandates")?)?;
            let raw = packed(node).map_err(|_| import_error())?;
            work.push(WorkRecord {
                work_id: work_id.clone(),
                parent_id,
                title: BoundedText::parse(text(field(node, "title")?)?)?,
                kind: work_kind(legacy_kind)?,
                work_type: WorkType::Change,
                state: WorkState::Planned,
                order: normalized_order(field(node, "order")?, order_offset)?,
                depends_on,
                acceptance: acceptance.clone(),
                required_stage: MaturityStage::Prototype,
                validation_generation: 1,
                active_job: None,
                revision: Revision::new(1),
            });
            let mut owned = mandate_ids.clone();
            for (index, statement) in acceptance.iter().enumerate() {
                let obligation_id = mapped_obligation(
                    &mut mapping,
                    LegacyKind::Other,
                    &format!("legacy:acceptance:{legacy_id}:{index:04}"),
                )?;
                obligations.push(ObligationRecord {
                    obligation_id: obligation_id.clone(),
                    created_for_outcome: outcome_id.clone(),
                    current_outcomes: vec![outcome_id.clone()],
                    statement: statement.clone(),
                    essential: true,
                    owners: vec![ObligationOwner {
                        work_id: work_id.clone(),
                        role: OwnershipRole::Implementation,
                    }],
                    status: ObligationStatus::Active,
                    disposition: ObligationDisposition::Retained,
                    successors: Vec::new(),
                    unmet_portion: None,
                    revision: Revision::new(1),
                });
                owned.push(obligation_id);
            }
            owned.sort();
            owned.dedup();
            obligations_by_work.insert(work_id.clone(), owned);
            node_metadata.push(LegacyNodeMetadataRecord {
                work_id,
                legacy_kind: BoundedText::parse(legacy_kind)?,
                legacy_state: BoundedText::parse(legacy_state)?,
                legacy_order: exact_i64(field(node, "order")?)?,
                mandate_ids,
                evidence_ids: bounded_strings::<4096>(field(node, "evidence")?)?,
                contract_anchor: optional_text(node, "contract")?
                    .map(BoundedText::parse)
                    .transpose()?,
                contract_digest: optional_text(node, "contract_sha256")?
                    .map(Digest32::parse)
                    .transpose()?,
                zoom: optional_text(node, "zoom")?
                    .map(BoundedText::parse)
                    .transpose()?,
                unknown_fields: unknown_fields(
                    node,
                    &[
                        "acceptance",
                        "contract",
                        "contract_sha256",
                        "depends_on",
                        "evidence",
                        "id",
                        "kind",
                        "mandates",
                        "order",
                        "parent",
                        "state",
                        "title",
                        "zoom",
                    ],
                )?,
                raw_digest: PayloadDigest::hash(&raw),
                raw,
                revision: Revision::new(1),
            });
        }
        Ok(())
    })();
    node_result.map_err(|_| translation_stage("legacy node projection failed"))?;

    let mandate_records = translate_mandates(mandates, &mut mapping, &outcome_id, &mut obligations)
        .map_err(|_| translation_stage("legacy mandate projection failed"))?;
    let (contracts, task_constraints) = translate_contracts(
        field(&base, "task_contracts")?,
        nodes,
        &task_sources,
        &mut mapping,
        &obligations_by_work,
    )?;
    sort_and_validate(&mut work, &mut node_metadata, &mut obligations)
        .map_err(|_| translation_stage("legacy graph projection validation failed"))?;
    Ok(TranslatedLegacy {
        campaign_id,
        base_id,
        bundle: LegacyProjectionBundle {
            work,
            contracts,
            obligations,
            mandates: mandate_records,
            node_metadata,
            task_constraints,
        },
        mappings: mapping.finish(),
    })
}

#[derive(Clone)]
struct TaskSource {
    path: String,
    raw: Vec<u8>,
    digest: Digest32,
    contract_raw: Vec<u8>,
}

fn task_sources(
    base: &LegacyValue,
    mapping: &mut ImportMapBuilder,
) -> Result<BTreeMap<String, TaskSource>, ZapError> {
    let mut result = BTreeMap::new();
    let sources = field(base, "sources")?;
    for source in array(field(sources, "tasks")?)? {
        let path = text(field(source, "path")?)?.to_owned();
        let raw = base64::engine::general_purpose::STANDARD
            .decode(text(field(source, "raw_base64")?)?)
            .map_err(|_| import_error())?;
        let digest = Digest32::hash(&raw);
        if digest != Digest32::parse(text(field(source, "sha256")?)?)? {
            return Err(import_error());
        }
        mapping.map_subject(
            LegacyId::new(LegacyKind::Source, &path)?,
            SubjectTarget::Source,
        )?;
        let group = unpack(&raw).map_err(|_| import_error())?;
        for task in array(field(&group, "tasks")?)? {
            let id = text(field(task, "id")?)?.to_owned();
            let item = TaskSource {
                path: path.clone(),
                raw: raw.clone(),
                digest,
                contract_raw: packed(task).map_err(|_| import_error())?,
            };
            if result.insert(id, item).is_some() {
                return Err(import_error());
            }
        }
    }
    Ok(result)
}

fn translate_mandates(
    mandates: &[LegacyValue],
    mapping: &mut ImportMapBuilder,
    outcome_id: &OutcomeId,
    obligations: &mut Vec<ObligationRecord>,
) -> Result<Vec<LegacyMandateRecord>, ZapError> {
    let mut records = Vec::with_capacity(mandates.len());
    for mandate in mandates {
        let legacy_id = text(field(mandate, "id")?)?;
        let mandate_id = mapped_obligation(mapping, LegacyKind::Mandate, legacy_id)?;
        let work_ids = ids::<WorkId>(field(mandate, "nodes")?)?;
        let statement = BoundedText::parse(text(field(mandate, "text")?)?)?;
        let disposition = text(field(mandate, "disposition")?)?;
        let raw = packed(mandate).map_err(|_| import_error())?;
        let mut owners = work_ids
            .iter()
            .cloned()
            .map(|work_id| ObligationOwner {
                work_id,
                role: OwnershipRole::Implementation,
            })
            .collect::<Vec<_>>();
        owners.sort();
        owners.dedup();
        obligations.push(ObligationRecord {
            obligation_id: mandate_id.clone(),
            created_for_outcome: outcome_id.clone(),
            current_outcomes: vec![outcome_id.clone()],
            statement: statement.clone(),
            essential: disposition == "owned",
            owners,
            status: ObligationStatus::Active,
            disposition: ObligationDisposition::Retained,
            successors: Vec::new(),
            unmet_portion: None,
            revision: Revision::new(1),
        });
        records.push(LegacyMandateRecord {
            mandate_id,
            statement,
            disposition: BoundedText::parse(disposition)?,
            work_ids,
            source: optional_text(mandate, "source")?
                .map(BoundedText::parse)
                .transpose()?,
            raw_digest: PayloadDigest::hash(&raw),
            raw,
            revision: Revision::new(1),
        });
    }
    records.sort_by(|left, right| left.mandate_id.cmp(&right.mandate_id));
    Ok(records)
}

fn translate_contracts(
    value: &LegacyValue,
    nodes: &[LegacyValue],
    sources: &BTreeMap<String, TaskSource>,
    mapping: &mut ImportMapBuilder,
    obligations_by_work: &BTreeMap<WorkId, Vec<ObligationId>>,
) -> Result<(Vec<TaskContractRecord>, Vec<LegacyTaskConstraintRecord>), ZapError> {
    let mut contracts = Vec::new();
    let mut constraints = Vec::new();
    for (legacy_id, contract) in object(value)? {
        if text(field(contract, "id")?)? != legacy_id {
            return Err(translation_stage(
                "task-contract map key differs from its ID",
            ));
        }
        let work_id = WorkId::parse(legacy_id)?;
        let contract_id = mapped_contract(mapping, LegacyKind::Task, legacy_id)?;
        let source = sources.get(legacy_id).ok_or_else(import_error)?;
        let raw = packed(contract).map_err(|_| import_error())?;
        if raw != source.contract_raw {
            return Err(translation_stage(
                "captured task source differs from base task-contract projection",
            ));
        }
        let digest = Digest32::hash(&raw);
        let _node = nodes
            .iter()
            .find(|node| matches!(field(node, "id"), Ok(value) if text(value) == Ok(legacy_id)))
            .ok_or_else(import_error)?;
        let checks = strings(field(contract, "checks")?)?
            .into_iter()
            .map(|check| {
                Ok(VerificationMethod {
                    argv: vec![BoundedText::parse(&check)?],
                    target: BoundedText::parse("legacy-imported-check")?,
                    toolchain: BoundedText::parse("legacy-zap/1")?,
                    environment: BoundedText::parse("inactive-import")?,
                    subjects: vec![SubjectRef::Work(work_id.clone())],
                    cases: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        let obligation_ids = obligations_by_work
            .get(&work_id)
            .cloned()
            .ok_or_else(import_error)?;
        contracts.push(TaskContractRecord {
            contract_id: contract_id.clone(),
            work_id: work_id.clone(),
            version: Revision::new(1),
            contract_digest: ContractDigest::from_digest(digest),
            active: false,
            contract: TaskContract {
                contract_id: contract_id.clone(),
                work_id,
                title: BoundedText::parse(text(field(contract, "title")?)?)?,
                goal: BoundedText::parse(text(field(contract, "goal")?)?)?,
                read_subjects: Vec::new(),
                write_subjects: Vec::new(),
                resources: Vec::new(),
                steps: bounded_strings::<4096>(field(contract, "steps")?)?,
                positive_cases: bounded_strings::<4096>(field(contract, "positive_cases")?)?,
                negative_cases: bounded_strings::<4096>(field(contract, "negative_cases")?)?,
                checks,
                acceptance: bounded_strings::<4096>(field(contract, "acceptance")?)?,
                safe_stop: BoundedText::parse(text(field(contract, "safe_stop")?)?)?,
                integration_owner: WorkId::parse(legacy_id)?,
                delivery_route: DeliveryRoute::Direct,
                required_stage: MaturityStage::Prototype,
                source_handles: Vec::new(),
                obligation_ids,
            },
        });
        constraints.push(LegacyTaskConstraintRecord {
            contract_id,
            source_path: BoundedText::parse(&source.path)?,
            source_raw: source.raw.clone(),
            source_digest: source.digest,
            contract_raw: raw.clone(),
            contract_digest: PayloadDigest::hash(&raw),
            read_paths: bounded_strings::<4096>(field(contract, "read_paths")?)?,
            write_paths: bounded_strings::<4096>(field(contract, "write_paths")?)?,
            commit_subject: BoundedText::parse(text(field(contract, "commit_subject")?)?)?,
            notes: bounded_strings::<4096>(field(contract, "notes")?)?,
            unknown_fields: unknown_fields(
                contract,
                &[
                    "acceptance",
                    "checks",
                    "commit_subject",
                    "goal",
                    "id",
                    "negative_cases",
                    "notes",
                    "positive_cases",
                    "read_paths",
                    "safe_stop",
                    "steps",
                    "title",
                    "write_paths",
                ],
            )?,
            adaptation_required: true,
            revision: Revision::new(1),
        });
    }
    if contracts.len() != sources.len() {
        return Err(translation_stage(
            "task-contract and captured task-source identity sets differ",
        ));
    }
    contracts.sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
    constraints.sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
    Ok((contracts, constraints))
}

fn sort_and_validate(
    work: &mut [WorkRecord],
    metadata: &mut [LegacyNodeMetadataRecord],
    obligations: &mut [ObligationRecord],
) -> Result<(), ZapError> {
    work.sort_by(|left, right| left.work_id.cmp(&right.work_id));
    metadata.sort_by(|left, right| left.work_id.cmp(&right.work_id));
    obligations.sort_by(|left, right| left.obligation_id.cmp(&right.obligation_id));
    if work
        .windows(2)
        .any(|pair| pair[0].work_id == pair[1].work_id)
        || metadata
            .windows(2)
            .any(|pair| pair[0].work_id == pair[1].work_id)
        || obligations
            .windows(2)
            .any(|pair| pair[0].obligation_id == pair[1].obligation_id)
    {
        return Err(import_error());
    }
    let ids = work
        .iter()
        .map(|record| record.work_id.clone())
        .collect::<BTreeSet<_>>();
    if work.iter().any(|record| {
        record
            .parent_id
            .as_ref()
            .is_some_and(|id| !ids.contains(id))
            || record.depends_on.iter().any(|id| !ids.contains(id))
    }) {
        return Err(import_error());
    }
    Ok(())
}
