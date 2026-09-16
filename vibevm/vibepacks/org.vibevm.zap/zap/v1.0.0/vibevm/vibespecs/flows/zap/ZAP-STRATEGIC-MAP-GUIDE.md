# ZAP strategic map Rust usage guide {#root}

`guide r1`

The strategic map API reads one explicitly named `StrategicPlanRecord`. It projects
existing identities and records at one query snapshot. Reading it does not create
Work, change a plan, authorize a route, allocate a resource, or accept a result.

## Object references {#object-references}

`guide r1`

`MapObjectRef` keeps existing viewer objects, strategy revisions, and execution
resource references in distinct tagged variants. Display names and landmark facets do
not create identity.

```rust
use zap_domain::strategic_map::MapObjectRef;
use zap_domain::viewer_queries::ViewerNodeId;
use zap_wire::{ResourceId, StrategicRevisionId, WorkId};

let work = MapObjectRef::Viewer(ViewerNodeId::Work(WorkId::parse("work.map")?));
let strategy = MapObjectRef::Strategy(StrategicRevisionId::parse("strategy.7")?);
let resource = MapObjectRef::Resource(ResourceId::parse("resource.build-slot")?);
assert_ne!(work, strategy);
assert_ne!(strategy, resource);
# Ok::<(), zap_wire::ZapError>(())
```

## Overview {#overview}

`guide r1`

`MapOverviewInput` names the exact strategy, a filter, an emitted-card limit, and an
operation budget. `All` visits every strategic node. `Landmarks` emits only
materialized non-Atom Work: `Portfolio`, `Campaign`, `Phase`, `Workstream`, `Group`,
`Gate`, or `Horizon`.
Unmaterialized strategic nodes remain in `missing_work_ids`; they are not silently
classified as landmarks.

Each page reports the source plan's node count and canonical encoded byte length.
Those values make clear that decoding the existing source record is separate from the
number of memberships examined and cards emitted. Membership examination, emitted
relationships, and index rows are separately bounded components; they are not claimed
to be one measured CPU-cost total.

```rust
use zap_domain::strategic_map::{MapOverviewFilter, MapOverviewInput};
use zap_wire::StrategicRevisionId;

let input = MapOverviewInput {
    strategy_id: StrategicRevisionId::parse("strategy.7")?,
    filter: MapOverviewFilter::Landmarks,
    cursor: None,
    limit: 8,
    operation_budget: 32,
};
assert_eq!(input.limit, 8);
# Ok::<(), zap_wire::ZapError>(())
```

## Continuations {#continuations}

`guide r1`

An overview cursor binds store, base, query epoch, snapshot revision, strategy ID,
strategy record revision, strategy semantic digest, normalized filter, and the last
examined Work ID. An object cursor binds the same source plus the exact object and
relationship offset. Foreign, stale, wrong-epoch, changed-filter, or changed-source
cursors refuse.

Filtering is examination-based. A page may emit no landmark cards and still carry a
continuation after examining a bounded source segment. The caller follows the result's
typed `next` field; outer `UnknownBoundary` means that more typed continuation work is
available, not that omitted objects are absent.

## Semantic cards {#semantic-cards}

`guide r1`

`MapObjectInput` reads one `MapObjectRef` against the selected strategy and pages its
complete, budget-checked relationship set. A materialized Work card takes its canonical
name and state from `WorkRecord`; a selected active contract supplies goal, acceptance
text, sources, and resource references. Active-contract selection uses the existing
complete contract-index policy: the last active contract in canonical ContractId index
order. It does not choose a highest revision or an arbitrary first row.

An unmaterialized strategic Work uses the immutable `StrategicNode` title,
obligations, dependencies, and refinement trigger. Its source state is
`StrategicNodeOnly`, its live Work kind/type are absent, and its acceptance view does
not claim a Work state.

`MapTextValue` distinguishes an available value and its source from a bounded reason
why the field is missing. `MapBlockerView::NotEvaluated` states that card construction
did not run readiness, hold, pause, capacity, or proof-validity evaluation. Raw
dependencies are relationships; an empty vector is never used to claim readiness.

