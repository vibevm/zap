"""Public composition seam for the ZAP adaptive domain reducers."""
from __future__ import annotations

from types import MappingProxyType

from .domain_adaptive import DOMAIN_ADAPTIVE_HANDLERS, build_sparse_review_transition
from .domain_deferrals import DOMAIN_DEFERRAL_HANDLERS
from .domain_graph import DOMAIN_GRAPH_HANDLERS, intent_fingerprint
from .domain_model import DOMAIN_SCHEMA, domain_frontier, domain_state
from .domain_proof import DOMAIN_PROOF_HANDLERS
from .domain_reuse import current_acceptance_coverage
from .domain_schemas import DOMAIN_EVENT_SCHEMAS, SPARSE_REVIEW_TRANSITION_SCHEMA
from .domain_work import DOMAIN_WORK_HANDLERS


def _registry():
    result = {}
    for source in (DOMAIN_GRAPH_HANDLERS, DOMAIN_WORK_HANDLERS, DOMAIN_DEFERRAL_HANDLERS,
                   DOMAIN_PROOF_HANDLERS, DOMAIN_ADAPTIVE_HANDLERS):
        overlap = set(result) & set(source)
        if overlap:
            raise RuntimeError(f"duplicate domain handler kinds: {sorted(overlap)}")
        result.update(source)
    return MappingProxyType(result)


DOMAIN_HANDLERS = _registry()
DOMAIN_COMMANDS = ()
DOMAIN_OPERATIONS = MappingProxyType({
    "domain.materialize-review-transition": MappingProxyType({
        "callable": "build_sparse_review_transition", "input_schema": SPARSE_REVIEW_TRANSITION_SCHEMA,
        "returns": "domain.review-proposed.transition",
    }),
})
DOMAIN_DATA_KINDS = frozenset({
    "domain.intent-proposed", "domain.outcome-proposed", "domain.review-proposed",
})
DOMAIN_ACTION_KINDS = MappingProxyType({
    "domain.intent-adopted": "outcome.adopt",
    "domain.outcome-adopted": "outcome.adopt",
    "domain.task-contract-replaced": "task.update",
    "domain.work-renamed": "plan.lower",
    "domain.work-transitioned": "plan.lower",
    "domain.work-dispatched": "work.dispatch",
    "domain.plan-lowered": "plan.lower",
    "domain.deferral-created": "task.update",
    "domain.deferral-transferred": "task.update",
    "domain.deferral-closed": "task.update",
    "domain.deferral-inapplicable": "task.update",
    "domain.evidence-adjudicated": "evidence.adjudicate",
    "domain.stage-accepted": "stage.accept",
    "domain.integration-accepted": "work.accept",
    "domain.work-accepted": "work.accept",
    "domain.fact-promotion-recorded": "fact.promote",
    "domain.review-applied": "adaptive.apply",
    "domain.campaign-closed": "campaign.close",
})
if DOMAIN_DATA_KINDS & set(DOMAIN_ACTION_KINDS) or DOMAIN_DATA_KINDS | set(DOMAIN_ACTION_KINDS) != set(DOMAIN_HANDLERS):
    raise RuntimeError("every domain handler must have exactly one service authorization class")
DOMAIN_CAPABILITIES = MappingProxyType({
    "schema": DOMAIN_SCHEMA,
    "version": 1,
    "events": tuple(sorted(DOMAIN_HANDLERS)),
    "operations": tuple(sorted(DOMAIN_OPERATIONS)),
    "privileged_actions": (
        "outcome.adopt", "adaptive.apply", "task.update", "evidence.adjudicate",
        "work.accept", "stage.accept", "fact.promote", "campaign.close",
        "work.dispatch", "verification.run", "plan.lower",
    ),
})

__all__ = (
    "DOMAIN_ACTION_KINDS", "DOMAIN_CAPABILITIES", "DOMAIN_COMMANDS", "DOMAIN_DATA_KINDS",
    "DOMAIN_EVENT_SCHEMAS", "DOMAIN_HANDLERS", "DOMAIN_OPERATIONS", "DOMAIN_SCHEMA",
    "SPARSE_REVIEW_TRANSITION_SCHEMA", "build_sparse_review_transition",
    "current_acceptance_coverage", "domain_frontier", "domain_state", "intent_fingerprint",
)
