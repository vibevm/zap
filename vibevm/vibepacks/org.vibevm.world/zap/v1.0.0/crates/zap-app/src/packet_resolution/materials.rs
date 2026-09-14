use std::ops::Bound;

use zap_core::{
    CapturedPacketMaterial, KeyRange, PacketMaterialProvider, PacketMaterialRequest,
    PacketMaterialSubject, RecordCompleteness, ResolvedPacketFork, ResolvedPacketRule,
    ResolvedPacketSource, RuntimeJobClaimRecord, StateReader, StateReaderExt, StoredRecord,
};
use zap_domain::lowering::CurrentWorkerPacket;
use zap_runtime::CapabilityObservationRecord;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, PayloadDigest, ZapError};

use super::{PACKET_REQ, packet_conflict, packet_error, packet_unavailable};

pub(super) struct MaterialRequests {
    sources: Vec<(
        PacketMaterialRequest,
        zap_wire::SourceId,
        zap_wire::SourceDigest,
    )>,
    rules: Vec<(
        PacketMaterialRequest,
        zap_wire::RequirementRef,
        zap_wire::SourceId,
        zap_wire::SourceDigest,
    )>,
    forks: Vec<(PacketMaterialRequest, zap_wire::ForkId, PayloadDigest)>,
}

pub(super) type CapturedMaterials = (
    Vec<ResolvedPacketSource>,
    Vec<ResolvedPacketRule>,
    Vec<ResolvedPacketFork>,
);

pub(super) fn material_requests(
    identity: &zap_core::StoreIdentity,
    revision: zap_wire::Revision,
    domain: &CurrentWorkerPacket,
) -> MaterialRequests {
    let base = |subject| PacketMaterialRequest {
        store_id: identity.store_id.clone(),
        base_id: identity.base_id.clone(),
        revision,
        subject,
    };
    MaterialRequests {
        sources: domain
            .packet
            .source_captures
            .iter()
            .map(|source| {
                (
                    base(PacketMaterialSubject::Source {
                        source_id: source.source_id.clone(),
                        source_digest: source.digest,
                    }),
                    source.source_id.clone(),
                    source.digest,
                )
            })
            .collect(),
        rules: domain
            .packet
            .rules
            .iter()
            .map(|rule| {
                (
                    base(PacketMaterialSubject::Rule {
                        requirement: rule.requirement.clone(),
                        source_id: rule.source_id.clone(),
                        source_digest: rule.source_digest,
                    }),
                    rule.requirement.clone(),
                    rule.source_id.clone(),
                    rule.source_digest,
                )
            })
            .collect(),
        forks: domain
            .packet
            .forks
            .iter()
            .map(|fork| {
                (
                    base(PacketMaterialSubject::Fork {
                        fork_id: fork.fork_id.clone(),
                        semantic_digest: fork.semantic_digest,
                    }),
                    fork.fork_id.clone(),
                    fork.semantic_digest,
                )
            })
            .collect(),
    }
}

pub(super) fn capture_materials(
    provider: &dyn PacketMaterialProvider,
    requests: &MaterialRequests,
) -> Result<CapturedMaterials, ZapError> {
    let capture = |request: &PacketMaterialRequest| -> Result<CapturedPacketMaterial, ZapError> {
        let material = provider.capture_live(request)?;
        provider.verify_captured(request, &material)?;
        Ok(material)
    };
    Ok((
        requests
            .sources
            .iter()
            .map(|(request, source_id, source_digest)| {
                Ok(ResolvedPacketSource {
                    source_id: source_id.clone(),
                    source_digest: *source_digest,
                    material: capture(request)?,
                })
            })
            .collect::<Result<_, ZapError>>()?,
        requests
            .rules
            .iter()
            .map(|(request, requirement, source_id, source_digest)| {
                Ok(ResolvedPacketRule {
                    requirement: requirement.clone(),
                    source_id: source_id.clone(),
                    source_digest: *source_digest,
                    material: capture(request)?,
                })
            })
            .collect::<Result<_, ZapError>>()?,
        requests
            .forks
            .iter()
            .map(|(request, fork_id, semantic_digest)| {
                Ok(ResolvedPacketFork {
                    fork_id: fork_id.clone(),
                    semantic_digest: *semantic_digest,
                    material: capture(request)?,
                })
            })
            .collect::<Result<_, ZapError>>()?,
    ))
}

