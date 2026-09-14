# MS02/MS05 information and canonical map report

Status: implemented and focused-test green on 2026-09-14. This report records deterministic product evidence. No comparative agent pilot, productivity threshold or visual client was run or claimed.

## Delivered domain surface

The new `zap-domain::information` module persists `InformationOpportunityRecord` and `InformationSelectionRecord`. Registered data-proposal commands are `information.opportunity-proposed` and `information.selection-proposed`; registered query `zap.information.opportunities.v1` returns revision-bound recommendations. Discovery and selection insert no `WorkRecord`. Selected execution remains subject to the existing milestone-planning, lowering, economics, authority, contract and packet path.

An opportunity binds a logical `DecisionId` to a supported exact basis: Owner decision, strategic fork, region, outcome, or an explicit unresolved identity. It stores competing possibilities or an unknown condition, observation power, exact source capture/applicability records, regions and `BoundedHorizon` values, acquisition/verification/coordination/delay intervals, reused `CostUnknown` rows, avoided-agent-hour or avoided-elapsed-hour benefit ranges or explicit unknown, retained evidence and a structured stop rule.

Selection outcomes are `Worthwhile`, `NotWorthwhile`, `Unknown`, `CheaperDecisive`, `Dominated`, `AlreadySatisfied`, and `AlreadySelected`. The algorithm uses interval dominance in compatible hour units. It emits no scalar score or manufactured probability. Equal/overlapping ranges remain unknown unless one exact peer has a strict improvement. A reasoned proposal may retain an honest `Unknown` verdict when the canonical decision and source applicability are current; it still grants no execution.

Decision and acquisition indexes partition opportunities by `DecisionId` and acquisition fingerprint. The query meters index rows and exact opportunity loads: at least two operations are required, `limit <= operation_budget / 2`, and the requested decision partition must fit its half-budget for a total comparison. Unrelated opportunity partitions do not consume this budget. Each recommendation separately discloses exact decision/source/applicability/region decoding through `InformationBasisEvaluationCost`. Reused proof invokes the existing broader current-proof graph and reports `StoreWideCurrentProofContextScan`; that store-wide helper is not represented as bounded opportunity-query work.

`selected_information_execution_context` carries selection revision, Work identity/type, opportunity/basis/stop fingerprints, exact sources/evidence, full stop rule and deterministic safe-stop boundary. `ContractBoundary` is required to match `TaskContract.safe_stop`; existing candidate/packet derivation then preserves the same boundary. `DecisionGuidance` remains visibly non-enforced. Empty information-binding validation returns without store reads, and selected bindings load only their exact selection/opportunity/Work keys.

Strategic-fork bases hash exact semantic fork/strategy fields and a normalized superseded flag. Candidate-to-Current bookkeeping therefore preserves the information fingerprint; actual supersession or semantic/source changes make it stale.

## Canonical map and public integration

The strategic map now has canonical `Milestone`, `InformationOpportunity`, and `StrategicFork` object/semantic variants rather than projecting milestones as Work landmarks. Milestone cards load and validate the self-consistent head/current revision, preserve the full records as underlying detail, and expose obligations, outcome, consumers, Work/evidence/milestone contributions and typed preparation/achievement dependencies. Information cards link the actual decision-basis object, sources, regions, horizon subjects, retained evidence and selected Work. Unsupported decision bases and unrepresentable or missing linked objects are explicit gaps. A missing or mismatched linked selection is corrupt state, not an unselected opportunity.

Application evidence submits both information data commands through the protected agent-data channel, reads milestone/planning/information queries through the machine port, reads information through authenticated HTTP, and retrieves canonical milestone/information map cards. The compiled CLI test reads map, milestone, no-adopted-plan planning, and information query responses from the durable store. Existing `zap.map.route.v1` Work estimates and shared-Work accounting were unchanged.

## Focused evidence

All commands used the required shared-target offline wrapper:

