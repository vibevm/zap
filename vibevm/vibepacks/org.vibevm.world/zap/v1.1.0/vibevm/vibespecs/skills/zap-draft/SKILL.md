---
name: zap-draft
description: Draft or revise a ZAP campaign's intent, outcomes, owner charter, uncertainty map and lowering strategy without implicitly activating execution.
---

# ZAP drafting

Read [the methodology](../../flows/zap/ZAP-METHODOLOGY.xml) and
[the adaptive cycle](../../flows/zap/ZAP-ADAPTIVE-CYCLE.xml). For migration,
event schemas or viewer integration also read
[the data design](../../flows/zap/ZAP-DATA-AND-VIEWER.xml).

Drafts and detached Dreamer branches are durable typed data, not executable
plans. When the protected Rust application service is available, use its
read-only `prepare_effect_bundle`, `prepare_effect_comparison`, and
`prepare_projected_record` requests to derive exact bases, projected records,
and affected scopes. Preserve returned payloads and item digests unchanged
through any economics or Owner review. Preparation never consumes approval or
changes live state.

Start from the owner's current intent and existing decisions. Determine which
facts can be established from available sources before asking questions. Explain
material choices in plain language, with options, consequences and a recommendation.
Ask dependent questions after their prerequisites; keep useful independent work moving.

Separate the owner's intent, essential constraints, allowed tradeoffs, current
outcome hypothesis and working route. On meaningful observations reassess unknowns,
value, remaining cost and feasibility before selecting the next useful result.
Fog can grow; a previously accepted observation can become inapplicable without
a local code change. Change the target within the delegated outcome envelope
when a better or feasible approximation serves the intent. This need not wait
for failure or ask the owner again on every turn. Unknown feasibility is not
proof that the goal is impossible; transport failures are not architectural verdicts.

Preserve the provenance and explicit disposition of every obligation. Separate excluded scope from an
unexamined area, a precise question and an evidenced fact. Detail the near frontier;
keep distant work coarse without losing its acceptance obligations. Work purpose
(evidence, decision, change, verification, integration), maturity and authority
are independent dimensions. Do not infer them from an imported title.

When revising an existing strategy, use the registered strategic-map overview
and object queries to inspect its exact revision, current work, preserved
obligations and known gaps before creating more tasks. Read the
[map contract](../../flows/zap/ZAP-STRATEGIC-MAP.xml) and
[usage guide](../../flows/zap/ZAP-STRATEGIC-MAP-GUIDE.md).
For durable intermediate results read the
[milestone contract](../../flows/zap/ZAP-MILESTONES.xml),
[milestone guide](../../flows/zap/ZAP-MILESTONES-GUIDE.md) and
[planning guide](../../flows/zap/ZAP-MILESTONE-PLANNING-GUIDE.md).
Give each milestone an observable result, beneficiary or consequential decision,
required obligations and evidence boundary. Keep its stable identity through
task reshuffling. Distinguish a canonical milestone from an existing structural
landmark facet. Neither a visual group nor completed-child count proves a result.

Propose the milestone plan before materializing its work. Preserve full required
obligation coverage, choose the justified current focus and prerequisite frontier,
and record distant horizons. Adoption uses the normal privileged preparation and
admission path. For an adopted plan, submit an exact refinement record for the
intended lowering: every new Work needs its causal result/blocker/selected-research
binding, expected artifact, decision relevance, retained-work/evidence comparison
and retry lineage. The lowering kernel enforces these bindings. Replanning must
update the adopted plan; selecting another strategy does not turn the policy off.

Materialize a new action only when it advances a required result, removes a
blocker or resolves a selected information need. Compare retained work and
evidence first. Keep unselected alternatives and distant horizons explicit;
the mere existence of a question is not another runnable task. No fixed town
count or arbitrary graph-size target replaces obligation conservation.

Use the [information guide](../../flows/zap/ZAP-INFORMATION-GUIDE.md) for
opportunities that may change a decision. Discovery creates no executable Work.
Compare compatible benefit/cost ranges and cheaper decisive observations; retain
unknown judgments. Bind selected acquisition to ordinary research Work and its
stop conditions, reuse existing applicable evidence, and reconsider on relevant
source drift. An optional question is not a completion requirement.

Prepare typed split, merge, contribution-move, route-change or retirement through
the milestone transformation preview. Inspect identity/edge mappings, obligation
and consumer conservation, retained evidence, current proof applicability and
affected execution before admitting the exact change. Preserve historical
achievement receipts and distinguish them from current validity.

Use recursive lowering or prototype / functional MVP / productization only when
it resolves a concrete difficulty. Lowering maps the current outcome revision's
obligations to implementation and verification. A target revision separately records
what was retained, replaced or remains unmet, why, and under which delegated choice.
Close applicable formal deferrals; justify inapplicability for a discarded artifact
and verify its actual remaining effects. Reconcile live jobs and valid evidence
with the new plan. Preserve the guarantees required by each intermediate consumer.

Capture the autonomy contract: delegated decisions, exclusions, existing holds,
reserved actions, allowed outcome adjustments, stop predicates and what constitutes
original success, revised success or only partial benefit. Turn a complex
stop condition into typed inputs and an expression; distinguish a semantic judgment
from its deterministic evaluation. Unknown inputs require evidence. For the default
meaning of "stop", pause the whole campaign: no new jobs, drain current operations
to their safe boundary, preserve results and prepare the owner's requested choices.
Do not create extra methodological pauses for every task, stage or session.

Record consequential alternatives, concise rationale, sources, authority and affected
obligations as durable data. Preserve conflicting observations and superseded decisions.
Use the fact graph to identify affected work and checks; do not turn every new finding
into a full-suite run or mutation campaign. Reconsider the relevant region and missing
links; unchanged assumptions need no repeated full review. A new target does not
reset problem identity, failure counters or an owner pause. The runtime can
apply an adopted review, but a proposal remains data until the authenticated
application service admits the exact delegated action.

Deliver a reviewable draft and explicit unresolved choices. Existing permission to
design does not activate execution. The implemented owner channel activates only
an exact full charter bound to campaign/base, legacy classifications, intent
fingerprint and policy revision; never fabricate or infer its receipt. Import experiments belong in a
fresh local store, with the original plan and its execution holds preserved. Do not
start NEXT, replace its context, or publish personal state as a side effect of drafting.

Use `vibe bin exec zap -- capabilities` only as a truthful availability probe.
The binary does not call a native harness or model by itself. Live execution
belongs to the separately configured `zap-run` service and its cooperating
driver.
