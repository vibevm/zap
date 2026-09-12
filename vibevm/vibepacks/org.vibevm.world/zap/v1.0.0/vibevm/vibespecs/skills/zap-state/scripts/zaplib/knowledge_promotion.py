"""Bound project-fact promotion through injected authorization and event I/O."""
from __future__ import annotations

import copy
import os
from pathlib import Path
from typing import Any, Callable
import uuid

from .common import exact, identity, need, packed, sha, string, strings
from .sources import current_applicability, observe_source, validate_source_descriptor
from .storage import safe_path, write_new

PROMOTION_SCHEMA = "zap-fact-promotion/1"
PROJECT_FACT_SCHEMA = "zap-project-fact/1"
PORTABLE_PROOF_SCHEMA = "zap-portable-proof/1"
DOMAIN_EVIDENCE_SCHEMA = "zap-domain/evidence-adjudicated/1"
DOMAIN_EVIDENCE_FIELDS = {"schema", "evidence_id", "expected_revision", "disposition", "applies_to", "source_refs", "method",
                          "limitations", "revision", "event_id", "applicability_at_adjudication", "history"}
SOURCE_DESCRIPTOR_FIELDS = {"schema", "id", "source_kind", "root", "path", "content_sha256", "bytes", "applicability_scope"}
PROMOTION_OPERATIONS = {
    "fact.promote": {"required": ["state", "proposal", "project_root", "authorization", "event_writer"], "optional": [],
                     "effect": "new_bound_project_fact", "route": "trusted_service", "action": "fact.promote",
                     "authorization": "injected_trusted_service", "existing": "exact_only"},
}


def _fact(state: dict[str, Any], fact_id: str) -> tuple[str, dict[str, Any]]:
    if fact_id in state.get("facts", {}):
        return "recorded_fact", copy.deepcopy(state["facts"][fact_id])
    native = state.get("extensions", {}).get("knowledge", {}).get("native_facts", {})
    if fact_id in native:
        return "native_spec_fact", copy.deepcopy(native[fact_id])
    raise ValueError(f"unknown promotable fact: {fact_id}")


def build_promotion_proposal(
    state: dict[str, Any],
    *,
    promotion_id: str,
    fact_id: str,
    source_refs: list[str],
    target: str,
) -> dict[str, Any]:
    """Build a pure proposal; this does not authorize or write anything."""
    promotion_id = identity(promotion_id)
    fact_id = identity(fact_id)
    source_refs = strings(source_refs, "promotion source refs")
    need(source_refs, "PROMOTION", "promotion requires captured sources")
    applicability = current_applicability(state, source_refs)
    need(applicability["status"] == "applicable", "APPLICABILITY", "promotion sources are stale, unknown, or incompletely bounded")
    target_path = Path(string(target, "promotion target"))
    need(not target_path.is_absolute() and ".." not in target_path.parts and target_path.suffix.casefold() == ".json", "PATH", "promotion target must be a relative JSON path")
    fact_kind, fact = _fact(state, fact_id)
    if fact_kind == "native_spec_fact":
        need(fact["source_id"] in source_refs, "PROMOTION", "native fact source is not bound by the proposal")
    return {"schema": PROMOTION_SCHEMA, "promotion_id": promotion_id, "fact_id": fact_id, "fact_kind": fact_kind,
            "expected_revision": state["revision"], "base_sha256": state["base_sha256"], "source_refs": list(source_refs),
            "applicability": applicability, "target": target_path.as_posix(), "fact": fact}


def _validate_authorization(value: Any, proposal: dict[str, Any]) -> dict[str, Any]:
    exact(value, {"authorized", "action", "promotion_id", "accepted_proof_refs", "authorization_ref"})
    need(value["authorized"] is True and value["action"] == "fact.promote" and value["promotion_id"] == proposal["promotion_id"], "AUTHORITY", "fact promotion was not authorized")
    proofs = strings(value["accepted_proof_refs"], "accepted proof refs")
    need(proofs and len(set(proofs)) == len(proofs), "ACCEPTANCE", "fact promotion requires distinct accepted proof")
    for proof in proofs:
        identity(proof)
    string(value["authorization_ref"], "authorization reference")
    return copy.deepcopy(value)