Available card text preserves the largest supported existing viewer text, including a
16 KiB Fact statement. A Fact card's short canonical name is derived from its stable
FactId while the complete statement remains in `description` and `underlying`; it is
not silently truncated to fit a display name.

`SemanticCard.underlying` preserves an existing `ViewerDetail` without changing that
wire type. Strategy and reference-only resource cards retain their typed object refs
instead of inventing viewer variants.

A materialized Work card also exposes `assessment_source_fingerprint`, computed from
the already loaded Work and selected active contract. Clients can bind the first
`MapWorkAssessmentProposed` to that witness without private store access. The witness
does not grant authority; assessment ingress still rechecks it at commit time.

## Acceptance meaning {#acceptance-meaning}

`guide r1`

Acceptance fields report the source record's actual category:

- `StrategyState` is a planning revision state, not outcome acceptance.
- `WorkRecordState` carries the Work state and declared criteria. It does not establish
  current acceptance-proof validity.
- `ObligationState` preserves obligation status and disposition.
- `OutcomeState` preserves the outcome lifecycle state.
- `StrategicNodeOnly` and `NotApplicable` make absent live acceptance semantics clear.

A non-Atom landmark remains a facet of its exact `WorkKind`. It introduces no
milestone identity or completion predicate.

## Typed relationships {#typed-relationships}

`guide r1`

`MapRelationship` has stable endpoints, a semantic kind, optional obligation ownership
role, an exact source identity, and a digest ID computed from all of those fields.
Parallel meanings between the same endpoints remain distinct.

The projection preserves Work containment and prerequisites, strategic obligation
coverage, obligation owners and roles, current outcome scope and successors, contract
acceptance duties, source and evidence references, region containment, resource use,
and knowledge relations. `KnowledgeDependencyRecord` relations remain `DependsOn`,
`DerivedFrom`, `Supports`, `Verifies`, `Affects`, or `Consumes`; they are not collapsed
into a generic road.

`WorkRecord.depends_on` is reported as `WorkPrerequisite` under its actual source. The
projection does not relabel it as an acceptance-only edge. A separate client pilot may
add presentation-only acceptance dependencies, but those remain a different source and
kind.

## Relationship coverage {#relationship-coverage}

`guide r1`

Exact cards use existing current record and derived-index joins. `WORK_CHILD`,
`WORK_DEPENDENT`, knowledge incoming/outgoing, obligation-owner, source-subject,
fact-subject, region-subject, and evidence-work partitions stay revision- and
catalog-bound.

`MapRelationshipGap` records known limits. Project-scoped sources are not expanded for
every object, missing referenced records stay explicit, and execution resources have
no reverse consumer index. The implementation does not replace those gaps with family
scans. Assessment availability is a separate card field and never changes relationship
completeness.

## Descriptive assessments {#descriptive-assessments}

`guide r1`

The optional `MapAssessmentView` retains the exact `MapWorkAssessmentRecord` and its
current or stale source-fingerprint classification. Absence is reported independently
from relationship coverage. Display labels and explanations do not replace the
canonical Work title, goal, or acceptance text.

Remaining agent effort, elapsed duration, passive wait, structural complexity,
executor-relative difficulty, and uncertainty remain separate assessment dimensions.
`Unassessed` is distinct from `Low`. Stale estimates remain inspectable but are excluded
from current route aggregation.

## Route projection {#route-projection}

`guide r1`

`MapRouteInput` supplies a nonempty, strictly ordered set of distinct Work IDs from the
selected plan. The route query validates the strategy DAG and expands the complete
prerequisite closure. Each Work identity is counted once, including shared diamond
prerequisites. Missing materialized Work remains explicit.

Node, edge, and index work are each capped by `operation_budget`. If the required
closure exceeds a cap, the query returns `LimitExceeded`; it does not return a partial
route labeled complete. The result preserves the selected IDs separately from added
prerequisites and returns only strategy-source prerequisite edges.

