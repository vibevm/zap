# Rust implementation and evidence boundaries {#root}

`guide r1`

These non-normative notes preserve implementation decisions and the scope of
validation for the Rust 1.0.0 product. The permanent methodology, agent,
runtime, economics, lowering and [storage contracts](ZAP-RUST-STORAGE.xml)
retain authority. A configured application's capability registry describes its
available operations; planned text does not grant a capability.

## Canonical state and derived reads {#state-and-reads}

`guide r1`

Semantic mutations enter the registered commit service. Accepted events,
receipts, revision and sequence changes, and required derived index mutations
share one transaction. Reopening a configured store restores its persisted
state; replay-based integrity verification and explicit index rebuilding are
separate operations. Missing, stale or incompatible index catalogs refuse.

Raw record pages distinguish Complete, More and UnknownBoundary. Their last key
supports internal continuation; it is not a public query cursor. Public query
cursors bind store, base, revision, query identity, epoch and normalized inputs.
The [store guide](ZAP-RUST-STORE-GUIDE.md) describes those separate interfaces.

Every registered material record can be read through the existing NoOp
projected-record route without a mutation. The
[application-server tests](../../../../crates/zap-app/tests/application_server.rs)
include direct and authenticated HTTP canonical-record reads at unchanged head.
This generic read mechanism does not imply a separate convenience view for
every record type, or that every record family has an individual HTTP test.

## Runtime scheduling and recovery {#runtime-and-recovery}

`guide r1`

Runtime discovery uses Ready-work, relevant-job, pending-authorization and
per-job wait indexes. Ready work is ordered by numeric order and WorkId.
Packetless work is skipped while bounded continuation searches for a runnable
candidate; it does not reserve tentative capacity. Coordinator selection binds
the initial store and revision. Terminal execution retains occupancy while its
external effect is Started or Unknown.

The [runtime persistence journeys](../../../../crates/zap-runtime/tests/runtime_persistence.rs)
cover indexed discovery after 4,101 settled jobs, exact recovery after reopen,
pending authorizations and wait insertion/removal. The application frontier
fixture separately places Ready work after 4,101 unrelated Accepted records.
Measured read decorators reject record-family scans on those paths.

The coordinator counts its index operations, exact reads and read-port calls
under one finite budget. Nested basis/admission providers retain their own
bounds. These generated fixtures establish continuation and locality, not
universal latency, memory or whole-command cost guarantees.

## Deterministic and external evidence {#execution-evidence}

`guide r1`

The [deterministic completion journey](../../../../crates/zap-app/tests/deterministic_campaign_completion.rs)
uses actual service, runtime, candidate, evidence and closure routes with a
controlled host fixture. It keeps claims, observations, safe state, verification
and acceptance distinct. Native dispatch refusal and unknown-effect recovery
are separate evidence from successful external execution.

The selected release evidence does not claim successful live model inference or
a native goal-change acknowledgment. Desired profiles, resolved profiles,
capability observations, manual application and unavailable operations remain
explicit. No local Qwen inference is required. Offline bundles preserve
protocol mechanics; a simulated weak worker is not evidence of model quality.

The accepted package inventory contains 176 unit/integration cases and 185
doctests. A complete pass through the inventory was followed by focused repairs
of seven fixture/registration failures; the unchanged passing targets and the
corrected targets form the final evidence. Sixteen special large-data or native
probe cases remain ignored. The environment-conditional Vibe adapter test can
return early and is not itself proof of a live Vibe integration. Installation
and binary checks have their own artifact-specific evidence.

## Migration and comparison limits {#migration-and-comparison}

`guide r1`

The frozen zap/1 reader and archive preserve their supported raw bytes, codecs,
digests and history. The normalized application import accepts revision-zero
legacy state and explicitly refuses unsupported nonzero normalization. Import
preserves inactive authority; explicit registered activation is a later step.
The legacy corpus and
[activation journey](../../../../crates/zap-app/tests/legacy_activation.rs)
are distinct evidence. Importing a campaign does not start its work.

Detached effect preparation preserves the live revision. Base comparison uses
the initial relevant scope and does not support an arbitrary comparison root
that exists only because an earlier simulated effect created it. This limitation
applies to both alternatives/NoOp comparison and scoped causal-basis comparison;
it is not a claim that every hypothetical sequence is equivalent to live state.

## Process and distribution boundaries {#process-and-distribution}

`guide r1`

The production core, domain and runtime receive explicit ports. Filesystem,
network and process integration lives in application/store adapters. The
installed ambient-environment checker covers environment-variable access; it
is not a general mechanical filesystem/process/network audit.

Relative material roots resolve against the configuration directory. Other relative paths and
the Vibe subprocess's inherited environment/cwd follow the
[application guide](ZAP-RUST-APP-GUIDE.md#internal-boundaries) and
[CLI guide](ZAP-CLI.md). The trusted local service boundary is not an operating
system sandbox or protection against a hostile administrator.

The source package carries Cargo.lock and exact package-local specmark sources
with [provenance](../../../../crates/vendor/PROVENANCE.md). Python prototypes,
private captures, development checkpoints, generated dependency slots and
compiled output are excluded. Offline release installation is an execution
constraint of this validation campaign, not a production requirement that all
users remain offline. Build and installed-source receipts identify actual
artifact bytes separately from these permanent behavioral notes.
