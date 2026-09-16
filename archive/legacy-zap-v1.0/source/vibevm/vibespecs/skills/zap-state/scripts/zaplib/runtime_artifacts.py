"""Trusted generated-artifact capture and evidence applicability sequencing."""
from __future__ import annotations

import base64
from pathlib import Path
import copy

from .common import Refusal, exact, need, parse, sha
from .runtime_model import runtime_state
from .sources import validate_source_descriptor


def classify_worker_result(result):
    """Turn stable bridge/CLI argument failures into non-retrying configuration waits."""
    value = copy.deepcopy(result)
    if value.get("state") == "succeeded":
        stdout = value.get("stdout", {}); raw = Path(stdout.get("path", "")).read_bytes()
        need(len(raw) == stdout.get("bytes") and sha(raw) == stdout.get("sha256"), "RUNTIME_ARTIFACT", "worker result output identity differs")
        try:
            report = parse(raw)
        except (ValueError, UnicodeError, Refusal):
            report = None
        status = report.get("status") if isinstance(report, dict) and report.get("schema") == "zap-worker-candidate/1" else "candidate" if isinstance(report, dict) and report.get("candidate") is True else "stopped" if isinstance(report, dict) and report.get("safe") is True else "invalid"
        if status != "candidate":
            classification = "worker_blocked" if status == "blocked" else "worker_stopped" if status == "stopped" else "worker_result_invalid"
            value["diagnostic"] = {**value.get("diagnostic", {}), "classification": classification,
                                   "stdout_sha256": stdout["sha256"]}
        return value
    if value.get("diagnostic", {}).get("classification") in {"provider_quota", "provider_auth", "provider_unavailable", "rate_limit"}:
        return value
    if value.get("state") != "failed" or not isinstance(value.get("stderr"), dict):
        return value
    stderr = value["stderr"]; raw = Path(stderr["path"]).read_bytes()
    need(len(raw) == stderr["bytes"] and sha(raw) == stderr["sha256"], "RUNTIME_ARTIFACT", "worker stderr identity differs")
    lowered = raw.decode("utf-8", errors="replace").casefold()
    if any(marker in lowered for marker in ("cannot be used with", "invalid_json_schema", "not inside a trusted directory",
                                             "psargumentexception", "unrecognized option", "unknown option", "strict config")):
        value["diagnostic"] = {**value.get("diagnostic", {}), "classification": "configuration_error",
                               "stderr_sha256": stderr["sha256"]}
    return value


def verification_ready_for_review(row, *, artifact_capture_required):
    if row.get("state") != "observed":
        return False
    passed = row.get("result", {}).get("state") == "succeeded"
    return passed and (not artifact_capture_required or row.get("artifact_source") is not None)


def _verified_target(row, spec):
    stdout = row.get("result", {}).get("stdout", {})
    raw = Path(stdout.get("path", "")).read_bytes()
    need(len(raw) == stdout.get("bytes") and sha(raw) == stdout.get("sha256"),
         "RUNTIME_ARTIFACT", "verification output identity differs")
    value = parse(raw); exact(value, {"schema", "result", "artifacts", "summary"})
    need(value["schema"] == "zap-verification-output/1" and value["result"] == "pass" and isinstance(value["artifacts"], list),
         "RUNTIME_ARTIFACT", "verification did not report a passing artifact identity")
    target_path = Path(spec.target); target = (target_path if target_path.is_absolute() else Path(spec.cwd) / target_path).resolve(); found = []
    for item in value["artifacts"]:
        exact(item, {"path", "sha256", "bytes"})
        item_path = Path(item["path"]); item_target = (item_path if item_path.is_absolute() else Path(spec.cwd) / item_path).resolve()
        if item_target == target:
            found.append(item)
    need(len(found) == 1 and isinstance(found[0]["sha256"], str) and type(found[0]["bytes"]) is int,
         "RUNTIME_ARTIFACT", "verification output did not bind the configured target exactly once")
    return found[0], target


def attach_artifact_content(view, reader):
    artifact = view.get("artifact_source")
    if not artifact:
        return view
    need(callable(reader), "RUNTIME_ARTIFACT", "registered artifact reader is unavailable")
    blob = artifact["blob"]; offset = 0; raw = bytearray()
    while True:
        page = reader(blob["handle"], blob["sha256"], offset=offset)
        exact(page, {"schema", "handle", "kind", "sha256", "bytes", "offset", "returned_bytes", "complete", "next_offset", "encoding", "content"})
        need(page["schema"] == "zap-content/1" and page["handle"] == blob["handle"] and page["sha256"] == blob["sha256"]
             and page["bytes"] == blob["bytes"] and page["offset"] == offset, "RUNTIME_ARTIFACT", "artifact content page binding differs")
        part = page["content"].encode("utf-8") if page["encoding"] == "utf-8" else base64.b64decode(page["content"], validate=True)
        need(len(part) == page["returned_bytes"], "RUNTIME_ARTIFACT", "artifact content page length differs")
        raw.extend(part)
        if page["complete"]:
            need(page["next_offset"] is None, "RUNTIME_ARTIFACT", "complete artifact page has a continuation")
            break
        need(type(page["next_offset"]) is int and page["next_offset"] == offset + len(part) and page["next_offset"] > offset,
             "RUNTIME_ARTIFACT", "artifact page continuation differs")
        offset = page["next_offset"]
    captured = bytes(raw)
    need(len(captured) == blob["bytes"] and sha(captured) == blob["sha256"], "RUNTIME_ARTIFACT", "assembled artifact content differs")
    try:
        encoding, content = "utf-8", captured.decode("utf-8")
    except UnicodeDecodeError:
        encoding, content = "base64", base64.b64encode(captured).decode("ascii")
    view["artifact_content"] = {"handle": blob["handle"], "sha256": blob["sha256"], "bytes": blob["bytes"],
                                "encoding": encoding, "content": content}
    return view


