use std::collections::{BTreeMap, BTreeSet};

use zap_core::{CandidateResultContract, RuntimeJobClaimRecord};
use zap_domain::lowering::{
    BundleEntryBinding, BundleEntryKind, CharterPermissionBinding, WeakBundleManifest,
};
use zap_domain::owner_control::StopRuleRecord;
use zap_runtime::CapabilityObservationRecord;
use zap_wire::{ArtifactDigest, ContractDigest, ZapError};

use crate::{BundleArtifactCapture, PortableAssignmentBody, PortablePacketBody};

use super::super::config::PortableArchiveLimits;
use super::super::paths::validate_archive_path;
use super::codec::{canonical_bytes, canonical_digest, decode_semantic_entry};
use super::{PortableBundleEntryBody, archive_conflict, archive_limit};

pub(super) fn validate_manifest(
    manifest: WeakBundleManifest,
    limits: &PortableArchiveLimits,
) -> Result<WeakBundleManifest, ZapError> {
    let sealed = manifest.clone().seal()?;
    if sealed != manifest
        || manifest.entries.len() > usize::try_from(limits.maximum_entries).unwrap_or(usize::MAX)
        || manifest
            .entries
            .iter()
            .any(|entry| entry.byte_len == 0 || entry.byte_len > limits.maximum_entry_bytes)
    {
        return Err(archive_conflict(
            "portable bundle manifest is not an exact bounded sealed closure",
        ));
    }
    let mut paths = BTreeSet::new();
    for entry in &manifest.entries {
        validate_entry_path(entry.kind, entry.path.as_str())?;
        if !paths.insert(entry.path.as_str()) {
            return Err(archive_conflict(
                "portable bundle manifest contains a duplicate destination",
            ));
        }
    }
    let expected = expected_entry_bindings(&manifest)?;
    let observed = manifest
        .entries
        .iter()
        .map(|entry| ((entry.kind, entry.path.as_str().to_owned()), entry))
        .collect::<BTreeMap<_, _>>();
    if expected.len() != observed.len()
        || expected.iter().any(|(key, exact)| {
            observed.get(key).is_none_or(|entry| {
                exact.is_some_and(|(artifact, len)| {
                    entry.artifact != artifact || entry.byte_len != len
                })
            })
        })
    {
        return Err(archive_conflict(
            "portable bundle entries do not equal the independently derived R09 closure",
        ));
    }
    Ok(manifest)
}

type ExactEntry = Option<(ArtifactDigest, u64)>;

fn expected_entry_bindings(
    manifest: &WeakBundleManifest,
) -> Result<BTreeMap<(BundleEntryKind, String), ExactEntry>, ZapError> {
    let mut expected = BTreeMap::new();
    for source in &manifest.sources {
        insert_expected(
            &mut expected,
            BundleEntryKind::Source,
            material_path(BundleEntryKind::Source, source.artifact),
            Some((source.artifact, source.byte_len)),
        )?;
    }
    for rule in &manifest.rules {
        insert_expected(
            &mut expected,
            BundleEntryKind::Rule,
            material_path(BundleEntryKind::Rule, rule.artifact),
            Some((rule.artifact, rule.byte_len)),
        )?;
    }
    for fork in &manifest.forks {
        insert_expected(
            &mut expected,
            BundleEntryKind::Fork,
            material_path(BundleEntryKind::Fork, fork.artifact),
            Some((fork.artifact, fork.byte_len)),
        )?;
    }
    for attempt in &manifest.attempts {
        insert_expected(
            &mut expected,
            BundleEntryKind::Workspace,
            material_path(BundleEntryKind::Workspace, attempt.workspace_manifest),
            Some((
                attempt.workspace_manifest,
                manifest_entry_len(
                    manifest,
                    BundleEntryKind::Workspace,
                    attempt.workspace_manifest,
                )?,
            )),
        )?;
    }
    for packet in &manifest.packets {
        insert_expected(
            &mut expected,
            BundleEntryKind::Packet,
            semantic_path(BundleEntryKind::Packet, packet.packet_id.as_str()),
            None,
        )?;
        insert_expected(
            &mut expected,
            BundleEntryKind::Assignment,
            semantic_path(BundleEntryKind::Assignment, packet.work_id.as_str()),
            None,
        )?;
        insert_expected(
            &mut expected,
            BundleEntryKind::ResultSchema,
            semantic_path(BundleEntryKind::ResultSchema, packet.packet_id.as_str()),
            None,
        )?;
    }
    for capability in &manifest.capabilities {
        insert_expected(
            &mut expected,
            BundleEntryKind::Capability,
            semantic_path(BundleEntryKind::Capability, capability.as_str()),
            None,
        )?;
    }
    for permission in &manifest.permissions {
        insert_expected(
            &mut expected,
            BundleEntryKind::Permission,
            semantic_path(BundleEntryKind::Permission, permission.action.as_str()),
            None,
        )?;
    }
    for stop in &manifest.stop_rules {
        insert_expected(
            &mut expected,
            BundleEntryKind::StopRule,
            semantic_path(BundleEntryKind::StopRule, stop.stop_rule_id.as_str()),
            Some((stop.artifact, stop.byte_len)),
        )?;
    }
    Ok(expected)
}

