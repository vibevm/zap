# ZAP runtime integration review

This is an independent implementation review, not package acceptance.

## Findings and dispositions

### P1 — semantic review lacked the knowledge state it must bind

The reassessment request supplied domain and source rows but omitted relevant
knowledge regions, their deterministic snapshot, typed dependencies,
applicability/closure, invalidation evidence and native fact changes. The
model runs without repository tools. `materialize_model_payload` rebound only
base/ZAP/domain capture values and left `knowledge.after` untouched.

A real relevant-region probe returned `context_has_knowledge=False` and
`context_has_dependencies=False`; a model placeholder SHA remained all zeroes
while `knowledge_snapshot` returned a different real SHA. Domain review must
therefore refuse unless the model guesses a state hash. Same-response region
events make guessing impossible because the target snapshot exists only after
those commands apply.

Disposition: assigned to runtime E. Include exact selected relevant knowledge
and native-fact/dependency context in both request and scope hash, then inject
or validate the exact post-region-event snapshot before submitting the review.
Relevant drift must stale; unrelated region changes must retain scoped rebind.

### P1 — knowledge command contracts were not machine-complete

The provider receives `KNOWLEDGE_EVENT_SCHEMAS`, but the reviewed descriptors
listed required top-level names only. They did not describe enum values or
nested region split/merge payloads. A no-tools provider could not construct the
exact commands it was instructed to emit.

Disposition: assigned to runtime/knowledge E. Publish exact closed nested
schemas and test the actual contracts passed to the provider.

### P1 — applied job reconciliation had no runtime consumer

`domain.review-applied` stored `job_reconciliation` and explicitly labelled
its effects planned. No runtime code consumed continue, finish, drain,
preserve-candidate or revalidate. Live work therefore continued across a drain,
and a changed work state could later reject the ordinary result transition.

Disposition: repaired by the G1 handler/effect API in
`RECONCILIATION-API.md`. Runtime E owns its two minimal loop hooks.

### P1 — semantic no-action/rejection could strand work permanently

The runtime mapped no-action and rejected responses to `awaiting_evidence`.
Scheduling selected only pending reviews, duplicate review creation refused the
existing identity, and trigger discovery considered every old trigger used.
A five-tick probe left the review in `awaiting_evidence` with no replacement
review request. The same path stranded candidate acceptance after a malformed
provider command.

Disposition: assigned to runtime E. Treat stale, malformed/rejected,
needs-evidence, wait and no-change as distinct durable outcomes. Retry rejected
responses with bounded attempt identity/backoff; requeue needs-evidence only on
relevant evidence/knowledge change; persist wait with a retry boundary; and
resolve no-change only where a truthful recorded assessment exists. Adaptive
invalidation should use an applied keep-route review.

### P1 — closure could miss post-acceptance source drift

Source refresh selected frontier work and nonaccepted jobs. Once all work was
accepted, proof inputs could disappear from the refresh set while a closure
semantic request remained outstanding. The knowledge projection still said
current, so closure rebind could accept bytes that had changed on disk.

Disposition: assigned to runtime E. Refresh every source used by current
accepted/final-gate proof before completion and closure admission. A changed
observation must alter current coverage and stale the pending closure.

### P1 — CLI automatic mode could not supply owner rule values

`RuntimeConfig` has a programmatic `assessment_provider`, so an embedding can
produce trusted values. The shipped profile loader accepts only backoff and
poll interval under `runtime`, and `build_automatic_coordinator` constructs the
config without that provider. `assessment_for` therefore supplies `{}`. Owner
stop-policy `eq` expressions on any field then evaluate unknown, and the
service correctly refuses the action as `NEEDS_EVIDENCE`. State-derived
`failed_approaches` rules still work, but the ordinary profile/CLI has no way to
supply other owner-defined assessment values.

Disposition: assigned to integration F. Keep fail-closed evaluation. Expose a
bounded trusted assessment provider through automatic construction, with
explicit named observations and exact value/drain capture; do not evaluate
arbitrary policy code or convert missing fields to false.

## G1 reconciliation validation

The focused real-process suite covers every reconciliation action, exact
registry/schema/routes, campaign-pause precedence, changed-source refusal,
unaffected peer execution, cooperative drain, restart idempotency, and the
separation between process exit and task safety. It also exercises accepted
work through revalidation release, a fresh validation generation, a new worker
attempt and verification, old-proof refusal and fresh central acceptance.

```text
python -B -m unittest test_runtime_reconciliation.py
```

Result from the integrated runtime: `Ran 6 tests in 118.862s` — `OK`.