def portable_accepted_proofs(state: dict[str, Any], proof_ids: list[str], bound_source_refs: list[str]) -> list[dict[str, Any]]:
    """Resolve accepted domain evidence into portable, self-describing proof rows."""
    adjudications = state.get("extensions", {}).get("domain", {}).get("evidence_adjudications", {})
    evidence = state.get("evidence", {})
    sources = state.get("extensions", {}).get("knowledge", {}).get("sources", {})
    proofs = []
    for proof_id in proof_ids:
        row = adjudications.get(proof_id)
        need(row is not None, "ACCEPTANCE", f"accepted proof does not exist: {proof_id}")
        exact(row, DOMAIN_EVIDENCE_FIELDS)
        need(row["schema"] == DOMAIN_EVIDENCE_SCHEMA and row["evidence_id"] == proof_id and row["disposition"] == "accepted", "ACCEPTANCE", f"proof is not accepted: {proof_id}")
        core = evidence.get(proof_id)
        need(core is not None and core.get("result") in {"observed_pass", "observed_fail"}, "ACCEPTANCE", f"accepted proof lacks a concrete observation: {proof_id}")
        source_refs = strings(row["source_refs"], "accepted proof source refs")
        need(set(source_refs) <= set(bound_source_refs), "PROMOTION", "accepted proof sources are not all bound by the promotion")
        applicability = current_applicability(state, source_refs)
        need(applicability["status"] == "applicable" and not applicability["incomplete_closure"], "APPLICABILITY", f"accepted proof is no longer applicable: {proof_id}")
        source_rows = []
        for source_id in source_refs:
            source = sources[source_id]
            source_rows.append({"id": source_id, "path": source["path"], "content_sha256": source["content_sha256"],
                                "applicability_scope": copy.deepcopy(state["extensions"]["knowledge"]["applicability"][source_id]["scope"])})
        proofs.append({"schema": PORTABLE_PROOF_SCHEMA, "evidence_id": proof_id,
                       "observation": {key: copy.deepcopy(core[key]) for key in ("claim", "subject", "result", "artifact_refs", "node_refs")},
                       "adjudication_revision": row["revision"], "adjudication_event_id": row["event_id"],
                       "applies_to": copy.deepcopy(row["applies_to"]), "method": copy.deepcopy(row["method"]),
                       "limitations": copy.deepcopy(row["limitations"]), "sources": source_rows,
                       "applicability_at_adjudication": copy.deepcopy(row["applicability_at_adjudication"]),
                       "current_applicability": applicability})
    return proofs


def _artifact(
    state: dict[str, Any], proposal: dict[str, Any], grant: dict[str, Any],
    portable_proofs: list[dict[str, Any]], source_observations: list[dict[str, Any]],
) -> dict[str, Any]:
    knowledge = state["extensions"]["knowledge"]
    sources = []
    for source_id in proposal["source_refs"]:
        source = knowledge["sources"][source_id]
        sources.append({"id": source_id, "path": source["path"], "content_sha256": source["content_sha256"],
                        "applicability_scope": copy.deepcopy(knowledge["applicability"][source_id]["scope"])})
    return {"schema": PROJECT_FACT_SCHEMA, "promotion_id": proposal["promotion_id"], "fact_id": proposal["fact_id"],
            "fact_kind": proposal["fact_kind"], "fact": copy.deepcopy(proposal["fact"]), "base_sha256": proposal["base_sha256"],
            "state_revision": proposal["expected_revision"], "sources": sources, "accepted_proof_refs": grant["accepted_proof_refs"],
            "portable_proofs": portable_proofs, "source_reobservations": source_observations,
            "authorization_ref": grant["authorization_ref"]}