pub(super) fn verify_captured_materials(
    provider: &dyn PacketMaterialProvider,
    requests: &MaterialRequests,
    captured: &RuntimeJobClaimRecord,
) -> Result<(), ZapError> {
    validate_captured_materials(requests, captured)?;
    for ((request, _, _), row) in requests.sources.iter().zip(&captured.sources) {
        provider.verify_captured(request, &row.material)?;
    }
    for ((request, _, _, _), row) in requests.rules.iter().zip(&captured.rules) {
        provider.verify_captured(request, &row.material)?;
    }
    for ((request, _, _), row) in requests.forks.iter().zip(&captured.forks) {
        provider.verify_captured(request, &row.material)?;
    }
    Ok(())
}

pub(super) fn validate_captured_materials(
    requests: &MaterialRequests,
    captured: &RuntimeJobClaimRecord,
) -> Result<(), ZapError> {
    if requests.sources.len() != captured.sources.len()
        || requests.rules.len() != captured.rules.len()
        || requests.forks.len() != captured.forks.len()
    {
        return Err(packet_conflict("captured packet material closure changed"));
    }
    for ((_, source_id, digest), row) in requests.sources.iter().zip(&captured.sources) {
        if source_id != &row.source_id || digest != &row.source_digest {
            return Err(packet_conflict("captured source identity changed"));
        }
    }
    for ((_, requirement, source_id, digest), row) in requests.rules.iter().zip(&captured.rules) {
        if requirement != &row.requirement
            || source_id != &row.source_id
            || digest != &row.source_digest
        {
            return Err(packet_conflict("captured rule identity changed"));
        }
    }
    for ((_, fork_id, digest), row) in requests.forks.iter().zip(&captured.forks) {
        if fork_id != &row.fork_id || digest != &row.semantic_digest {
            return Err(packet_conflict("captured fork identity changed"));
        }
    }
    Ok(())
}

pub(super) fn ensure_context_capacity(
    state: &dyn StateReader,
    observation_id: &zap_wire::CapabilityObservationId,
    sources: &[ResolvedPacketSource],
    rules: &[ResolvedPacketRule],
    forks: &[ResolvedPacketFork],
) -> Result<(), ZapError> {
    let observation = state
        .get_typed::<CapabilityObservationRecord>(observation_id)?
        .ok_or_else(|| packet_unavailable("capability observation is missing"))?;
    let required = sources
        .iter()
        .map(|row| row.material.token_estimate)
        .chain(rules.iter().map(|row| row.material.token_estimate))
        .chain(forks.iter().map(|row| row.material.token_estimate))
        .try_fold(0_u64, u64::checked_add)
        .ok_or_else(|| packet_error("packet token estimate overflowed"))?;
    if observation
        .capabilities
        .context_limit
        .is_some_and(|limit| required > limit)
    {
        return Err(ZapError::from_static(
            ErrorCode::LimitExceeded,
            PACKET_REQ,
            "required packet material exceeds the observed harness context",
            FixSurface::Configuration,
            ErrorDetail::None,
        ));
    }
    Ok(())
}

pub(super) fn scan_all<R: StoredRecord>(state: &dyn StateReader) -> Result<Vec<R>, ZapError> {
    let limit = zap_core::PageLimit::within(512, 512)?;
    let mut start = Bound::Unbounded;
    let mut rows = Vec::new();
    loop {
        let page = state.scan_typed::<R>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            limit,
        )?;
        match page.completeness {
            RecordCompleteness::Complete => {
                rows.extend(page.items);
                return Ok(rows);
            }
            RecordCompleteness::More => {
                let key = page.items.last().map(StoredRecord::key).ok_or_else(|| {
                    packet_conflict("record scan returned continuation without a boundary")
                })?;
                start = Bound::Excluded(key);
                rows.extend(page.items);
            }
            RecordCompleteness::UnknownBoundary => {
                return Err(packet_unavailable(
                    "packet resolution reached an unknown record boundary",
                ));
            }
        }
    }
}
