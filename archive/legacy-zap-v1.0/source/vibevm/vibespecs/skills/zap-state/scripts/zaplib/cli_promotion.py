"""CLI composition for proof-bound project fact promotion."""
from __future__ import annotations

from pathlib import Path
from typing import Any

from .artifacts import PrivateArtifactStore
from .common import need, packed, parse, sha
from .control import active_policy
from .engine import Engine
from .knowledge_promotion import build_promotion_proposal, promote_fact
from .runtime import ACTIVE_JOB_STATES, runtime_state


def _default_assessment(state: dict[str, Any], assessment_id: str) -> dict[str, Any]:
    policy = active_policy(state)
    need(policy is not None, "INACTIVE", "promotion requires an active charter")
    active_jobs = sorted(
        job_id for job_id, row in runtime_state(state)["jobs"].items()
        if row["state"] in ACTIVE_JOB_STATES
    )
    return {
        "schema": "zap-assessment/1",
        "assessment_id": assessment_id,
        "policy_id": policy["stop_policy"]["policy_id"],
        "policy_revision": policy["stop_policy"]["revision"],
        "phase": "before_action",
        "values": {},
        "drain_targets": active_jobs,
    }


def promote_from_cli(
    engine: Engine,
    *,
    credential_id: str,
    credential: str,
    project_root: str,
    artifact_store: str,
    promotion_id: str,
    fact_id: str,
    source_refs: list[str],
    target: str,
    proof_refs: list[str],
    authorization_ref: str,
    assessment_path: str | None = None,
) -> dict[str, Any]:
    state = engine.load()[0]
    principal = engine.trust.authenticate(credential_id, credential, state["plan"]["plan_id"])
    need(principal.role in {"owner", "coordinator"} and "fact.promote" in principal.action_classes, "AUTHORIZATION", "credential cannot promote facts")
    proposal = build_promotion_proposal(
        state, promotion_id=promotion_id, fact_id=fact_id,
        source_refs=source_refs, target=target,
    )
    artifacts = PrivateArtifactStore(artifact_store, create=True)

    def authorization(_proposal: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
        policy = active_policy(current)
        need(policy is not None and "fact.promote" in policy["allowed_actions"], "AUTHORIZATION", "active charter does not delegate fact promotion")
        return {
            "authorized": True,
            "action": "fact.promote",
            "promotion_id": promotion_id,
            "accepted_proof_refs": list(proof_refs),
            "authorization_ref": authorization_ref,
        }

    def event_writer(effect: dict[str, Any]) -> dict[str, Any]:
        current = engine.load()[0]
        effect_payload = effect["payload"]
        target_path = Path(project_root) / effect_payload["target"]
        artifact = artifacts.capture_file(
            f"promotion:{promotion_id}", target_path, project_root, kind="promotion",
        )
        need(artifact["sha256"] == effect_payload["artifact_sha256"], "PROMOTION", "private promotion artifact hash differs")
        adapter_receipt = f"promotion-effect:{sha(packed(effect))}"
        payload = {
            "schema": "zap-domain/fact-promotion-recorded/1",
            "promotion_id": promotion_id,
            "fact_id": fact_id,
            "target_handle": artifact["handle"],
            "content_sha256": artifact["sha256"],
            "evidence_ids": list(proof_refs),
            "adapter_receipt": adapter_receipt,
            "summary": f"Promoted fact {fact_id} through validated adapter",
        }
        command = {
            "event_id": f"promotion:{promotion_id}",
            "base_revision": current["revision"],
            "kind": "domain.fact-promotion-recorded",
            "reason": {"summary": f"Record promotion {promotion_id}"},
            "payload": payload,
        }
        policy = active_policy(current)
        source_rows = [
            {
                "source_id": source_id,
                "sha256": current["extensions"]["knowledge"]["sources"][source_id]["content_sha256"],
            }
            for source_id in sorted(source_refs)
        ]
        action = {
            "schema": "zap-action/1",
            "action_id": f"action:promotion:{promotion_id}",
            "action_class": "fact.promote",
            "campaign_id": policy["campaign_id"],
            "base_sha256": policy["base_sha256"],
            "charter_revision": policy["revision"],
            "payload_sha256": sha(packed(payload)),
            "source_captures": source_rows,
            "branch_id": None,
            "run_id": None,
            "problem_id": None,
        }
        assessment = (
            parse(Path(assessment_path).read_bytes())
            if assessment_path
            else _default_assessment(current, f"assessment:promotion:{promotion_id}")
        )
        return engine.service.apply_control_action(
            command, action, assessment,
            credential_id=credential_id, credential=credential,
        )

    return promote_fact(
        state, proposal, project_root,
        authorization=authorization,
        event_writer=event_writer,
    )
