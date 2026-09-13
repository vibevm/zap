"""Selective, provenance-preserving reuse across outcome revisions."""
from __future__ import annotations

from typing import Any

from .common import Refusal, need
from .domain_model import content_hash, domain_state, effective_nodes, inherited_dependencies, owned_obligations


COLLECTIONS = {
    "evidence": "evidence_adjudications",
    "stages": "stages",
    "acceptances": "acceptances",
    "integrations": "integration_acceptances",
}


def _witnesses(domain: dict[str, Any], kind: str, source_id: str) -> list[dict[str, Any]]:
    return domain["reuse_witnesses"][kind].get(source_id, [])


def _source(domain: dict[str, Any], kind: str, source_id: str, outcome_id: str):
    record = domain[COLLECTIONS[kind]].get(source_id)
    need(record is not None, "REFERENCE", f"preserved {kind} record missing: {source_id}")
    versions = list(record.get("history", [])) + [record] if kind == "evidence" else [record]
    direct = next((row for row in reversed(versions) if row["applies_to"]["outcome_id"] == outcome_id), None) \
        if kind == "evidence" else (record if record["outcome_id"] == outcome_id else None)
    if direct is not None:
        return direct, {"kind": "record", "event_id": direct["event_id"], "revision": direct.get("revision")}
    witness = next((row for row in reversed(_witnesses(domain, kind, source_id))
                    if row["to_outcome_id"] == outcome_id), None)
    need(witness is not None, "DOMAIN_EVIDENCE", f"{kind} does not apply to outcome {outcome_id}")
    if kind == "evidence":
        record = next((row for row in versions
                       if row["revision"] == witness["adjudication_revision"]
                       and row["event_id"] == witness["adjudication_event_id"]), None)
        need(record is not None, "DOMAIN_EVIDENCE", "reuse witness adjudication history is missing")
    return record, {"kind": "reuse_witness", "event_id": witness["event_id"], "revision": witness["revision"]}


def _witness(domain, kind, source_id, pointer):
    if pointer["kind"] == "record":
        return None
    row = next((item for item in _witnesses(domain, kind, source_id)
                if item["event_id"] == pointer["event_id"] and item["revision"] == pointer["revision"]), None)
    need(row is not None, "DOMAIN_EVIDENCE", "reuse witness history is missing")
    return row


def _obligation_fingerprint(row):
    return content_hash({key: row[key] for key in (
        "id", "kind", "statement", "essential_declared", "source",
    )})


def _ownership_fingerprint(domain, work_id):
    rows = sorted(
        ({"obligation_id": row["obligation_id"], "role": row["role"]}
         for row in domain["ownership"] if row["work_id"] == work_id
         and domain["obligations"][row["obligation_id"]]["status"] == "active"),
        key=lambda row: (row["obligation_id"], row["role"]),
    )
    return content_hash(rows)


def _active_contract(domain: dict[str, Any], work_id: str):
    history = domain["task_contracts"].get(work_id)
    need(history is not None, "DOMAIN_CONTRACT", "preserved work has no task contract")
    record = next((row for row in history["versions"] if row["version"] == history["active_version"]), None)
    need(record is not None and record["schema"] == "zap-task-contract/1",
         "DOMAIN_CONTRACT", "preserved work contract is absent or legacy")
    return history["active_version"], record


def _work_unchanged(state: dict[str, Any], domain: dict[str, Any], work_id: str) -> None:
    node = effective_nodes(state, domain).get(work_id)
    need(node is not None and node["state"] not in {"dropped", "superseded"},
         "DOMAIN_EVIDENCE", "preserved proof work subject changed or disappeared")
    need(not domain["work_updates"].get(work_id, {}).get("revalidation_required"),
         "DOMAIN_EVIDENCE", "preserved proof work requires revalidation")


def _sources_current(state: dict[str, Any], source_refs: list[str]) -> None:
    from .sources import current_applicability
    result = current_applicability(state, source_refs)
    need(result["status"] == "applicable" and not result["incomplete_closure"],
         "DOMAIN_EVIDENCE", "preserved proof source is stale, unknown or incomplete")


def _captures_current(state: dict[str, Any], captures: list[dict[str, str]]) -> None:
    from .sources import compare_source_captures
    result = compare_source_captures(state, captures)
    need(result["status"] == "current", "DOMAIN_EVIDENCE", "preserved proof source content changed")


