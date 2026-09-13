"""Render deterministic legacy ZAP compatibility fixtures from reference functions."""
from __future__ import annotations

import argparse
import base64
import copy
import datetime as dt
import json
import math
from pathlib import Path
import platform
import sys
import tomllib
from typing import Any, Callable


HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parents[3] / "skills" / "zap-state" / "scripts"
sys.path.insert(0, str(SCRIPTS))

from zaplib.common import PROJECTION_SCHEMA, Refusal, packed, parse, sha, unwire, wire  # noqa: E402
from zaplib.graph import frontier, validate_plan, validate_tasks  # noqa: E402
from zaplib.records import CORE_HANDLERS, apply_command, initial_state  # noqa: E402
from zaplib.snapshots import DEFAULT_REDUCER_VERSION, SNAPSHOT_SCHEMA, _reducer  # noqa: E402
from zaplib.storage import capture, replay_committed  # noqa: E402


PLAN_TEXT = '''schema = 1
plan_id = "legacy-fixture"
revision = 4
root_node = "R"
current_node = "T"
recorded_datetime = 1979-05-27T07:00:00Z
recorded_date = 1979-05-27
recorded_time = 07:32:00.123456
negative_zero = -0.0
positive_infinity = inf
reserved = { "$zap_type" = "literal", value = "keep" }
future_plan_field = { nested = ["café", "rocket 🚀"] }

[[mandate]]
id = "M"
text = "Preserve every obligation."
disposition = "owned"
nodes = ["R", "G", "T"]

[[node]]
id = "R"
parent = ""
title = "Legacy fixture"
kind = "campaign"
state = "planned"
order = 0
depends_on = []
mandates = ["M"]
acceptance = ["The fixture remains lossless."]
evidence = []

[[node]]
id = "G"
parent = "R"
title = "Fixture group"
kind = "group"
state = "planned"
order = 1
depends_on = []
mandates = ["M"]
acceptance = ["The task is represented."]
evidence = []

[[node]]
id = "T"
parent = "G"
title = "Café 🚀"
kind = "atom"
state = "planned"
order = 2
depends_on = []
mandates = ["M"]
acceptance = ["Classification and evidence replay."]
evidence = []
future_node_field = { retain = [2, 1] }
'''


def task() -> dict[str, Any]:
    return {
        "id": "T",
        "title": "Café 🚀",
        "goal": "Replay exact legacy semantics.",
        "read_paths": ["input/漢字"],
        "write_paths": ["output"],
        "steps": ["Record evidence"],
        "positive_cases": ["Canonical bytes replay"],
        "negative_cases": ["Stale revisions refuse"],
        "checks": ["Scoped fixture comparison"],
        "acceptance": ["Every digest matches"],
        "safe_stop": "Fixture candidate saved",
        "commit_subject": "test: preserve legacy codec evidence",
        "notes": ["No activation or execution"],
        "future_task_field": {"retain": [2, 1]},
    }


def command(event_id: str, revision: Any, kind: str, payload: dict[str, Any]) -> dict[str, Any]:
    return {
        "event_id": event_id,
        "base_revision": revision,
        "kind": kind,
        "reason": {"summary": "Deterministic legacy fixture"},
        "payload": payload,
    }


def event_from(command_value: dict[str, Any], revision: int) -> dict[str, Any]:
    return {
        "seq": revision,
        "revision": revision,
        "previous_revision": revision - 1,
        "event_id": command_value["event_id"],
        "kind": command_value["kind"],
        "reason": command_value["reason"],
        "payload": command_value["payload"],
        "command_sha256": sha(packed(command_value)),
    }


def refusal(call: Callable[[], Any]) -> dict[str, str]:
    try:
        call()
    except Refusal as exc:
        return {"outcome": "refused", "code": exc.code, "message": str(exc)}
    raise AssertionError("expected legacy Refusal")


def state_summary(state: dict[str, Any]) -> dict[str, Any]:
    return {
        "revision": state["revision"],
        "execution_mode": state["execution_mode"],
        "owner_contract": state["owner_contract"],
        "node_states": {row["id"]: row["state"] for row in state["plan"]["node"]},
        "frontier": frontier(state),
        "classification_T": state["classifications"]["T"],
        "evidence_ids": sorted(state["evidence"]),
        "fact_ids": sorted(state["facts"]),
        "unknown_region_ids": sorted(state["unknown_regions"]),
        "relation_count": len(state["relations"]),
        "state_sha256": sha(packed(state)),
    }


