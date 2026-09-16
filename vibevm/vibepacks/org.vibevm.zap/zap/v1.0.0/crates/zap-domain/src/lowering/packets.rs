specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

use std::collections::BTreeMap;

use zap_core::WorkerRole;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, PacketDigest, ZapError};

use crate::lowering::*;

mod current;
mod derive;

pub use current::*;
pub use derive::derive_packet_role;
pub(crate) use derive::{derive_worker_packet, worker_packet_digest};

const PACKET_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT";
pub fn assemble_packet(input: PacketAssembly) -> Result<AssembledPacket, ZapError> {
    if !sorted_unique_values(&input.read_subjects)
        || !sorted_unique_values(&input.write_subjects)
        || input
            .read_subjects
            .iter()
            .any(|subject| input.write_subjects.binary_search(subject).is_ok())
        || input.result_contract.as_str().trim().is_empty()
        || !sorted_unique_values(&input.basis_roots)
        || input.basis_roots.is_empty()
    {
        return Err(packet_error(
            "packet subjects and result contract must be exact",
        ));
    }
    if input.desired_profile.role == WorkerRole::Senior && !input.write_subjects.is_empty() {
        return Err(ZapError::from_static(
            ErrorCode::Unauthorized,
            PACKET_REQ,
            "Senior packets cannot receive production write subjects",
            FixSurface::Payload,
            ErrorDetail::None,
        ));
    }
    if let Some(resolved) = &input.resolved_profile
        && resolved.desired != input.desired_profile
    {
        return Err(packet_error(
            "resolved profile does not bind the desired packet profile",
        ));
    }
    if let Some(abstraction) = &input.abstraction {
        validate_abstraction(abstraction)?;
    }
    let mut by_digest = BTreeMap::new();
    for fragment in input.fragments {
        if let Some(existing) = by_digest.insert(fragment.digest, fragment.clone())
            && existing != fragment
        {
            return Err(packet_error(
                "one fragment digest names conflicting packet metadata",
            ));
        }
    }
    let mut fragments: Vec<_> = by_digest.into_values().collect();
    fragments.sort_by_key(|row| (!row.required, row.class, row.digest));
    let mut included = Vec::new();
    let mut omissions = Vec::new();
    let mut token_estimate = 0_u64;
    for fragment in fragments {
        if fragment.class == FragmentClass::FullBoot
            && (input.desired_profile.role != WorkerRole::Senior
                || !input.architecture_context_required)
        {
            return Err(packet_error(
                "full boot is allowed only for an explicitly architectural Senior packet",
            ));
        }
        match &fragment.availability {
            FragmentAvailability::Available {
                token_estimate: cost,
                bytes,
            } => {
                if *bytes == 0
                    || *cost == 0
                    || fragment.source_id.is_some() != fragment.source_digest.is_some()
                    || fragment.class == FragmentClass::Source && fragment.source_id.is_none()
                {
                    return Err(packet_error(
                        "packet fragment provenance and measured cost are incomplete",
                    ));
                }
                let next = token_estimate.checked_add(*cost).ok_or_else(|| {
                    packet_error("packet token estimate overflowed its declared counter")
                })?;
                if input.token_budget.is_some_and(|budget| next > budget) {
                    if fragment.required {
                        return Err(ZapError::from_static(
                            ErrorCode::LimitExceeded,
                            PACKET_REQ,
                            "mandatory packet context exceeds the explicitly supplied budget",
                            FixSurface::Configuration,
                            ErrorDetail::None,
                        ));
                    }
                    let retrieval = fragment.retrieval.clone().ok_or_else(|| {
                        packet_error("optional overflow requires an explicit retrieval handle")
                    })?;
                    omissions.push(ContextOmission::Unloaded {
                        digest: fragment.digest,
                        retrieval,
                    });
                } else {
                    token_estimate = next;
                    included.push(fragment);
                }
            }
            FragmentAvailability::Unavailable { reason } if !fragment.required => {
                omissions.push(ContextOmission::Unavailable {
                    digest: fragment.digest,
                    reason: reason.clone(),
                });
            }
            FragmentAvailability::Unknown { question } if !fragment.required => {
                omissions.push(ContextOmission::Unknown {
                    digest: fragment.digest,
                    question: question.clone(),
                });
            }
            FragmentAvailability::Excluded { authority, reason } if !fragment.required => {
                omissions.push(ContextOmission::Excluded {
                    digest: fragment.digest,
                    authority: authority.clone(),
                    reason: reason.clone(),
                });
            }
            FragmentAvailability::Unavailable { .. }
            | FragmentAvailability::Unknown { .. }
            | FragmentAvailability::Excluded { .. } => {
                return Err(ZapError::from_static(
                    ErrorCode::Unavailable,
                    PACKET_REQ,
                    "required packet context is unavailable or semantically unknown",
                    FixSurface::SourceCapture,
                    ErrorDetail::None,
                ));
            }
        }
    }
    if included.is_empty() {
        return Err(packet_error(
            "packet must include concrete protocol and assignment context",
        ));
    }
    let mut packet = AssembledPacket {
        packet_id: input.packet_id,
        parent_packet_id: input.parent_packet_id,
        supersedes: input.supersedes,
        lowering_id: input.lowering_id,
        work_id: input.work_id,
        semantic_digest: input.semantic_digest,
        basis_roots: input.basis_roots,
        desired_profile: input.desired_profile,
        resolved_profile: input.resolved_profile,
        included,
        omissions,
        token_estimate,
        read_subjects: input.read_subjects,
        write_subjects: input.write_subjects,
        forks: input.forks,
        checks: input.checks,
        safe_stop: input.safe_stop,
        abstraction: input.abstraction,
        result_contract: input.result_contract,
        digest: PacketDigest::hash(b"pending"),
    };
    packet.digest = assembled_packet_digest(&packet)?;
    Ok(packet)
}