def _evidence_captures(state, row):
    captures = row.get("source_captures_at_adjudication")
    need(isinstance(captures, list) and {item.get("source_id") for item in captures} == set(row["source_refs"]),
         "DOMAIN_EVIDENCE", "preserved evidence lacks source identity at adjudication")
    _captures_current(state, captures)
    return captures


def evidence_for_outcome(state, domain, evidence_id, outcome_id):
    row, pointer = _source(domain, "evidence", evidence_id, outcome_id)
    need(row["disposition"] == "accepted", "DOMAIN_EVIDENCE", "preserved evidence is not accepted")
    _sources_current(state, row["source_refs"])
    captures = _evidence_captures(state, row)
    nodes = effective_nodes(state, domain)
    for work_id in row["applies_to"]["work_ids"]:
        need(work_id in nodes, "DOMAIN_EVIDENCE", "preserved evidence work subject disappeared")
        _work_unchanged(state, domain, work_id)
    for obligation_id in row["applies_to"]["obligation_ids"]:
        obligation = domain["obligations"].get(obligation_id)
        need(obligation and obligation["status"] == "active" and outcome_id in obligation["current_outcome_ids"],
             "DOMAIN_EVIDENCE", "preserved evidence obligation changed or was not retained")
    witness = _witness(domain, "evidence", evidence_id, pointer)
    if witness is not None:
        need(witness["adjudication_revision"] == row["revision"]
             and witness["adjudication_event_id"] == row["event_id"]
             and witness["core_evidence_sha256"] == content_hash(state["evidence"][evidence_id])
             and witness["work_ids"] == row["applies_to"]["work_ids"]
             and witness["source_captures"] == captures,
             "DOMAIN_EVIDENCE", "preserved evidence witness no longer matches its proof")
        need(witness["obligation_fingerprints"] == {
            key: _obligation_fingerprint(domain["obligations"][key])
            for key in row["applies_to"]["obligation_ids"]
        }, "DOMAIN_EVIDENCE", "preserved evidence obligation semantics changed")
    return row, pointer


def stage_for_outcome(state, domain, stage_id, outcome_id):
    row, pointer = _source(domain, "stages", stage_id, outcome_id)
    _work_unchanged(state, domain, row["work_id"])
    need(set(row["obligation_ids"]) <= owned_obligations(domain, row["work_id"]),
         "DOMAIN_EVIDENCE", "preserved stage obligations changed")
    for evidence_id in row["evidence_ids"]:
        evidence_for_outcome(state, domain, evidence_id, outcome_id)
    witness = _witness(domain, "stages", stage_id, pointer)
    if witness is not None:
        version, contract = _active_contract(domain, row["work_id"])
        need(witness["work_id"] == row["work_id"] and witness["obligation_ids"] == row["obligation_ids"]
             and witness["evidence_ids"] == row["evidence_ids"] and witness["contract_version"] == version
             and witness["contract_sha256"] == contract["sha256"]
             and witness["ownership_sha256"] == _ownership_fingerprint(domain, row["work_id"]),
             "DOMAIN_EVIDENCE", "preserved stage witness no longer matches its contract")
    return row, pointer


def integration_for_outcome(state, domain, integration_id, outcome_id, trail=None):
    row, pointer = _source(domain, "integrations", integration_id, outcome_id)
    _work_unchanged(state, domain, row["work_id"])
    need(set(row["obligation_ids"]) <= owned_obligations(domain, row["work_id"]),
         "DOMAIN_ACCEPTANCE", "preserved integration obligations changed")
    for evidence_id in row["evidence_ids"]:
        evidence_for_outcome(state, domain, evidence_id, outcome_id)
    for child in set(row["child_work_ids"]) - set(row["legacy_child_ids"]):
        need(work_acceptance_current(state, domain, child, outcome_id, trail, allow_legacy=False),
             "DOMAIN_ACCEPTANCE", "preserved integration child is not currently accepted")
    witness = _witness(domain, "integrations", integration_id, pointer)
    if witness is not None:
        need(witness["work_id"] == row["work_id"] and witness["child_work_ids"] == row["child_work_ids"]
             and witness["obligation_ids"] == row["obligation_ids"]
             and witness["evidence_ids"] == row["evidence_ids"]
             and witness["legacy_child_ids"] == row["legacy_child_ids"]
             and witness["ownership_sha256"] == _ownership_fingerprint(domain, row["work_id"]),
             "DOMAIN_ACCEPTANCE", "preserved integration witness no longer matches its subjects")
    return row, pointer


