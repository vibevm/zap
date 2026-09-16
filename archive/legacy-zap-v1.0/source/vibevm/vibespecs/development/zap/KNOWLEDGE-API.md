# ZAP knowledge and source API

The knowledge companion is a Python 3.11 standard-library extension of the
foundation reducer. It never authenticates a caller, executes work, or treats
source text as authority. Integration composes `KNOWLEDGE_HANDLERS` explicitly
with the core/domain/control/runtime registries.

## Captured sources

`zaplib.sources.capture_source(path, allowed_root, *, source_id=None,
source_kind="file", applicability_scope=None) -> descriptor` reads one regular
file through the foundation symlink/reparse guard. The path must stay below the
allowed root. The returned exact `zap-source/1` descriptor is:

```json
{
  "schema": "zap-source/1",
  "id": "stable-id",
  "source_kind": "file | vibevm_xml_spec",
  "root": "absolute captured root",
  "path": "root-relative/locator",
  "content_sha256": "64 hex characters",
  "bytes": 123,
  "applicability_scope": {"kind": "unassessed | project | subjects", "subjects": []}
}
```

The default scope is `unassessed`; capture existence and a source label grant
no truth, acceptance, applicability, or authority. `subjects` entries are exact
typed endpoints `{kind,id}`. `observe_source(descriptor, allowed_root=None)`
reads the bound path and returns a candidate observation with status `current`,
`changed`, or `unavailable`; it does not mutate projection state.

`capture_vibevm_facts(path, allowed_root, ...) -> {source,facts}` accepts only
an XML file whose root is exactly `{https://vibevm.org/spec/1}spec`. It extracts
same-namespace elements explicitly marked `fact="true"`. Markdown, bridge XML,
foreign namespaces, unmarked elements, DTDs, and entity declarations do not
become native facts. Each fact keeps its source/hash/address/text and stores the
authored `status` as `normative_status`; it starts with
`observation_status="unobserved"` and `acceptance_status="unassessed"`.
Consequently `impl/done` is a normative source marker, never ZAP acceptance.

## Applicability

`zaplib.sources.current_applicability(state, refs) -> result` is the frozen
domain-B interoperation call. `refs` is a nonduplicated list of stable source
IDs. The exact result is:

```json
{
  "status": "applicable | stale | unknown",
  "refs": [],
  "stale_refs": [],
  "unknown_refs": [],
  "incomplete_closure": false
}
```

`applicable` requires every named source to have its current captured content,
an explicit `applicable` assessment bound to that content, and an explicit
complete dependency closure for the relevant local boundary. A changed or
explicitly inapplicable source is `stale`. Missing, unavailable, unassessed, or
incompletely bounded inputs are `unknown`. Empty refs are unknown. Unrelated
unknown sources and fog regions are not consulted, so they cannot invalidate a
fully bounded local proof. “Complete” means the explicitly bounded relevant
inputs of that proof; it does not claim that the whole world is known.

Domain evidence adjudication stores the result fields as returned and keeps the
assessment scope separately. Scope-to-consumer checks remain part of domain
adjudication.

`compare_source_captures(state, captures) -> result` is the pure adaptive-review
CAS helper. Each input is exactly `{source_id,sha256}`. It returns
`{status,captures,stale_captures,unknown_captures}`, where status is `current`,
`stale`, or `unknown`. It compares recorded content identity only; it does not
claim applicability or acceptance.

## Dependency graph and fog

Dependency endpoints support `source`, `fact`, `evidence`, `decision`, `task`,
`node`, `obligation`, and `outcome`. Obligation and outcome lookup uses
`extensions.domain.obligations` and `extensions.domain.outcome_revisions`.
`knowledge.dependency-recorded` stores an acyclic edge from `prerequisite` to
`dependent` with a typed relation. `invalidation_closure(state, roots)` walks
only known edges iteratively and returns sorted affected endpoints plus
`incomplete_closure`. A changed adjudicated source marks only this known closure
stale and preserves unrelated proof. Missing relevant edges remain visible as
incomplete rather than implying no impact.

Accepting a new dependency invalidates any previously complete closure on its
prerequisite and known upstream chain. Recapturing changed bytes performs the
same selective invalidation even when no preceding source-observed event exists.
Native fact capture installs its source-to-fact derivation edge explicitly.