fn insert_expected(
    expected: &mut BTreeMap<(BundleEntryKind, String), ExactEntry>,
    kind: BundleEntryKind,
    path: String,
    exact: ExactEntry,
) -> Result<(), ZapError> {
    let key = (kind, path);
    match expected.get(&key) {
        Some(existing) if existing == &exact => Ok(()),
        Some(_) => Err(archive_conflict(
            "overlapping bundle material has conflicting byte identity",
        )),
        None => {
            expected.insert(key, exact);
            Ok(())
        }
    }
}

fn manifest_entry_len(
    manifest: &WeakBundleManifest,
    kind: BundleEntryKind,
    artifact: ArtifactDigest,
) -> Result<u64, ZapError> {
    manifest
        .entries
        .iter()
        .find(|entry| entry.kind == kind && entry.artifact == artifact)
        .map(|entry| entry.byte_len)
        .ok_or_else(|| archive_conflict("bundle workspace material entry is missing"))
}

fn decode_typed_entry(
    capture: BundleArtifactCapture,
    manifest: &WeakBundleManifest,
) -> Result<PortableBundleEntryBody, ZapError> {
    let id = entry_id(capture.path.as_str())?;
    match capture.kind {
        BundleEntryKind::Packet => {
            let body: PortablePacketBody = capture.body.decode_json()?;
            validate_packet_body(&body, id, manifest)?;
            Ok(PortableBundleEntryBody::Packet(Box::new(body)))
        }
        BundleEntryKind::Assignment => {
            let body: PortableAssignmentBody = capture.body.decode_json()?;
            if body.job.clone().validate()? != body.job {
                return Err(archive_conflict(
                    "assignment runtime job body is not in validated canonical form",
                ));
            }
            validate_assignment_body(&body, id, manifest)?;
            Ok(PortableBundleEntryBody::Assignment(Box::new(body)))
        }
        BundleEntryKind::ResultSchema => {
            let body: CandidateResultContract = capture.body.decode_json()?;
            let packet = manifest
                .packets
                .iter()
                .find(|packet| packet.packet_id.as_str() == id)
                .ok_or_else(|| archive_conflict("result schema packet binding is missing"))?;
            if body.work_id != packet.work_id
                || body.contract_id != packet.contract_id
                || body.contract_digest != packet.contract_digest
                || body.relevant_basis != packet.relevant_basis
            {
                return Err(archive_conflict(
                    "result schema differs from the packet contract binding",
                ));
            }
            Ok(PortableBundleEntryBody::ResultSchema(Box::new(body)))
        }
        BundleEntryKind::Capability => {
            let body: CapabilityObservationRecord = capture.body.decode_json()?;
            if body.observation_id.as_str() != id
                || manifest
                    .capabilities
                    .binary_search(&body.observation_id)
                    .is_err()
                || !manifest.attempts.iter().any(|attempt| {
                    attempt.capability_observation == body.observation_id
                        && body
                            .capabilities
                            .digest()
                            .is_ok_and(|digest| digest == attempt.capability_digest)
                })
            {
                return Err(archive_conflict(
                    "capability body differs from the attempt closure",
                ));
            }
            Ok(PortableBundleEntryBody::Capability(Box::new(body)))
        }
        BundleEntryKind::Permission => {
            let body: CharterPermissionBinding = capture.body.decode_json()?;
            if body.action.as_str() != id
                || manifest.permissions.binary_search(&body).is_err()
                || canonical_digest(&(
                    &body.charter_id,
                    body.charter_revision,
                    body.charter_digest,
                    &body.action,
                    &body.packet_ids,
                ))? != body.digest
            {
                return Err(archive_conflict(
                    "permission body differs from the charter-derived closure",
                ));
            }
            Ok(PortableBundleEntryBody::Permission(Box::new(body)))
        }
        BundleEntryKind::StopRule => {
            let body: StopRuleRecord = capture.body.decode_json()?;
            let binding = manifest
                .stop_rules
                .iter()
                .find(|binding| binding.stop_rule_id.as_str() == id)
                .ok_or_else(|| archive_conflict("stop-rule binding is missing"))?;
            if !body.active
                || body.campaign_id != manifest.campaign_id
                || body.stop_rule_id != binding.stop_rule_id
                || body.revision != binding.revision
                || canonical_digest(&body)? != binding.rule_digest
            {
                return Err(archive_conflict(
                    "stop-rule body differs from the active rule closure",
                ));
            }
            Ok(PortableBundleEntryBody::StopRule(Box::new(body)))
        }
        BundleEntryKind::Source
        | BundleEntryKind::Rule
        | BundleEntryKind::Fork
        | BundleEntryKind::Workspace => Err(archive_conflict(
            "raw material entry was encoded as a semantic entry",
        )),
    }
}