Three added focused cases verify that ordinary `AutomaticCoordinator.tick`
invokes the merged registry, failed/blocked producer output remains rework,
and a pause arriving after durable revalidation release defers readiness across
coordinator restart. Results: `5.356s`, `8.443s`, and `9.271s`, all `OK`.

### Review boundary still open

F owns the accepted assessment-provider repair. Runtime E owns the remaining
semantic-context, result-recovery and closure-refresh findings above.

## Mandatory kernel capability matrix

This matrix rechecked the permanent `ZAP-RUNTIME` clauses after the accepted
repairs. It records implementation evidence and the remaining acceptance
boundary; it does not promote specification status.

| Permanent clause | Implemented evidence | Review result |
| --- | --- | --- |
| `PORTABLE-RUNTIME` | Standard-library CLI, typed application service, automatic coordinator, authenticated JSON/SSE backend, and finite/live event-tail APIs | Kernel present; Qwik/canvas remains explicitly separate future work |
| `PRESERVE-AND-EXTEND` | Immutable base, append-only CAS journal, pending-tail boundary, deterministic replay, snapshots, quarantine and explicit repair | No missing kernel behavior found |
| `MODULE-BOUNDARIES` | Composed core/control/domain/knowledge/runtime/reconciliation registries; exact route partitions; effect adapters outside reducers | No missing kernel behavior found |
| `TRUSTED-CONTROL` | External credential bindings and host principal; separate data/action/observation/read routes; public sanitization | No missing kernel behavior found within the documented same-user/OS-admin threat boundary |
| `CONTROL-BINDING` | Campaign/base/charter/payload/source binding, scoped sticky pauses, delivery and safe-state acknowledgement, one-shot exceptions, durable reservation rebind | No missing kernel behavior found |
| `ADAPTIVE-DOMAIN` | Intent/outcome revisions, full obligation dispositions, typed ownership, fog snapshots, selective evidence carryover, validation generations, deferrals, central/integration acceptance, truthful closure | No missing kernel behavior found after B2/G1 |
| `ACTUAL-RUNNER` | Persistent tick/run loop, literal-argv worker and verification transports, semantic adapter, strict provider contracts, model-result validation, candidate-only producer boundary | No missing kernel behavior found after semantic-context and liveness repairs |
| `RECOVERY-AND-RESOURCES` | Idempotent transport recovery, unknown-effect hold, waits/backoff, read/write/resource/integration capacities, truthful stop, durable adaptive job reconciliation | No missing kernel behavior found after G1 |
| `FACTS-AND-PROOF` | Exact source capture/reobservation, dependency/applicability/closure state, native facts, scoped invalidation, verified artifact capture, portable proof and permanent promotion | No missing kernel behavior found |
| `BACKEND-CONSISTENCY` | Capabilities from actual registries, snapshot/tail identities, pagination/search/subgraph/detail/explanation APIs, gaps and pending-tail diagnostics, credential/output redaction | No missing kernel behavior found; interactive rendering is outside the package |
| `MIGRATION-AND-RELEASE` | Lossless NEXT migration evidence for 425 nodes, 212 task contracts, 64 mandates and 1,292 obligations; zero-execution isolation; package-only verification and live isolated worker/check proof | Final combined regression, installed-payload verification, and publication remain root release evidence rather than kernel implementation gaps |

The earlier P1 findings are now assigned or implemented: runtime E supplied
scoped knowledge/native-fact context and deterministic review snapshot binding,
exact knowledge command schemas, semantic recovery, and accepted-proof source
refresh; integration F supplied the configured assessment adapter; G1 supplied
durable live-job reconciliation. The review found no additional mandatory
kernel clause still represented only by prose.

## English specification pass

All ZAP XML specifications and research/pilot XML prose were translated to
English without changing element names, stable IDs, status values, addresses,
hashes, or protocol identifiers. `AUTHORING-LANGUAGE` is the new permanent
methodology clause: ZAP-authored specifications, public docs, skills, and
examples are English; imported/legacy bytes and intentional Unicode fixtures
remain exact exceptions. README and the remaining Russian audit prose were
also translated after F froze its public API edits.

Validation parsed all eight XML files, found no duplicate document IDs or fact
element identities, and found zero Cyrillic characters in README, vibe.toml,
or `vibevm/vibespecs`. Fact-identity counts remained 25 adaptive, 56 data,
7 pilot, and 18 ancillary; methodology is 50 because it gained the one required
`AUTHORING-LANGUAGE` clause. There were no scan exceptions.
