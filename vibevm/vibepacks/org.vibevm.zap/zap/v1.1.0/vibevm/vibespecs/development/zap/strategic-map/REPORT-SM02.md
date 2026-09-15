# SM02 bounded strategic-map projection report

Status: **candidate complete and source-frozen for root review**. The implementation is
confined to ZAP 1.1.0. It adds read-only domain queries and public projection types; it
does not alter released 1.0.0, create a canonical milestone, mutate a plan, authorize a
route, replace runtime frontier selection, or add a frontend/model call.

## Registered surface

The existing domain `QuerySet` now registers:

| Query ID | Input | Result |
| --- | --- | --- |
| `zap.map.overview.v1` | `MapOverviewInput` | `MapOverviewResult` |
| `zap.map.object.v1` | `MapObjectInput` | `MapObjectResult` |
| `zap.map.route.v1` | `MapRouteInput` | `MapRouteResult` |

`MapObjectRef` separates existing `ViewerNodeId`, exact `StrategicRevisionId`, and
reference-only `ResourceId`. Existing viewer wire types are reused unchanged. The new
module has 29 public declarations, each mapped to an anchored `guide r1` section and
listed in the public type catalog.

## Source and bounds

Every operation exact-reads the caller-selected `StrategicPlanRecord`; no global current
strategy is inferred. The source is checked for its existing sorted membership,
dependency closure, DAG, obligation coverage, and semantic digest invariants while
preserving Candidate/Current/Superseded state as data.

Overview cursors bind store, base, query epoch, snapshot revision, strategy ID, strategy
record revision, semantic digest, normalized filter, and last examined Work. Filtering
advances by examined membership, so an empty landmark page can continue. Missing live
Work is reported separately and is not classified as a landmark. All known non-Atom
`WorkKind` values retain explicit facets: Portfolio, Campaign, Phase, Workstream, Group,
Gate, and Horizon.

The result reports source-plan node count and canonical encoded bytes separately from
examined membership, emitted cards, emitted relationships, and observed index rows.
Those are independently bounded components, not a fabricated total read or CPU count.
The whole existing source record must still be decoded. Complete object relationships
or route closures that exceed their component budget refuse with `LimitExceeded`.

## Cards and relationships

Materialized Work cards reuse one aggregate-bounded source loader for the exact Work and
the existing last-active-in-ContractId-order contract policy. They expose the resulting
`assessment_source_fingerprint` even when no assessment exists, so a generic client can
prepare the first descriptive proposal. Strategic-only Work retains title,
dependencies, obligations, and refinement trigger with no invented live state.

Canonical Work title, `WorkKind`, `WorkType`, record state, contract goal, acceptance
text, evidence/source references, descriptive assessment, and assessment freshness
remain distinct. `MapBlockerView::NotEvaluated` prevents empty blockers from claiming
readiness or successful hold/pause/capacity/proof evaluation. Assessment absence and
staleness do not change graph relationship completeness.

Relationships retain source identity and distinguish containment, Work prerequisites,
contract ownership, obligation roles, acceptance duty, knowledge relation, evidence,
source, region, resource, read-subject, and write-subject meaning. Stable relationship
digests include endpoints, kind, role, and source. Derived index rows are rechecked
against the current partition predicate and record. Conflicting equal-ID knowledge rows,
wrong endpoint directions, stale child/dependent membership, or wrong source/fact/
region/evidence membership refuse as corrupt index state.

Fact subject edges use `AppliesTo` in both forward and reverse cards. Region
`subject_refs` use `AppliesTo` while `work_refs` use `Affects`; when both fields name the
same Work, both stable relationship IDs remain visible from either card direction.
Valid Fact statements up to their existing 16 KiB bound remain complete card
descriptions and underlying detail. Their canonical display name uses the short stable
Fact identity, so a valid long Unicode statement is never rejected or truncated merely
to fit a 4 KiB name.

Known gaps remain explicit: project-scoped source expansion, unsupported `SubjectRef`
kinds, missing referenced records, relation families not projected by the current card,
and the absent resource reverse index. Resource cards do not invent capacity,
availability, occupancy, or descriptions.

## Routes and descriptive estimates

Route input must contain a nonempty strictly ordered set of distinct Work IDs from the
source plan. The query validates and expands the complete prerequisite closure under
separate node, relationship, and index bounds. Shared Work is counted once. Missing live
Work remains explicit; cycles, dangling dependencies, outside selections, duplicates,
and undersized budgets refuse.

Current `MapWorkAssessmentRecord` values retain per-Work ranges, precision, source,
assumptions, complexity, difficulty, uncertainty, and evidence. Stale or missing
estimates are listed and excluded from aggregation rather than treated as zero. Known
agent-hour intervals count each Work identity once. Elapsed output uses a topological
longest-path calculation over explicit prerequisites, including reverse-lexical graphs;
it is labeled a precedence-only lower bound. It makes no resource-feasible schedule
claim.

## Verification

All Cargo commands used the fixed `run-cargo.ps1` target and `--offline`.

- `cargo test -p zap-domain --test strategic_map --offline`: 5 passed, 0 failed,
  0 ignored. The real registered-query/store cases cover 524 source nodes, pagination
  beyond 512, empty filtered continuation, source/epoch/revision/filter cursor refusal,
  missing Work, typed relationship distinctions, reference-only resources, route
  diamond deduplication, field-specific equal-endpoint Fact/Region edges, a greater-than-
  4 KiB Unicode Fact statement, invalid selections, cycles, dangling dependencies, and
  source record preservation.
- `cargo test -p zap-domain --lib --offline`: 7 passed, 0 failed, 0 ignored. The added
  unit proves reverse-lexical topological ordering and a shared diamond lower bound.
- strict clippy for `zap-domain --lib --test strategic_map`: passed with `-D warnings`.
- `cargo fmt -p zap-domain -- --check`: passed.
- AI-Native conform: 9 gated, 0 exempt, 0 findings, baseline 0.
- specmap orphan gate: 0 gated orphans, 0 dispositions.

Package-wide specmap regeneration, application/HTTP/CLI consumption, full package Cargo
regression, installation, and publication remain root/SM04/SM05 work. No Git mutation,
live inference, ignored test, host panel, installed-slot mutation, or publication ran in
SM02.

## Files

Product implementation:

- `crates/zap-domain/src/strategic_map/{mod,model,common,cards,relationships,indexes,queries,route}.rs`
- `crates/zap-domain/src/lib.rs`
- `crates/zap-domain/src/registration.rs`
- `crates/zap-domain/src/viewer_queries/mod.rs` (crate-private exact-node wrapper only)

Focused evidence and public usage:

- `crates/zap-domain/tests/strategic_map.rs`
- `crates/zap-domain/tests/strategic_map/{support,overview,object_route}.rs`
- `vibevm/vibespecs/flows/zap/ZAP-STRATEGIC-MAP-GUIDE.md`

SM02 consumes the separately owned `map_assessment` source/basis API. It does not own or
alter that record's authority, cell, route, or validation semantics.
