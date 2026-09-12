# ZAP-D knowledge and durable storage report

## Result

The accepted foundation now has complete companions for captured sources,
native XML facts, applicability, known dependency invalidation, fog evolution,
snapshot tails, non-destructive migration, atomic initial publication,
auditable final-tail repair, and authorized permanent fact promotion.

No operation activates a charter, dispatches work, runs NEXT, or interprets an
editable actor label as authority. Pure reducers replay stored transitions.
Filesystem capture, promotion, and repair are explicit effect adapters.

## Knowledge surface

`sources.py` publishes stable `zap-source/1` descriptors with guarded root and
relative path, exact SHA-256/byte identity, kind, and explicit applicability
scope. Capture defaults to unassessed. `observe_source` reads actual filesystem
bytes; untrusted producer observations have their own candidate event and do
not alter readiness.

The native facts adapter accepts only namespaced VibeVM `<spec>` XML and only
same-namespace elements carrying `fact="true"`. It ignores foreign/unmarked
data and refuses Markdown, bridge roots, DTDs, and entity declarations. Authored
progress status is stored separately from observation and acceptance, so an
`impl/done` source marker remains unaccepted.

`KNOWLEDGE_HANDLERS` implements explicit source capture/recapture, observation
candidate and adjudication, dependency, closure, applicability, region state,
relevance, split, and merge events. `KNOWLEDGE_EVENT_SCHEMAS` and
`KNOWLEDGE_EVENT_ROUTES` cover every handler. Applicability, closure, and
accepted invalidation route through trusted `evidence.adjudicate`; source bytes
route through the `evidence.adjudicate` control action plus the native
`effect_adapter` discriminator; harmless producer candidates stay agent data.

`current_applicability` has the domain-B exact result fields `status`, `refs`,
`stale_refs`, `unknown_refs`, and `incomplete_closure`. It inspects only named
source refs and their explicitly bounded local closure. Unrelated unknown fog
does not poison local proof; a missing relevant edge or assessment stays
unknown. `compare_source_captures` supplies adaptive review with a pure
`{source_id,sha256}` CAS comparison that does not imply applicability.

Dependency endpoints include sources, facts, evidence, decisions, tasks, nodes,
domain obligations, and domain outcome revisions. Edges are acyclic. Changed
source adjudication traverses only the known dependent closure, marks that
closure stale, retains unrelated evidence, and preserves an incomplete-closure
flag when the graph boundary is not established.

Fog history explicitly records evidence transitions, reopening, relevance,
split lineage, and merge lineage under `extensions.knowledge` version 1.
`knowledge_snapshot` binds exact selected region rows/questions with a scoped
revision/hash, so relevant reopening or expansion stales an adaptive review
while unrelated knowledge does not; fabricated region IDs refuse.

## Storage surface

`storage.import_mup` now prepares base and import receipt in a private sibling
directory and publishes the complete store with one rename. Pre-publication
faults expose no output directory. A post-publication fault leaves a complete
legacy-compatible store. Existing base bytes and hashes remain unchanged.
`read_committed_journal` and `replay_committed` are reusable validated tail
boundaries for recovery, snapshots, and integration.

`snapshots.py` binds projection state to base hash, projection schema, reducer
version, exact handler-kind identity, state hash, revision, and committed-log
prefix bytes/hash/line boundary/last event. Tail replay validates every new
event with foundation sequence, CAS, hash, and duplicate rules. Foreign base,
prefix, state, reducer version, or handler set refuses. Snapshot creation holds
the writer lock across state/prefix capture. A cold load derives state from the
validated base and prefix; only successful derivation populates the optional
in-process cache. Its exact identity key permits warm prefix reuse without
trusting a serialized self-claim.

`migration.py` performs a non-destructive atomic import, verifies source hashes
before/after, and writes an exact report inside the new store. The report maps
all node/mandate/task identities, copies task contracts and node constraints,
retains raw captures, and enumerates unknown plan/node/task/group fields with
values. The before/after path-set, sizes, hashes, and embedded import captures
detect added/removed files and ABA capture. Exact revision-zero stores and exact
reports resume safely after crashes without overwrite. Authority-looking
imported data remains unverified; the report records zero activation and zero
command execution.

`recovery.py` repairs only an explicitly selected final fragment under the
existing non-stealing writer lock. It validates the committed middle first and
requires the exact tail hash. Before the journal switch it atomically publishes
the complete original journal, tail bytes, committed prefix, and a bound repair
receipt. The switch is atomic; a separate completion receipt makes both crash
boundaries resumable. Middle corruption and altered quarantine/journal bytes
refuse.

The storage, snapshot, migration, recovery, and promotion modules expose
machine-readable operation maps with actual names and required/optional fields
for integration capability generation.

## Promotion surface

`knowledge_promotion.py` builds a pure proposal bound to current state, fact,
sources, applicability, and relative target. The effect adapter requires an
injected trusted grant for `fact.promote`. Each proof ID must resolve to accepted
domain evidence with a concrete observation and current complete applicability.
The effect adapter reobserves actual bound source bytes immediately before the
write. It writes a new canonical `zap-project-fact/1` JSON artifact only below
the guarded project root, embedding portable observation, adjudication, method,
scope, limitation, source-hash, and reobservation evidence. Stale or unavailable
actual bytes, fake/absent proof, unexpected existing bytes, symlinks, and
outside-root targets refuse.

The adapter records the effect through an injected event writer. Receipt
failure rolls back a newly linked artifact when possible and otherwise reports
`present_unreceipted`; an exact pre-existing artifact supports crash recovery.

## Verification

Focused command:

```text
python -B -m unittest test_sources.py test_knowledge.py test_snapshots.py test_migration.py test_recovery.py test_knowledge_promotion.py
```

Result: 23 tests passed. They cover native source bytes and XML semantics,
normative/acceptance separation, local applicability, producer candidates,
selective and incomplete invalidation, domain endpoint lookup, fog durability,
snapshot-tail equivalence and identity refusals, migration preservation, atomic
import crash points, quarantine/switch crash recovery, middle-corruption and
lock refusal, extension-aware repair, writer-locked snapshot capture, cold/warm
derivation caching, forged rehash refusal, migration retry/path-set/ABA cases,
direct recapture invalidation, selected fog bindings, authorized promotion,
fake/nonexistent proof refusal, unjournaled source-drift refusal, stale/outside/
unexpected-content refusal, owned rollback, and preservation of a foreign replacement.

Because `storage.py` changed, the foundation regression was also rerun:
25 tests passed, including all original 18 behavior tests.