```rust
use zap_domain::strategic_map::MapRouteInput;
use zap_wire::{StrategicRevisionId, WorkId};

let input = MapRouteInput {
    strategy_id: StrategicRevisionId::parse("strategy.7")?,
    selected_work_ids: vec![WorkId::parse("work.release")?],
    operation_budget: 64,
};
assert_eq!(input.selected_work_ids.len(), 1);
# Ok::<(), zap_wire::ZapError>(())
```

## Route estimates {#route-estimates}

`guide r1`

Current assessment records retain per-Work ranges, precision, source, assumptions, and
freshness. Known agent-hour intervals sum each distinct Work once. Missing or stale
estimates are listed and excluded; they never become zero estimates.

Elapsed output is a precedence-only lower bound over current known lower bounds. The
calculation uses a topological traversal of the actual prerequisite DAG, so lexical Work
ID order has no scheduling meaning and shared diamond work is not duplicated. When
estimates are missing, the result names them and marks the known subtotal incomplete.
It makes no resource-feasible schedule claim: calendars, contention, capacity, and
executor availability are outside this projection.

## Query registration {#query-registration}

`guide r1`

The domain `QuerySet` registers three versioned IDs:

- `zap.map.overview.v1` for source-plan membership pages;
- `zap.map.object.v1` for exact semantic cards and typed relationships;
- `zap.map.route.v1` for complete selected prerequisite closures and truthful measures.

They use the existing generic machine query transport. No new command channel, runtime
frontier, model call, frontend, or plan mutation is introduced.

## Public type catalog {#public-type-catalog}

`guide r1`

This catalog links every public strategic-map declaration to the operation described
by its `documents` section.

| Public type | Operation |
| --- | --- |
| `strategic_map::MapObjectRef` | Typed object identity. |
| `strategic_map::MapSemanticType` | Card semantic family independent of Work kind/type. |
| `strategic_map::MapLandmarkFacet` | Exact non-Atom Work landmark facet. |
| `strategic_map::MapTextSource` | Source of available bounded card text. |
| `strategic_map::MapTextValue` | Available text or explicit missing reason. |
| `strategic_map::MapSourceState` | Materialized, strategic-only, or reference-only source. |
| `strategic_map::MapAcceptanceView` | Actual source-record acceptance category. |
| `strategic_map::MapRelationshipKind` | Typed relationship meaning. |
| `strategic_map::MapRelationshipSource` | Exact relationship source identity. |
| `strategic_map::MapRelationship` | Stable typed edge and digest identity. |
| `strategic_map::MapRelationshipGap` | Explicit finite relation-coverage gap. |
| `strategic_map::MapBlockerView` | Established, unevaluated, or inapplicable blocker state. |
| `strategic_map::MapAssessmentState` | Assessment availability independent of graph coverage. |
| `strategic_map::MapAssessmentView` | Exact assessment record and freshness. |
| `strategic_map::SemanticCard` | Common bounded semantic card. |
| `strategic_map::MapOverviewFilter` | All-members or non-Atom landmark selection. |
| `strategic_map::MapOverviewCursor` | Source-bound overview continuation. |
| `strategic_map::MapOverviewInput` | Overview request, output limit, and operation budget. |
| `strategic_map::MapOverviewResult` | Bounded cards, diagnostics, counts, and continuation. |
| `strategic_map::MapObjectCursor` | Source/object-bound relationship continuation. |
| `strategic_map::MapObjectInput` | Exact object-card request. |
| `strategic_map::MapObjectResult` | Paged card relationships and counts. |
| `strategic_map::MapRouteInput` | Explicit selected Work and closure budget. |
| `strategic_map::MapRouteWorkEstimate` | One Work's retained current/stale estimate data. |
| `strategic_map::MapRouteEstimateSummary` | Known subtotal, missing inputs, and lower bound. |
| `strategic_map::MapRouteResult` | Complete prerequisite closure and measures. |
| `strategic_map::StrategicMapOverviewQuery` | Registered `zap.map.overview.v1` adapter. |
| `strategic_map::StrategicMapObjectQuery` | Registered `zap.map.object.v1` adapter. |
| `strategic_map::StrategicMapRouteQuery` | Registered `zap.map.route.v1` adapter. |