def promote_fact(
    state: dict[str, Any],
    proposal: dict[str, Any],
    project_root: str | os.PathLike[str],
    *,
    authorization: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]],
    event_writer: Callable[[dict[str, Any]], Any],
    fault: Callable[[str], None] | None = None,
) -> dict[str, Any]:
    """Write one bound fact, then persist its effect receipt or roll back truthfully."""
    exact(proposal, {"schema", "promotion_id", "fact_id", "fact_kind", "expected_revision", "base_sha256", "source_refs", "applicability", "target", "fact"})
    need(proposal["schema"] == PROMOTION_SCHEMA, "PROMOTION", "unsupported promotion proposal")
    need(proposal["expected_revision"] == state["revision"] and proposal["base_sha256"] == state["base_sha256"], "STALE", "promotion proposal state binding differs")
    rebuilt = build_promotion_proposal(state, promotion_id=proposal["promotion_id"], fact_id=proposal["fact_id"],
                                       source_refs=proposal["source_refs"], target=proposal["target"])
    need(rebuilt == proposal, "STALE", "promotion proposal inputs changed")
    grant = _validate_authorization(authorization(copy.deepcopy(proposal), state), proposal)
    portable_proofs = portable_accepted_proofs(state, grant["accepted_proof_refs"], proposal["source_refs"])
    root = safe_path(project_root)
    need(root.is_dir(), "PATH", "project root must exist")
    target = safe_path(root / proposal["target"])
    try:
        target.relative_to(root)
    except ValueError as exc:
        raise ValueError("promotion target is outside project root") from exc
    need(target.parent.is_dir(), "PATH", "promotion target parent must already exist")
    source_observations = []
    source_records = state["extensions"]["knowledge"]["sources"]
    for source_id in proposal["source_refs"]:
        descriptor = validate_source_descriptor({key: source_records[source_id][key] for key in SOURCE_DESCRIPTOR_FIELDS})
        observation = observe_source(descriptor)
        need(observation["observed"]["status"] == "current" and observation["observed"]["sha256"] == descriptor["content_sha256"] and
             observation["observed"]["bytes"] == descriptor["bytes"], "STALE", f"promotion source changed or is unavailable: {source_id}")
        source_observations.append(observation)
    artifact = _artifact(state, proposal, grant, portable_proofs, source_observations)
    raw = packed(artifact) + b"\n"
    created = False
    ownership_path: Path | None = None
    if target.exists():
        need(target.is_file() and target.read_bytes() == raw, "OUTPUT_EXISTS", "promotion target contains unexpected content")
    else:
        ownership_path = safe_path(target.parent / f".{target.name}.{uuid.uuid4().hex}.tmp")
        try:
            write_new(ownership_path, raw)
            os.link(ownership_path, target)
            created = True
        except Exception:
            if ownership_path.exists():
                ownership_path.unlink()
            raise
    effect = {"kind": "fact.promotion-effect-recorded", "payload": {"promotion_id": proposal["promotion_id"], "fact_id": proposal["fact_id"],
              "target": proposal["target"], "artifact_sha256": sha(raw), "base_sha256": proposal["base_sha256"],
              "state_revision": proposal["expected_revision"], "source_refs": proposal["source_refs"],
              "accepted_proof_refs": grant["accepted_proof_refs"], "authorization_ref": grant["authorization_ref"]}}
    try:
        if fault:
            fault("after_artifact")
        event_receipt = event_writer(copy.deepcopy(effect))
    except Exception as exc:  # effect adapters report failure as data; callers decide retry
        artifact_status = "present_unreceipted"
        if created:
            try:
                if ownership_path is not None and ownership_path.exists() and target.exists() and os.path.samestat(ownership_path.stat(), target.stat()) and target.read_bytes() == raw:
                    target.unlink()
                    artifact_status = "rolled_back"
                elif not target.exists():
                    artifact_status = "absent_unreceipted"
            except OSError:
                artifact_status = "present_unreceipted" if target.exists() else "absent_unreceipted"
        if ownership_path is not None and ownership_path.exists():
            ownership_path.unlink()
        return {"ok": False, "code": "PROMOTION_RECEIPT", "message": str(exc), "promotion_id": proposal["promotion_id"],
                "artifact_status": artifact_status, "target": str(target),
                "artifact_sha256": sha(raw), "event_recorded": False}
    if ownership_path is not None and ownership_path.exists():
        ownership_path.unlink()
    if not target.is_file() or target.read_bytes() != raw:
        return {"ok": False, "code": "PROMOTION_POSTCONDITION", "message": "artifact changed after effect receipt",
                "promotion_id": proposal["promotion_id"], "artifact_status": "changed_after_receipt", "target": str(target),
                "artifact_sha256": sha(raw), "event_recorded": True, "event_receipt": event_receipt}
    return {"ok": True, "promotion_id": proposal["promotion_id"], "target": str(target), "artifact_sha256": sha(raw),
            "artifact_status": "created" if created else "recovered_existing", "event_recorded": True, "event_receipt": event_receipt,
            "effect": effect}