fn validate_packet_body(
    body: &PortablePacketBody,
    id: &str,
    manifest: &WeakBundleManifest,
) -> Result<(), ZapError> {
    let packet = manifest
        .packets
        .iter()
        .find(|packet| packet.packet_id.as_str() == id)
        .ok_or_else(|| archive_conflict("packet manifest binding is missing"))?;
    let attempt = manifest
        .attempts
        .iter()
        .find(|attempt| attempt.packet_id == packet.packet_id)
        .ok_or_else(|| archive_conflict("packet attempt binding is missing"))?;
    let record = &body.packet;
    let claim = &body.claim;
    if record.packet_id != packet.packet_id
        || record.packet_digest != packet.packet_digest
        || record.work_id != packet.work_id
        || record.contract_id != packet.contract_id
        || record.contract_version != packet.contract_version
        || record.contract_digest != packet.contract_digest
        || record.lowering_id != manifest.binding.lowering_id
        || record.lowering_revision != manifest.binding.lowering_revision
        || record.lowering_semantic_digest != manifest.binding.lowering_semantic_digest
        || record.strategy_id != manifest.binding.strategy_id
        || record.strategy_revision != manifest.binding.strategy_revision
        || record.strategy_semantic_digest != manifest.binding.strategy_semantic_digest
        || claim.identity.store_id != manifest.store_id
        || claim.identity.campaign_id != manifest.campaign_id
        || claim.identity.base_id != manifest.base_id
        || claim.identity.packet_id != packet.packet_id
        || claim.identity.packet_digest != packet.packet_digest
        || claim.job_id != attempt.job_id
        || claim.attempt_id != attempt.attempt_id
        || claim.dispatch_id != attempt.dispatch_id
        || claim.effect_id != attempt.effect_id
        || claim.digest != attempt.packet_resolution_digest
        || claim.capability_observation != attempt.capability_observation
        || claim.capability_digest != attempt.capability_digest
        || claim.workspace.manifest_artifact != attempt.workspace_manifest
        || claim.work.work_id != packet.work_id
        || claim.work.contract_id != packet.contract_id
        || claim.work.contract_version.get() != packet.contract_version.get()
        || claim.work.contract_digest != packet.contract_digest
        || claim.work.relevant_basis != packet.relevant_basis
        || claim.candidate_result.work_id != packet.work_id
        || claim.candidate_result.contract_id != packet.contract_id
        || claim.candidate_result.contract_digest != packet.contract_digest
        || claim.candidate_result.relevant_basis != packet.relevant_basis
        || !claim_materials_match_manifest(claim, manifest)
    {
        return Err(archive_conflict(
            "packet and claim bodies differ from the exact manifest closure",
        ));
    }
    Ok(())
}

fn claim_materials_match_manifest(
    claim: &RuntimeJobClaimRecord,
    manifest: &WeakBundleManifest,
) -> bool {
    claim.sources.iter().all(|row| {
        manifest.sources.iter().any(|source| {
            source.source_id == row.source_id
                && source.source_digest == row.source_digest
                && source.artifact == row.material.artifact
                && source.byte_len == row.material.byte_len
        })
    }) && claim.rules.iter().all(|row| {
        manifest.rules.iter().any(|rule| {
            rule.requirement == row.requirement
                && rule.source_id == row.source_id
                && rule.source_digest == row.source_digest
                && rule.artifact == row.material.artifact
                && rule.byte_len == row.material.byte_len
        })
    }) && claim.forks.iter().all(|row| {
        manifest.forks.iter().any(|fork| {
            fork.fork_id == row.fork_id
                && fork.semantic_digest == row.semantic_digest
                && fork.artifact == row.material.artifact
                && fork.byte_len == row.material.byte_len
        })
    })
}

