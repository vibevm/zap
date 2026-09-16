"""Actual-source refresh for ready, running, accepted, and closing work."""
from pathlib import Path

from .common import Refusal
from .control import active_policy
from .domain import domain_frontier, domain_state
from .runtime_model import runtime_state
from .runtime_packets import active_contract
from .runtime_reconciliation import ADMISSION_PENDING
from .sources import capture_vibevm_facts, observe_source, validate_source_descriptor

SOURCE_FIELDS = {"schema", "id", "source_kind", "root", "path", "content_sha256", "bytes", "applicability_scope"}


def refresh_sources(coordinator, actions):
    state, _ = coordinator._load(); domain = domain_state(state); runtime = runtime_state(state)
    policy = active_policy(state)
    if policy and any(row["scope"] == "campaign" for row in policy.get("pauses", [])):
        return
    work_ids = set(domain_frontier(state)) | {row["work_id"] for row in runtime["jobs"].values() if row["state"] not in {"accepted", "retry_released"}}
    wanted = set()
    for work_id in work_ids:
        try:
            _version, contract, _digest = active_contract(state, work_id); wanted.update(contract["source_handles"])
        except Refusal:
            continue
    wanted.update(row["artifact_source"]["source_id"] for row in runtime["verification_jobs"].values() if row.get("artifact_source"))
    for evidence in domain["evidence_adjudications"].values():
        if evidence.get("disposition") == "accepted":
            wanted.update(evidence.get("source_refs", []))
    sources = state.get("extensions", {}).get("knowledge", {}).get("sources", {})
    for source_id in sorted(wanted):
        source = sources.get(source_id)
        if source is None:
            continue
        descriptor = validate_source_descriptor({key: source[key] for key in SOURCE_FIELDS}); observed = observe_source(descriptor)["observed"]
        last = source.get("observations", [])[-1] if source.get("observations") else None
        if last and all(last.get(key) == observed.get(key) for key in ("status", "sha256", "bytes", "detail")):
            continue
        payload = {"source_id": source_id, "observed": observed}
        try:
            coordinator._action("knowledge.source-observed", payload, f"Refresh bound source {source_id}",
                                coordinator._event_id("source-observation", source_id, payload), "evidence.adjudicate",
                                source_rows=[{"source_id": source_id, "sha256": source["content_sha256"]}])
            actions.append({"kind": "source_observed", "source_id": source_id, "status": observed["status"]})
            if descriptor["source_kind"] == "vibevm_xml_spec" and observed["status"] == "changed":
                capture = capture_vibevm_facts(Path(descriptor["root"]) / descriptor["path"], descriptor["root"], source_id=source_id,
                                               applicability_scope=descriptor["applicability_scope"])
                native_payload = {"schema": "zap-runtime/native-facts-observed/1", "source_id": source_id,
                                  "previous_sha256": descriptor["content_sha256"], "capture": capture}
                coordinator._observe("runtime.native-facts-observed", native_payload, f"Capture changed native facts for review {source_id}",
                                     coordinator._event_id("native-facts", source_id, native_payload))
                actions.append({"kind": "native_facts_observed", "source_id": source_id, "fact_count": len(capture["facts"])})
        except Refusal as exc:
            if exc.code not in ADMISSION_PENDING:
                raise
        except (OSError, ValueError) as exc:
            actions.append({"kind": "native_facts_unavailable", "source_id": source_id, "error": type(exc).__name__})


__all__ = ("refresh_sources",)