def capture_verified_outputs(coordinator, actions):
    """Capture successful check targets, then explicitly assess their bounded use."""
    callback = coordinator.config.artifact_capture
    if callback is None:
        return
    state, _ = coordinator._load(); runtime = runtime_state(state)
    for verification_id, row in runtime["verification_jobs"].items():
        if row["state"] != "observed" or row.get("artifact_source") is not None or row.get("result", {}).get("state") != "succeeded":
            continue
        job = runtime["jobs"][row["work_job_id"]]; spec = coordinator._verification_spec(job["work_id"], row["plan"]["check_id"])
        source_id = f"artifact:{job['attempt_id']}:{spec.check_id}"
        try:
            verified, target = _verified_target(row, spec)
            captured = callback(target, coordinator.config.artifact_allowed_root, source_id=source_id,
                                source_kind="file", applicability_scope=None)
            need(isinstance(captured, dict) and set(captured) == {"source", "blob"}, "RUNTIME_ARTIFACT", "artifact callback result differs")
            descriptor = validate_source_descriptor(captured["source"]); blob = captured["blob"]
            bound = (Path(descriptor["root"]) / descriptor["path"]).resolve()
            need(descriptor["id"] == source_id and descriptor["source_kind"] == "file" and target == bound,
                 "RUNTIME_ARTIFACT", "artifact callback captured a different output")
            need(isinstance(blob, dict) and blob.get("sha256") == descriptor["content_sha256"] and blob.get("bytes") == descriptor["bytes"],
                 "RUNTIME_ARTIFACT", "artifact blob identity differs")
            need(descriptor["content_sha256"] == verified["sha256"] and descriptor["bytes"] == verified["bytes"],
                 "RUNTIME_STALE", "generated artifact changed after configured verification")
            capture = {"source_id": source_id, "sha256": descriptor["content_sha256"]}
            existing = state.get("extensions", {}).get("knowledge", {}).get("sources", {}).get(source_id)
            if existing is None:
                coordinator._observe("knowledge.source-recorded", {"source": descriptor}, f"Capture verified output {source_id}",
                                     f"artifact-source:{source_id}:{descriptor['content_sha256']}")
            elif existing.get("content_sha256") != descriptor["content_sha256"] or existing.get("bytes") != descriptor["bytes"]:
                observed = {"status": "changed", "sha256": descriptor["content_sha256"], "bytes": descriptor["bytes"], "detail": None}
                coordinator._action("knowledge.source-observed", {"source_id": source_id, "observed": observed},
                                    f"Observe changed verified output {source_id}", f"artifact-changed:{source_id}:{descriptor['content_sha256']}",
                                    "evidence.adjudicate", source_rows=[{"source_id": source_id, "sha256": existing["content_sha256"]}])
                actions.append({"kind": "artifact_changed", "verification_id": verification_id, "source_id": source_id})
                continue
            scope = {"kind": "subjects", "subjects": [{"kind": "task", "id": job["work_id"]}]}
            coordinator._action("knowledge.applicability-assessed", {"source_id": source_id, "status": "applicable", "scope": scope,
                                "evidence_refs": [row["evidence_id"]], "basis": "Configured verification checked this exact generated artifact"},
                                f"Assess verified output {source_id}", f"artifact-applicability:{source_id}:{descriptor['content_sha256']}",
                                "evidence.adjudicate", source_rows=[capture])
            coordinator._action("knowledge.dependency-recorded", {"id": f"dependency:{source_id}:{job['work_id']}",
                                "prerequisite": {"kind": "source", "id": source_id}, "dependent": {"kind": "task", "id": job["work_id"]},
                                "relation": "verifies"}, f"Bind verified output {source_id} to {job['work_id']}",
                                f"artifact-dependency:{source_id}:{job['work_id']}", "evidence.adjudicate", source_rows=[capture])
            coordinator._action("knowledge.closure-assessed", {"subject": {"kind": "source", "id": source_id}, "status": "complete",
                                "boundary": [{"kind": "task", "id": job["work_id"]}], "missing": [], "evidence_refs": [row["evidence_id"]],
                                "basis": "Configured verification bounded the generated artifact and its consuming task"},
                                f"Close verified output boundary {source_id}", f"artifact-closure:{source_id}:{descriptor['content_sha256']}",
                                "evidence.adjudicate", source_rows=[capture])
            payload = {"schema": "zap-runtime/verification-artifact-captured/1", "verification_id": verification_id,
                       "source_id": source_id, "source_sha256": descriptor["content_sha256"], "blob": blob}
            coordinator._observe("runtime.verification-artifact-captured", payload, f"Bind verified artifact {source_id}",
                                 f"verification-artifact:{verification_id}:{descriptor['content_sha256']}")
            actions.append({"kind": "verification_artifact_captured", "verification_id": verification_id, "source_id": source_id})
        except (OSError, ValueError, Refusal) as exc:
            actions.append({"kind": "artifact_capture_unavailable", "verification_id": verification_id,
                            "code": getattr(exc, "code", type(exc).__name__)})


__all__ = ("attach_artifact_content", "capture_verified_outputs", "classify_worker_result", "verification_ready_for_review")