- `run-cargo.ps1 -CargoArgs @('test','-p','zap-domain','--test','information','--offline')`: 3 passed, 0 failed. Covers persistence/reopen, exact retry, CAS, no Work creation, worthwhile/unknown/cheaper-decisive results, finite material unknown cost, 20 unrelated opportunities under an 8-operation relevant query, honest unknown selection, selected context and source drift.
- `run-cargo.ps1 -CargoArgs @('test','-p','zap-domain','--test','strategic_map','--offline')`: 5 passed, 0 failed before the canonical-object extension; a later `cargo check -p zap-domain --tests --offline` compiled the extension and split card modules.
- `run-cargo.ps1 -CargoArgs @('test','-p','zap-app','--test','strategic_map_service','map_objects::machine_service_exposes_canonical_milestone_and_information_objects','--offline')`: 1 passed, 0 failed. Covers protected opportunity/selection commands, machine milestone/planning/information reads, authenticated HTTP information read, and canonical cards.
- `run-cargo.ps1 -CargoArgs @('test','-p','zap-cli','--test','strategic_map','--offline')`: 1 passed, 0 failed. Covers compiled CLI map/milestone/planning/information reads.

## Review-finding dispositions

- Material finite or unbounded `CostUnknown` rows now conservatively produce `Unknown`; they are not omitted from a worthwhile result.
- A decisive observation must distinguish every declared possibility. Dominance compares the same exact decision basis and possibility meanings and requires a strict improvement, preventing mutual ties.
- Source/decision freshness is checked before evidence reuse. `AlreadySatisfied` uses the existing current-proof index and reports its store-wide cost.
- Per-decision and per-acquisition indexes replaced full-family query/dedup scans. Empty lowering bindings perform no read, so optional information cannot block ordinary work by cardinality.
- Selection may retain a current, supported `Unknown` verdict with rationale, avoiding pressure to invent benefit numbers. Stale, dominated, already-satisfied and unsupported opportunities remain unselectable.
- Candidate-to-Current strategy promotion is normalized out of strategic-fork basis state; Superseded remains material.
- Information cards no longer invent a canonical `Decision` record from a logical label, include horizon semantics/gaps, and refuse missing or foreign selection links.
- Milestone cards validate the same self-consistency predicate as canonical milestone reads.

Production and test Rust files added or expanded for this slice remain below 600 lines after splitting `strategic_map/cards/canonical.rs` from `cards.rs` and splitting information scenarios from `tests/information.rs`.

## Independent transform review

A bounded read-only review covered `milestones/transform_validation.rs`, `milestones/transform_query.rs`, and `tests/milestones_transforms.rs`. Preview and apply share `prepare_transform`; the preview digest binds the typed plan and exact before/after revision identities, the applied record is queryable by operation identity, removed dependency dispositions are exact and typed, and the preview truthfully reports its store-wide dependent scan.

One concrete defect was reported without editing milestone code: auxiliary rows use `same_non_route`, which normalizes both contributions and dependencies. That can admit an unrelated Work/Evidence contribution change inside Split/Merge/Move/Retire as an auxiliary route rewrite, even though the operation kind did not declare that contribution move. The correction sent to the milestone owner is to preserve Work/Evidence contributions exactly, allow only legitimate Milestone-reference rebasing, and require an auxiliary row to depend on a changed row; a negative globally-conserved but undeclared contribution-relocation test should cover it.

The latest tested source boundary after canonical decision-basis and card-splitting fixes is the focused app command `run-cargo.ps1 -CargoArgs @('test','-p','zap-app','--test','strategic_map_service','map_objects::machine_service_exposes_canonical_milestone_and_information_objects','--offline')`: 1 passed, 0 failed. The remaining shared formatting, conformance, regression and package-install gates are owned by the root session.

## Final lint and documentation repair boundary

The final source repair removed panic-based recommendation construction by propagating `BoundedText` errors, removed test `unwrap` calls, and boxed the large milestone/information underlying-detail records without changing their serde field shape. Proof-index preparation is now limited to current, applicable evidence-bearing opportunities on the emitted page, so a later-page peer cannot cause an undisclosed store-wide proof scan.

After those repairs the affected focused suites were rerun through the required wrapper: information 3/3, strategic map 5/5, application machine/authenticated-HTTP journey 1/1, and compiled CLI journey 1/1. `cargo check -p zap-domain --lib --offline` also passed.

`ZAP-INFORMATION-GUIDE.md` is now an SPMD guide with explicit `{#root}` and section IDs plus `guide r1` markers. Public information records, payloads, query types, execution context, lowering binding, basis/freshness functions, cells, and registry functions carry meaningful `#[spec(documents)]` edges to those real units. The root session owns the subsequent shared specmap regeneration and conformance gate.