def acceptance_for_outcome(state, domain, acceptance_id, outcome_id, trail=None):
    row, pointer = _source(domain, "acceptances", acceptance_id, outcome_id)
    work_id = row["work_id"]
    _work_unchanged(state, domain, work_id)
    version, contract = _active_contract(domain, work_id)
    need(version == row["contract_version"], "DOMAIN_CONTRACT", "preserved work contract version changed")
    need(set(row["obligation_ids"]) == owned_obligations(domain, work_id),
         "DOMAIN_ACCEPTANCE", "preserved work obligation ownership changed")
    need(set(contract["contract"]["obligation_ids"]) == set(row["obligation_ids"]),
         "DOMAIN_CONTRACT", "preserved contract obligations changed")
    stage_for_outcome(state, domain, row["stage_acceptance_id"], outcome_id)
    for evidence_id in row["evidence_ids"]:
        evidence, _ = evidence_for_outcome(state, domain, evidence_id, outcome_id)
        need(state["evidence"][evidence_id]["result"] == "observed_pass",
             "DOMAIN_EVIDENCE", "preserved acceptance evidence is not an observed pass")
    for integration_id in row["integration_acceptance_ids"]:
        integration_for_outcome(state, domain, integration_id, outcome_id, trail)
    witness = _witness(domain, "acceptances", acceptance_id, pointer)
    if witness is not None:
        need(witness["work_id"] == work_id and witness["obligation_ids"] == row["obligation_ids"]
             and witness["evidence_ids"] == row["evidence_ids"]
             and witness["stage_acceptance_id"] == row["stage_acceptance_id"]
             and witness["integration_acceptance_ids"] == row["integration_acceptance_ids"]
             and witness["contract_version"] == version and witness["contract_sha256"] == contract["sha256"]
             and witness["ownership_sha256"] == _ownership_fingerprint(domain, work_id),
             "DOMAIN_ACCEPTANCE", "preserved work witness no longer matches its accepted contract")
    return row, pointer


def work_acceptance_current(state, domain, work_id, outcome_id, trail=None, *, allow_legacy=True) -> bool:
    trail = set() if trail is None else set(trail)
    if work_id in trail:
        return False
    trail.add(work_id)
    nodes = effective_nodes(state, domain)
    if work_id not in nodes:
        return False
    if allow_legacy and nodes[work_id]["state"] == "accepted" and work_id in domain["legacy_acceptance"]:
        return True
    for acceptance_id, row in domain["acceptances"].items():
        if row["work_id"] != work_id:
            continue
        try:
            acceptance_for_outcome(state, domain, acceptance_id, outcome_id, trail)
            if all(work_acceptance_current(state, domain, dep, outcome_id, trail)
                   for dep in inherited_dependencies(nodes, work_id)):
                return True
        except Refusal:
            continue
    return False


def current_acceptance_coverage(state: dict[str, Any]) -> dict[str, Any]:
    """Return only centrally accepted work/integration proof valid for the active outcome."""
    domain = domain_state(state)
    outcome_id = domain["active_outcome_id"]
    acceptance_ids = []
    work_ids = set()
    obligation_ids = set()
    integration_ids = []
    integration_obligations = {}
    if outcome_id is not None:
        for acceptance_id, row in sorted(domain["acceptances"].items()):
            try:
                acceptance_for_outcome(state, domain, acceptance_id, outcome_id)
            except Refusal:
                continue
            acceptance_ids.append(acceptance_id)
            work_ids.add(row["work_id"])
            obligation_ids.update(row["obligation_ids"])
        for integration_id, row in sorted(domain["integration_acceptances"].items()):
            try:
                integration_for_outcome(state, domain, integration_id, outcome_id)
            except Refusal:
                continue
            integration_ids.append(integration_id)
            integration_obligations[integration_id] = sorted(row["obligation_ids"])
    return {
        "schema": "zap-domain/acceptance-coverage/1", "outcome_id": outcome_id,
        "work_ids": sorted(work_ids), "obligation_ids": sorted(obligation_ids),
        "acceptance_ids": acceptance_ids, "integration_acceptance_ids": integration_ids,
        "integration_obligations": integration_obligations,
    }