fn validate_assignment_body(
    body: &PortableAssignmentBody,
    id: &str,
    manifest: &WeakBundleManifest,
) -> Result<(), ZapError> {
    let packet = manifest
        .packets
        .iter()
        .find(|packet| packet.work_id.as_str() == id)
        .ok_or_else(|| archive_conflict("assignment packet binding is missing"))?;
    let attempt = manifest
        .attempts
        .iter()
        .find(|attempt| attempt.packet_id == packet.packet_id)
        .ok_or_else(|| archive_conflict("assignment attempt binding is missing"))?;
    let contract = &body.contract;
    let job = &body.job;
    let contract_bytes = canonical_bytes(&contract.contract)?;
    if !contract.active
        || contract.contract_id != packet.contract_id
        || contract.work_id != packet.work_id
        || contract.version != packet.contract_version
        || contract.contract_digest != packet.contract_digest
        || ContractDigest::hash(&contract_bytes) != contract.contract_digest
        || job.job_id != attempt.job_id
        || job.attempt_id != attempt.attempt_id
        || job.dispatch_id != attempt.dispatch_id
        || job.effect_id != attempt.effect_id
        || job.packet_id != packet.packet_id
        || job.packet_digest != packet.packet_digest
        || job.packet_resolution_digest != attempt.packet_resolution_digest
        || job.contract_id != packet.contract_id
        || job.contract_version.get() != packet.contract_version.get()
        || job.contract_digest != packet.contract_digest
        || job.relevant_basis != packet.relevant_basis
        || job.workspace_manifest != attempt.workspace_manifest
        || job.candidate_result_contract.work_id != packet.work_id
        || job.candidate_result_contract.contract_id != packet.contract_id
        || job.candidate_result_contract.contract_digest != packet.contract_digest
        || job.candidate_result_contract.relevant_basis != packet.relevant_basis
    {
        return Err(archive_conflict(
            "assignment contract and job bodies differ from the exact manifest closure",
        ));
    }
    Ok(())
}

fn entry_id(path: &str) -> Result<&str, ZapError> {
    path.rsplit_once('/')
        .and_then(|(_, name)| name.strip_suffix(".json"))
        .ok_or_else(|| archive_conflict("bundle semantic entry path is invalid"))
}

pub(super) fn validate_entry_content(
    entry: &BundleEntryBinding,
    content: &[u8],
    manifest: &WeakBundleManifest,
) -> Result<PortableBundleEntryBody, ZapError> {
    if ArtifactDigest::hash(content) != entry.artifact
        || u64::try_from(content.len()).ok() != Some(entry.byte_len)
    {
        return Err(archive_conflict(
            "portable bundle entry content hash or length is invalid",
        ));
    }
    if matches!(
        entry.kind,
        BundleEntryKind::Source
            | BundleEntryKind::Rule
            | BundleEntryKind::Fork
            | BundleEntryKind::Workspace
    ) {
        return Ok(PortableBundleEntryBody::Material(content.to_vec()));
    }
    let capture = decode_semantic_entry(
        content,
        u64::try_from(content.len()).map_err(|_| archive_limit())?,
    )?;
    if capture.kind != entry.kind || capture.path != entry.path {
        return Err(archive_conflict(
            "bundle semantic entry does not match its manifest destination",
        ));
    }
    decode_typed_entry(capture, manifest)
}

pub(super) fn validate_entry_path(kind: BundleEntryKind, path: &str) -> Result<(), ZapError> {
    validate_archive_path(path)?;
    let expected_prefix = format!("{}/", kind_directory(kind));
    if !path.starts_with(&expected_prefix) || !path.ends_with(".json") {
        return Err(archive_conflict(
            "bundle entry path does not match its typed destination",
        ));
    }
    Ok(())
}

fn material_path(kind: BundleEntryKind, artifact: ArtifactDigest) -> String {
    semantic_path(kind, &artifact.to_string())
}

fn semantic_path(kind: BundleEntryKind, id: &str) -> String {
    format!("{}/{id}.json", kind_directory(kind))
}

fn kind_directory(kind: BundleEntryKind) -> &'static str {
    match kind {
        BundleEntryKind::Packet => "packets",
        BundleEntryKind::Assignment => "assignments",
        BundleEntryKind::Source => "sources",
        BundleEntryKind::Rule => "rules",
        BundleEntryKind::Fork => "forks",
        BundleEntryKind::Capability => "capabilities",
        BundleEntryKind::Permission => "permissions",
        BundleEntryKind::StopRule => "stop-rules",
        BundleEntryKind::Workspace => "workspaces",
        BundleEntryKind::ResultSchema => "result-schemas",
    }
}