fn assembled_packet_digest(packet: &AssembledPacket) -> Result<PacketDigest, ZapError> {
    #[derive(serde::Serialize)]
    struct PacketDigestInput<'a> {
        packet_id: &'a zap_wire::PacketId,
        parent_packet_id: &'a Option<zap_wire::PacketId>,
        supersedes: &'a Option<zap_wire::PacketId>,
        lowering_id: &'a zap_wire::LoweringId,
        work_id: &'a zap_wire::WorkId,
        semantic_digest: zap_wire::PayloadDigest,
        basis_roots: &'a [zap_wire::SubjectRef],
        desired_profile: &'a zap_core::DesiredProfile,
        resolved_profile: &'a Option<zap_core::ResolvedProfile>,
        included: &'a [PacketFragment],
        omissions: &'a [ContextOmission],
        token_estimate: u64,
        read_subjects: &'a [zap_wire::SubjectRef],
        write_subjects: &'a [zap_wire::SubjectRef],
        forks: &'a [zap_wire::ForkId],
        checks: &'a [zap_core::VerificationPlan],
        safe_stop: &'a zap_core::SafeStopContract,
        abstraction: &'a Option<AbstractionMap>,
        result_contract: &'a zap_wire::BoundedText<4096>,
    }
    let encoded = zap_wire::CanonicalOutput::encode_json(
        zap_wire::CodecEpoch::CURRENT,
        &PacketDigestInput {
            packet_id: &packet.packet_id,
            parent_packet_id: &packet.parent_packet_id,
            supersedes: &packet.supersedes,
            lowering_id: &packet.lowering_id,
            work_id: &packet.work_id,
            semantic_digest: packet.semantic_digest,
            basis_roots: &packet.basis_roots,
            desired_profile: &packet.desired_profile,
            resolved_profile: &packet.resolved_profile,
            included: &packet.included,
            omissions: &packet.omissions,
            token_estimate: packet.token_estimate,
            read_subjects: &packet.read_subjects,
            write_subjects: &packet.write_subjects,
            forks: &packet.forks,
            checks: &packet.checks,
            safe_stop: &packet.safe_stop,
            abstraction: &packet.abstraction,
            result_contract: &packet.result_contract,
        },
    )?;
    Ok(PacketDigest::hash(encoded.as_bytes()))
}

fn validate_abstraction(abstraction: &AbstractionMap) -> Result<(), ZapError> {
    if abstraction.concrete_to_abstract.is_empty()
        || abstraction.preserved_invariants.is_empty()
        || abstraction.reconstruction_checks.is_empty()
        || !abstraction.hidden_constraints.is_empty()
    {
        return Err(packet_error(
            "abstraction must preserve mappings and checks without hidden material constraints",
        ));
    }
    Ok(())
}

fn packet_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        PACKET_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn packet_missing(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        PACKET_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn packet_conflict(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        PACKET_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn sorted_unique_values<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