def _capture_map(review: dict[str, Any]) -> dict[str, str]:
    return {row["source_id"]: row["sha256"] for row in review["captures"]["source_captures"]}


def _selection(transition):
    return (
        set(transition["preserved_evidence_ids"]),
        set(transition["preserved_stage_acceptance_ids"]),
        set(transition["preserved_work_acceptance_ids"]),
        set(transition["preserved_integration_acceptance_ids"]),
    )


def validate_preserved_candidates(state, domain, transition, from_outcome, to_outcome):
    """Refuse a sparse/full reuse selection that cannot remain true after the proposed pivot."""
    evidence_ids, stage_ids, acceptance_ids, integration_ids = _selection(transition)
    if not any((evidence_ids, stage_ids, acceptance_ids, integration_ids)):
        return
    old = domain["outcome_revisions"].get(from_outcome)
    new = domain["outcome_revisions"].get(to_outcome)
    need(old is not None and new is not None and set(old["guarantees"]) == set(new["guarantees"]),
         "DOMAIN_EVIDENCE", "changed outcome guarantee requires revalidation")
    dispositions = {row["obligation_id"]: row["disposition"] for row in transition["obligation_dispositions"]}
    selected_evidence = {key: _source(domain, "evidence", key, from_outcome)[0] for key in sorted(evidence_ids)}
    selected_stages = {key: _source(domain, "stages", key, from_outcome)[0] for key in sorted(stage_ids)}
    selected_acceptances = {key: _source(domain, "acceptances", key, from_outcome)[0]
                            for key in sorted(acceptance_ids)}
    selected_integrations = {key: _source(domain, "integrations", key, from_outcome)[0]
                             for key in sorted(integration_ids)}
    selected_work = {row["work_id"] for row in selected_acceptances.values()}

    for key, row in selected_evidence.items():
        evidence_for_outcome(state, domain, key, from_outcome)
        need(all(dispositions.get(obligation_id) == "retained"
                 for obligation_id in row["applies_to"]["obligation_ids"]),
             "DOMAIN_EVIDENCE", "preserved evidence obligation was changed by the pivot")
        need(any(key in item["evidence_ids"] for item in selected_stages.values())
             or any(key in item["evidence_ids"] for item in selected_acceptances.values())
             or any(key in item["evidence_ids"] for item in selected_integrations.values()),
             "DOMAIN_EVIDENCE", "preserved evidence lacks an explicitly preserved acceptance context")
    for key, row in selected_stages.items():
        stage_for_outcome(state, domain, key, from_outcome)
        need(set(row["evidence_ids"]) <= evidence_ids,
             "DOMAIN_EVIDENCE", "preserved stage requires explicitly preserved evidence")
        need(any(item["stage_acceptance_id"] == key for item in selected_acceptances.values()),
             "DOMAIN_ACCEPTANCE", "preserved stage lacks an explicitly preserved work acceptance")
    for key, row in selected_integrations.items():
        integration_for_outcome(state, domain, key, from_outcome)
        need(set(row["evidence_ids"]) <= evidence_ids,
             "DOMAIN_EVIDENCE", "preserved integration requires explicitly preserved evidence")
        need(set(row["child_work_ids"]) - set(row["legacy_child_ids"]) <= selected_work,
             "DOMAIN_ACCEPTANCE", "preserved integration requires explicit child acceptance reuse")
        need(any(item["work_id"] == row["work_id"] and key in item["integration_acceptance_ids"]
                 for item in selected_acceptances.values()),
             "DOMAIN_ACCEPTANCE", "preserved integration lacks its parent work acceptance")
    nodes = effective_nodes(state, domain)
    for key, row in selected_acceptances.items():
        acceptance_for_outcome(state, domain, key, from_outcome)
        need(set(row["evidence_ids"]) <= evidence_ids and row["stage_acceptance_id"] in stage_ids
             and set(row["integration_acceptance_ids"]) <= integration_ids,
             "DOMAIN_ACCEPTANCE", "preserved work acceptance has unselected proof")
        need(all(dispositions.get(obligation_id) == "retained" for obligation_id in row["obligation_ids"]),
             "DOMAIN_ACCEPTANCE", "preserved work obligation was changed by the pivot")
        dependencies = set(inherited_dependencies(nodes, row["work_id"]))
        need(all(dep in selected_work or dep in domain["legacy_acceptance"] for dep in dependencies),
             "DOMAIN_ACCEPTANCE", "preserved work dependency lacks explicit current reuse")
        need(not any(owner["work_id"] == row["work_id"] for obligation in new["obligations"]
                     for owner in obligation["owners"]),
             "DOMAIN_ACCEPTANCE", "new work obligation is not covered by the preserved acceptance")

    proof_work = {work_id for row in selected_evidence.values() for work_id in row["applies_to"]["work_ids"]}
    proof_work |= selected_work | {row["work_id"] for row in selected_integrations.values()}
    for change in transition["ownership_changes"]:
        affected = {change["from_work_id"]} | {row["work_id"] for row in change["assignments"]}
        need(not affected & proof_work, "DOMAIN_ACCEPTANCE", "preserved proof ownership changed")
    for change in transition["work_changes"]:
        need(change["work_id"] not in proof_work or change["operation"] == "reprioritize",
             "DOMAIN_ACCEPTANCE", "preserved proof work changed or requires revalidation")