def codec_case(case_id: str, value: Any, describe: dict[str, Any] | None = None) -> dict[str, Any]:
    encoded = packed(value)
    decoded = parse(encoded, tagged=True)
    result = {
        "id": case_id,
        "outcome": "accepted",
        "packed_utf8": encoded.decode("utf-8"),
        "packed_base64": base64.b64encode(encoded).decode("ascii"),
        "packed_sha256": sha(encoded),
        "wire_value": wire(value),
        "roundtrip_wire_value": wire(decoded),
    }
    if describe:
        result.update(describe)
    return result


def render() -> dict[str, bytes]:
    plan_raw = PLAN_TEXT.encode("utf-8")
    plan = tomllib.loads(PLAN_TEXT)
    nodes, _, _ = validate_plan(plan)
    group = {"id": "G", "tasks": [task()], "future_group_field": "raw-capture-only"}
    task_raw = (json.dumps(group, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    task_group = parse(task_raw)
    task_contracts = validate_tasks([task_group], nodes)

    base = {
        "schema": "zap/1",
        "plan": plan,
        "task_contracts": task_contracts,
        "sources": {
            "plan": capture(Path("tiny-campaign/plan.toml"), plan_raw),
            "tasks": [capture(Path("tiny-campaign/tasks/G.json"), task_raw)],
        },
    }
    base_raw = packed(base) + b"\n"
    base_sha = sha(base_raw)
    receipt = {
        "seq": 0,
        "revision": 0,
        "previous_revision": None,
        "event_id": "legacy-import-000",
        "kind": "store.imported",
        "base_sha256": base_sha,
    }
    receipt_line = packed(receipt) + b"\n"

    commands = [
        command("legacy-event-001", 0, "node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype"}),
        command("legacy-event-002", 1, "evidence.recorded", {
            "id": "E", "claim": "Canonical replay observed", "subject": "fixture:legacy-bytes",
            "result": "observed_pass", "artifact_refs": ["fixture:trace"], "node_refs": ["T"],
        }),
        command("legacy-event-003", 2, "fact.recorded", {
            "id": "F", "statement": "The legacy projection replayed.", "status": "observed",
            "node_refs": ["T"], "evidence_refs": ["E"], "source_refs": ["fixture:legacy"],
        }),
        command("legacy-event-004", 3, "knowledge.region-recorded", {
            "id": "K", "question": "Which future mapping applies to 🚀?", "node_refs": ["T"],
        }),
    ]
    events = [event_from(value, index) for index, value in enumerate(commands, 1)]
    event_lines = [packed(receipt) + b"\n", *(packed(value) + b"\n" for value in events)]
    events_raw = b"".join(event_lines)

    states = [initial_state(base, base_sha)]
    for value in commands:
        states.append(apply_command(states[-1], value))
    replayed, applied, seen = replay_committed(
        initial_state(base, base_sha), events, seen={receipt["event_id"]: receipt}
    )
    assert packed(replayed) == packed(states[-1])
    assert len(applied) == len(events) and len(seen) == len(events) + 1

    reducer = _reducer(DEFAULT_REDUCER_VERSION, CORE_HANDLERS)
    snapshot = {
        "schema": SNAPSHOT_SCHEMA,
        "base_sha256": base_sha,
        "projection_schema": PROJECTION_SCHEMA,
        "reducer": reducer,
        "revision": states[-1]["revision"],
        "committed_prefix": {
            "sha256": sha(events_raw),
            "bytes": len(events_raw),
            "lines": len(event_lines),
            "last_event_id": events[-1]["event_id"],
        },
        "state_sha256": sha(packed(states[-1])),
        "state": states[-1],
    }
    snapshot_raw = packed(snapshot) + b"\n"

    ordered_collision = {"value": "keep", "$zap_type": "literal", "z": 0}
    nested_collision = {"outer": {"second": 2, "$zap_type": "literal", "first": 1}}
    codec_cases = {
        "schema": "zap-legacy-codec-corpus/1",
        "canonical_encoder": "zaplib.common.packed",
        "cases": [
            codec_case("ascii", {"z": "last", "a": "first"}),
            codec_case("unicode-bmp-and-non-bmp", {"bmp": "café 漢字", "non_bmp": "🚀"}),
            codec_case("nested-mapping-order", {"z": {"z": 2, "a": 1}, "a": [{"d": 4, "c": 3}]}),
            codec_case("reserved-tag-collision", ordered_collision, {"input_key_order": list(ordered_collision)}),
            codec_case("nested-reserved-tag-collision", nested_collision, {"input_key_order": list(nested_collision["outer"])}),
            codec_case("negative-zero-and-exponents", [-0.0, 1e20, 1e-7, 1.2345678901234567e30], {
                "negative_zero_sign_preserved": math.copysign(1.0, parse(packed(-0.0), tagged=True)) < 0,
            }),
            codec_case("date", dt.date(1979, 5, 27)),
            codec_case("time", dt.time(7, 32, 0, 123456)),
            codec_case("datetime-utc", dt.datetime(1979, 5, 27, 7, 32, tzinfo=dt.timezone.utc)),
            codec_case("datetime-offset", dt.datetime(1979, 5, 27, 7, 32, tzinfo=dt.timezone(dt.timedelta(hours=5, minutes=30)))),
            codec_case("nonfinite-nan", float("nan"), {"roundtrip_is_nan": math.isnan(unwire(wire(float("nan"))))}),
            codec_case("nonfinite-positive-infinity", float("inf")),
            codec_case("nonfinite-negative-infinity", float("-inf")),
        ],
        "refusals": [
            {"id": "duplicate-json-member", **refusal(lambda: parse(b'{"a":1,"a":2}'))},
            {"id": "bare-json-nan", **refusal(lambda: parse(b'{"value":NaN}'))},
            {"id": "bare-json-positive-infinity", **refusal(lambda: parse(b'{"value":Infinity}'))},
            {"id": "unknown-tagged-value", **refusal(lambda: parse(b'{"$zap_type":"future","value":"x"}', tagged=True))},
            {"id": "tagged-value-extra-field", **refusal(lambda: parse(b'{"$zap_type":"date","value":"1979-05-27","extra":true}', tagged=True))},
        ],
    }

    pending_fragment = packed(events[-1])[:-1]
    crlf_line = packed(events[0]) + b"\r\n"
    line_domains = {
        "schema": "zap-legacy-hash-domains/1",
        "base": {
            "packed_without_terminator": {"bytes": len(base_raw) - 1, "sha256": sha(base_raw[:-1])},
            "canonical_file_with_lf": {"bytes": len(base_raw), "sha256": base_sha},
        },
        "commands": [
            {"event_id": value["event_id"], "base_revision": value["base_revision"], "bytes": len(packed(value)), "sha256": sha(packed(value))}
            for value in commands
        ],
        "journal": {
            "canonical_prefix_with_lf": {"bytes": len(events_raw), "lines": len(event_lines), "sha256": sha(events_raw)},
            "first_event_without_terminator": {"bytes": len(packed(events[0])), "sha256": sha(packed(events[0]))},
            "first_event_with_lf": {"bytes": len(event_lines[1]), "sha256": sha(event_lines[1])},
            "first_event_with_crlf": {"bytes": len(crlf_line), "sha256": sha(crlf_line)},
            "pending_final_fragment": {
                "bytes": len(pending_fragment), "sha256": sha(pending_fragment),
                "base64": base64.b64encode(pending_fragment).decode("ascii"), "committed": False,
            },
        },
        "snapshot": {
            "state_sha256": snapshot["state_sha256"],
            "reducer_identity_sha256": reducer["identity_sha256"],
            "packed_without_terminator": {"bytes": len(snapshot_raw) - 1, "sha256": sha(snapshot_raw[:-1])},
            "file_with_lf": {"bytes": len(snapshot_raw), "sha256": sha(snapshot_raw)},
        },
    }

    state_one = states[1]
    stale_command = command("legacy-refusal-stale", 0, "knowledge.region-recorded", {"id": "STALE", "question": "stale", "node_refs": ["T"]})
    bool_revision = command("legacy-refusal-bool", False, "node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype"})
    unknown_kind = command("legacy-refusal-kind", 0, "future.event", {})
    extra_payload = command("legacy-refusal-fields", 0, "node.classified", {"node_id": "T", "work_type": "change", "maturity": "prototype", "extra": True})
    missing_evidence = command("legacy-refusal-fact", 0, "fact.recorded", {"id": "F0", "statement": "Unsupported observed fact", "status": "observed", "node_refs": ["T"], "evidence_refs": [], "source_refs": []})
    first_duplicate_state, first_duplicate_applied, _ = replay_committed(initial_state(base, base_sha), [events[0], events[0]], seen={receipt["event_id"]: receipt})
    conflicting = copy.deepcopy(events[0])
    conflicting["payload"]["maturity"] = "functional"
    gap = copy.deepcopy(events[0])
    gap["seq"] = gap["revision"] = 2
    bad_hash = copy.deepcopy(events[0])
    bad_hash["command_sha256"] = "0" * 64
    expectations = {
        "schema": "zap-legacy-campaign-expectations/1",
        "legacy_identity_epoch": "zap/1",
        "authority": {"execution_mode": "draft", "owner_contract_activated": False, "commands_executed": False},
        "source_preservation": {
            "plan_sha256": sha(plan_raw), "task_group_sha256": sha(task_raw),
            "unknown_plan_field_preserved": wire(plan["future_plan_field"]),
            "unknown_node_field_preserved": next(row for row in plan["node"] if row["id"] == "T")["future_node_field"],
            "unknown_task_field_preserved": task_contracts["T"]["future_task_field"],
            "unknown_group_field_is_raw_capture_only": group["future_group_field"],
        },
        "base_sha256": base_sha,
        "per_revision": [state_summary(value) for value in states],
        "accepted_sequence": {
            "event_ids": [value["event_id"] for value in events],
            "full_replay_matches_incremental": packed(replayed) == packed(states[-1]),
            "identical_duplicate_event": {"outcome": "accepted_idempotently", "applied_count": len(first_duplicate_applied), "revision": first_duplicate_state["revision"]},
        },
        "refusals": [
            {"id": "stale-base-revision", **refusal(lambda: apply_command(state_one, stale_command))},
            {"id": "bool-is-not-an-integer-revision", **refusal(lambda: apply_command(states[0], bool_revision))},
            {"id": "unknown-event-kind", **refusal(lambda: apply_command(states[0], unknown_kind))},
            {"id": "extra-payload-field", **refusal(lambda: apply_command(states[0], extra_payload))},
            {"id": "observed-fact-without-evidence", **refusal(lambda: apply_command(states[0], missing_evidence))},
            {"id": "conflicting-duplicate-event", **refusal(lambda: replay_committed(initial_state(base, base_sha), [events[0], conflicting], seen={receipt["event_id"]: receipt}))},
            {"id": "journal-sequence-gap", **refusal(lambda: replay_committed(initial_state(base, base_sha), [gap], seen={receipt["event_id"]: receipt}))},
            {"id": "journal-command-hash-mismatch", **refusal(lambda: replay_committed(initial_state(base, base_sha), [bad_hash], seen={receipt["event_id"]: receipt}))},
        ],
        "not_covered": [
            "No external transport, native host, command execution, or authority activation was invoked.",
            "No change-economics or closure-P1 behavior is represented as desired Rust behavior.",
            "No zap/2 identity mapping is asserted before the Rust architecture freezes that rule.",
        ],
    }

    sources = {
        "zaplib/common.py": SCRIPTS / "zaplib" / "common.py",
        "zaplib/graph.py": SCRIPTS / "zaplib" / "graph.py",
        "zaplib/records.py": SCRIPTS / "zaplib" / "records.py",
        "zaplib/storage.py": SCRIPTS / "zaplib" / "storage.py",
        "zaplib/snapshots.py": SCRIPTS / "zaplib" / "snapshots.py",
    }
    metadata = {
        "schema": "zap-legacy-corpus-metadata/1",
        "python_version": platform.python_version(),
        "reference_sources": {name: sha(path.read_bytes()) for name, path in sources.items()},
        "generation": "python -B legacy-corpus/generate_legacy_corpus.py --check",
        "byte_encoding": "utf-8",
        "json_files": "zaplib.common.packed(value) followed by LF",
        "identity_warning": "These are legacy zap/1 byte identities; a corrected Rust epoch must not silently reuse them.",
    }

    return {
        "metadata.json": packed(metadata) + b"\n",
        "codec-cases.json": packed(codec_cases) + b"\n",
        "hash-domains.json": packed(line_domains) + b"\n",
        "tiny-campaign/plan.toml": plan_raw,
        "tiny-campaign/tasks/G.json": task_raw,
        "tiny-campaign/base.json": base_raw,
        "tiny-campaign/events.jsonl": events_raw,
        "tiny-campaign/snapshot.json": snapshot_raw,
        "tiny-campaign/expectations.json": packed(expectations) + b"\n",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    rendered = render()
    if args.manifest:
        print(json.dumps({"files": [{"path": path, "content": raw.decode("utf-8")} for path, raw in rendered.items()]}, separators=(",", ":"), sort_keys=True))
        return 0
    if args.check:
        mismatches = [path for path, raw in rendered.items() if not (HERE / path).is_file() or (HERE / path).read_bytes() != raw]
        if mismatches:
            print(json.dumps({"ok": False, "mismatches": mismatches}, separators=(",", ":"), sort_keys=True))
            return 1
        print(json.dumps({"ok": True, "files": sorted(rendered)}, separators=(",", ":"), sort_keys=True))
        return 0
    parser.error("choose --manifest or --check")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