Knowledge state is lazy under `extensions.knowledge`, version 1. The public
`knowledge_state(state)` returns a detached projection containing sources,
native facts, dependencies, closure assessments, applicability assessments,
regions, invalidations, and endpoint status.

`knowledge_snapshot(state, region_ids=None) -> {revision,sha256,regions}` binds
an exact fog view for adaptive review. `regions` is a dictionary keyed by the
selected stable IDs and contains the complete region rows, including question,
state, relevance, lineage, history, and per-region revision. `revision` is the
sum of selected row revisions; `sha256` hashes only those exact rows. A missing
or fabricated selected ID refuses. Changes to a selected region move the
revision/hash, while unrelated source or region changes do not stale a bounded
review. Omitting IDs intentionally captures all currently materialized regions.

Fog regions use states `unexamined`, `bounded`, `evidenced`, and `invalidated`,
with relevance `relevant`, `irrelevant`, or `unknown`. The handlers record every
transition and reason. Reopening an evidenced/invalidated region is explicit;
split and merge preserve parent/child lineage and invalidate the superseded
region records. Irrelevance is a route choice, not evidence that the question
was answered.

## Event routing

`KNOWLEDGE_EVENT_SCHEMAS` is the machine-readable required-field map keyed by
every actual handler kind. `KNOWLEDGE_EVENT_ROUTES` has the same keys and gives
`route`, `action`, and `affects_readiness` for fail-closed service routing.

- `knowledge.source-recorded`, `knowledge.native-facts-recorded`, and
  `knowledge.source-recaptured` are filesystem effect-adapter events with
  control action `evidence.adjudicate`; the `effect_adapter` discriminator
  requires native actual-byte capture, and producer JSON is not a provenance adapter.
- `knowledge.source-observation-recorded` is harmless `agent_data`. It stores an
  unverified candidate and cannot change capture status or invalidate proof.
- `knowledge.source-observed`, `knowledge.closure-assessed`, and
  `knowledge.applicability-assessed` require the trusted service action
  `evidence.adjudicate` because they can affect readiness or proof.
- `knowledge.dependency-recorded` also requires `evidence.adjudicate`; accepting
  an edge invalidates complete closure assessments for its prerequisite and
  known upstream chain until they are explicitly rebound. Fog events are
  candidate data and do not directly admit work.

The pure reducer cannot authenticate those routes. The application service must
enforce the map before append. Replay reproduces an already admitted stored
transition.

## Project fact promotion

`build_promotion_proposal(state, *, promotion_id, fact_id, source_refs, target)`
is pure and requires current applicable source closure. The target is a relative
JSON path. It binds fact content, base, revision, source identities, and target.

`promote_fact(state, proposal, project_root, *, authorization, event_writer,
fault=None)` calls an injected trusted authorization function. The returned
grant must bind action `fact.promote`, the exact promotion ID, nonempty accepted
proof references, and a public authorization reference. Every proof ID must
resolve to an accepted `zap-domain/evidence-adjudicated/1` row, a concrete core
observation, and currently applicable complete source closure; a label or fake
ID is insufficient. Immediately before the filesystem effect, the adapter
reopens every bound source and refuses changed or unavailable bytes even when
no source-observed event has yet recorded the drift. Only then can it create a
`zap-project-fact/1` artifact below the guarded project root.

The permanent artifact embeds `zap-portable-proof/1` descriptors: observation
claim/subject/result/artifact pointers/node scope, adjudication revision/event,
outcome and obligation scope, verification argv/target/toolchain/environment/
subjects/cases, limitations, and bound source paths/hashes/applicability scopes.
It also embeds the immediate source reobservations. The private campaign store
is therefore not the sole explanation of accepted project knowledge.

Unexpected existing bytes, symlinks/reparse paths, outside-root targets, stale
state, and stale applicability refuse. Exact existing bytes support recovery
after a crash. The injected event writer persists the effect receipt. If that
receipt fails after a new artifact is linked, the adapter rolls back only while
a retained hard-link identity and exact bytes prove the target is still owned
by that adapter call. A foreign replacement is never deleted. The result reports
`rolled_back`, `absent_unreceipted`, or `present_unreceipted` truthfully. Module map
`PROMOTION_OPERATIONS` exposes `fact.promote` and its trusted route to the
integration capability builder.