def _append_witness(domain, kind, source_id, from_outcome, to_outcome, review, event_id, **details):
    rows = domain["reuse_witnesses"][kind].setdefault(source_id, [])
    record, source_pointer = _source(domain, kind, source_id, from_outcome)
    witness = {
        "schema": f"zap-domain/{kind}-reuse/1", "revision": len(rows) + 1,
        "source_id": source_id, "from_outcome_id": from_outcome, "to_outcome_id": to_outcome,
        "source": source_pointer, "review_id": review["review_id"], "event_id": event_id,
        **details,
    }
    rows.append(witness)
    return record, witness


def apply_preserved_reuse(state, domain, review, event_id, from_outcome, to_outcome):
    transition = review["transition"]
    evidence_ids, stage_ids, acceptance_ids, integration_ids = _selection(transition)
    captures = _capture_map(review)
    selected_acceptances = {key: _source(domain, "acceptances", key, from_outcome)[0]
                            for key in sorted(acceptance_ids)}

    for evidence_id in sorted(evidence_ids):
        row, _ = evidence_for_outcome(state, domain, evidence_id, from_outcome)
        need(set(row["source_refs"]) <= set(captures),
             "DOMAIN_EVIDENCE", "preserved evidence sources were not captured by the review")
        source_captures = row["source_captures_at_adjudication"]
        need(source_captures == [{"source_id": key, "sha256": captures[key]} for key in row["source_refs"]],
             "DOMAIN_EVIDENCE", "review source capture differs from the accepted proof input")
        details = {
            "obligation_fingerprints": {key: _obligation_fingerprint(domain["obligations"][key])
                                        for key in row["applies_to"]["obligation_ids"]},
            "work_ids": list(row["applies_to"]["work_ids"]),
            "source_captures": list(source_captures),
            "adjudication_revision": row["revision"], "adjudication_event_id": row["event_id"],
            "core_evidence_sha256": content_hash(state["evidence"][evidence_id]),
        }
        _append_witness(domain, "evidence", evidence_id, from_outcome, to_outcome, review, event_id, **details)

    for stage_id in sorted(stage_ids):
        row, _ = stage_for_outcome(state, domain, stage_id, from_outcome)
        need(set(row["evidence_ids"]) <= evidence_ids,
             "DOMAIN_EVIDENCE", "preserved stage requires explicitly preserved evidence")
        owners = [item for item in selected_acceptances.values() if item["stage_acceptance_id"] == stage_id]
        need(owners, "DOMAIN_CONTRACT", "preserved stage lacks an explicitly preserved work acceptance")
        version, contract = _active_contract(domain, row["work_id"])
        need(all(item["contract_version"] == version for item in owners),
             "DOMAIN_CONTRACT", "preserved stage contract changed")
        _append_witness(domain, "stages", stage_id, from_outcome, to_outcome, review, event_id,
                        work_id=row["work_id"], obligation_ids=list(row["obligation_ids"]),
                        evidence_ids=list(row["evidence_ids"]), contract_version=version,
                        contract_sha256=contract["sha256"], ownership_sha256=_ownership_fingerprint(domain, row["work_id"]))

    selected_work = {row["work_id"] for row in selected_acceptances.values()}
    for integration_id in sorted(integration_ids):
        row, _ = integration_for_outcome(state, domain, integration_id, from_outcome)
        need(set(row["evidence_ids"]) <= evidence_ids,
             "DOMAIN_EVIDENCE", "preserved integration requires explicitly preserved evidence")
        need(set(row["child_work_ids"]) - set(row["legacy_child_ids"]) <= selected_work,
             "DOMAIN_ACCEPTANCE", "preserved integration requires explicit child acceptance reuse")
        _append_witness(domain, "integrations", integration_id, from_outcome, to_outcome, review, event_id,
                        work_id=row["work_id"], child_work_ids=list(row["child_work_ids"]),
                        obligation_ids=list(row["obligation_ids"]), evidence_ids=list(row["evidence_ids"]),
                        legacy_child_ids=list(row["legacy_child_ids"]),
                        ownership_sha256=_ownership_fingerprint(domain, row["work_id"]))

    nodes = effective_nodes(state, domain)
    for acceptance_id, row in selected_acceptances.items():
        need(set(row["evidence_ids"]) <= evidence_ids and row["stage_acceptance_id"] in stage_ids,
             "DOMAIN_ACCEPTANCE", "preserved work acceptance has unselected proof")
        need(set(row["integration_acceptance_ids"]) <= integration_ids,
             "DOMAIN_ACCEPTANCE", "preserved work acceptance has unselected integration proof")
        dependencies = inherited_dependencies(nodes, row["work_id"])
        need(all(dep in selected_work or work_acceptance_current(state, domain, dep, to_outcome)
                 for dep in dependencies),
             "DOMAIN_ACCEPTANCE", "preserved work dependency is not accepted for the new outcome")
        version, contract = _active_contract(domain, row["work_id"])
        need(version == row["contract_version"], "DOMAIN_CONTRACT", "preserved work contract changed")
        _append_witness(domain, "acceptances", acceptance_id, from_outcome, to_outcome, review, event_id,
                        work_id=row["work_id"], obligation_ids=list(row["obligation_ids"]),
                        evidence_ids=list(row["evidence_ids"]), stage_acceptance_id=row["stage_acceptance_id"],
                        integration_acceptance_ids=list(row["integration_acceptance_ids"]),
                        contract_version=version, contract_sha256=contract["sha256"],
                        ownership_sha256=_ownership_fingerprint(domain, row["work_id"]))

    for evidence_id in sorted(evidence_ids):
        evidence_for_outcome(state, domain, evidence_id, to_outcome)
    for stage_id in sorted(stage_ids):
        stage_for_outcome(state, domain, stage_id, to_outcome)
    for integration_id in sorted(integration_ids):
        integration_for_outcome(state, domain, integration_id, to_outcome)
    for acceptance_id in sorted(acceptance_ids):
        acceptance_for_outcome(state, domain, acceptance_id, to_outcome)


def validate_preserved_current(state, domain, review):
    transition = review["transition"]
    outcome = domain["active_outcome_id"]
    captures = _capture_map(review)
    for evidence_id in transition["preserved_evidence_ids"]:
        row, _ = evidence_for_outcome(state, domain, evidence_id, outcome)
        need(set(row["source_refs"]) <= set(captures), "DOMAIN_EVIDENCE", "preserved evidence sources were not captured")
    for stage_id in transition["preserved_stage_acceptance_ids"]:
        stage_for_outcome(state, domain, stage_id, outcome)
    for integration_id in transition["preserved_integration_acceptance_ids"]:
        integration_for_outcome(state, domain, integration_id, outcome)
    for acceptance_id in transition["preserved_work_acceptance_ids"]:
        acceptance_for_outcome(state, domain, acceptance_id, outcome)


__all__ = (
    "acceptance_for_outcome", "apply_preserved_reuse", "evidence_for_outcome",
    "current_acceptance_coverage", "integration_for_outcome", "stage_for_outcome", "validate_preserved_current",
    "validate_preserved_candidates", "work_acceptance_current",
)
