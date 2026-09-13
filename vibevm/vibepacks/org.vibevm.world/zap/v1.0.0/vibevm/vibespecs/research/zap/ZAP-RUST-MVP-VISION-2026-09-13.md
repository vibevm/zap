# ZAP: durable planning and execution with bounded agent work

Status: architecture accepted by the Owner on 2026-09-13. The subsequent Owner
instruction authorizes planning and implementation with frequent filesystem
checkpoints; the earlier ZAP publication commission continues. NEXT execution
and installed local Qwen inference remain prohibited. The text below records
the accepted design; unevaluated technical candidates still need evidence.
Existing Python code is a prototype and behavioral reference, not the proposed
production runtime. Earlier descriptions of Python as the production runtime
are superseded by the Owner's language requirement and await coordinated spec
revision after design review.

The original Owner input is preserved byte-for-byte in
[OWNER-VISION-INPUT-2026-09-13.txt](OWNER-VISION-INPUT-2026-09-13.txt), SHA-256
`D4FFF19705F6742B84C74CB6F875BDBD0A9E2603AB393D271BB8BECF990B951F`.
It is imported Russian source data; ZAP-authored specifications and this vision
are English. This is a cohesive architecture proposal, not a new executable
campaign or a claim that the described features already exist.

## V01. The system owns continuity; models supply bounded judgments

ZAP should be a durable Rust planning and execution system. Its authority,
strategic intent, work graph, evidence, decisions, and active operations live
outside model context. Models propose semantic judgments and produce candidate
artifacts. Rust code determines which commands are admissible, what work is
ready, which inputs changed, and what must happen next under those judgments.

The recurring cycle is:

1. Capture a material signal with its sources and scope.
2. Recompute affected knowledge, unknown regions, and evidence applicability.
3. Request bounded semantic assessment where an algorithm cannot decide.
4. Compare value, feasibility, incremental cost, and admissible alternatives.
5. Keep the route or admit an authorized revision; reconcile live work.
6. Lower the relevant frontier and dispatch independent ready packets.
7. Collect, verify, and accept results; feed discoveries back into the cycle.

This same mechanism applies at campaign, branch, and task level. A task may
need no further decomposition, a short experiment, or a staged route through
prototype, functional MVP, and productization. Recursive refinement stops when
an executor can understand the work and a sufficient check is inexpensive.
Neither recursion nor repeated reassessment is a mandatory ritual.

## V02. Four representations, with explicit authority between them

| Representation | Purpose | May it silently redefine the desired result? |
| --- | --- | --- |
| Owner intent and charter | Required benefit, essential guarantees, delegated tradeoffs, reserved decisions and stop conditions | Only an authorized Owner revision changes these terms |
| Strategic graph | Current outcome hypothesis, obligations, major methods, forks, risks and integration conditions | Only through an admitted adaptive revision within the charter |
| Lowered execution graph | Concrete work, dependencies, stages, evidence duties and executor requirements | No; every refinement retains its strategic lineage |
| Packet and client projections | The next bounded assignment, GOAL.md, IDE views and resume views | No; they are derived views |

These are linked views of one versioned model, not four independently edited
plans. Strategy remains readable after extensive lowering. A strategic node
may have multiple execution realizations, and one integration task may serve
several strategic obligations. Traceability is therefore a graph, even when a
tree is used for navigation.

Human-reviewable XML can express strategic specifications and portable plan
revisions. The runtime stores the accepted revision and provenance. Editing
an exported XML or Markdown projection does not silently change a running
campaign: its delta enters the same proposal/application path. Package
specifications remain the requirements for the system; runtime state records
one campaign's execution of those requirements.

## V03. Lowering as a checked transformation

A lowering request binds a strategic revision, target branch, executor
capabilities, available facts, current source identities, allowed decisions,
and required maturity. Its output contains work contracts, traceability edges,
verification duties, important alternatives, integration owners, and unresolved
horizons. The kernel checks references, coverage, dependencies, authority,
write conflicts, and disposition of deferrals. A Senior assesses semantic
faithfulness where the change affects architecture or meaning.

Every undischarged prototype, functional-MVP, and production duty is mapped
to executable work, a named successor, or an authorized inapplicability
disposition. Debt cannot disappear at a stage or lowering boundary. A pure
capability-specific render preserves this ledger unchanged.

Coverage checks prove that every obligation has a disposition and supporting
work; they do not prove that the proposed work actually satisfies it. That
requires technical judgment and eventually evidence. An extra edge in a graph
cannot manufacture semantic correctness.

Normal execution lowers the nearest valuable frontier. Distant work retains
its obligations, known forks, and a refinement trigger. For a sealed on-prem
run, lowering prepares the whole intended execution envelope as far as the
available knowledge permits. It does not invent exact future steps across an
unknown architectural boundary. Such a boundary has a diagnostic task,
admissible alternatives, and a return condition.

When strategy changes, the system marks only affected lowering outputs stale,
retains applicable results, and reuses unchanged packets. A global event
counter changing does not itself invalidate every packet. Relevant semantic
inputs, contracts, sources, policies, and resources do.

Semantic lowering changes meaning-bearing work contracts and follows the
change-admission rules. Rendering the same admitted contract for another
model, context window, or harness is a derived operation and does not require
another economics decision. A semantic change cannot be disguised as packet
rendering to evade those rules. Every packet also identifies deliberately
unloaded context and unknown boundaries so omission is not mistaken for absence.

## V04. Roles, capabilities, and authority are separate axes

| Role | Responsibility | Proposed OpenAI default | Owner's Anthropic profile |
| --- | --- | --- | --- |
| Senior | Architecture, important lowering, semantic review, route judgment | gpt-5.6-sol / ultra in a supporting harness | fable / high |
| Middle | Implementation within an accepted contract; ordinary local refinement | gpt-5.6-sol / high | opus / high |
| Junior | Small, bounded work with cheap verification | gpt-5.6-luna / high | sonnet / high |

Senior does not write production code. Middle may delegate suitable small
work to Junior. Routine deterministic acceptance checks do not require a
Senior model invocation for every leaf. Material semantic acceptance remains
the coordinator's responsibility and uses independent review when it has
specific value. A worker never accepts its own candidate.

The Owner chooses profiles and fallback policy. Anthropic names above preserve
the requested logical profiles; adapters resolve actual available model IDs
and effort values rather than guessing them. The same weak local model may
fill all three roles. Role separation then organizes responsibility; it does
not imply increased intelligence or statistically independent review.

Routing considers novelty, architectural impact, consequence of error,
reversibility, context needed, and how cheaply correctness can be checked.
Architecture-wide contract changes, new persistent formats, uncertain
cross-component invariants, or failure of the prepared route require Senior
judgment. A small patch following a settled contract normally goes directly
to Middle. A deterministic transformation should use a utility before any
model is selected.

The role contract is enforced by output/action capabilities: Senior can submit
architecture, specification, lowering, and review artifacts, while production
implementation writes are assigned to Middle or Junior. This is stronger than
asking a Senior to remember a prompt preference and does not prevent necessary
architectural documentation.

Local capability observation: this session's native collaboration tool accepts
Sol with ultra. The public API model page lists a different set of effort
values. The design must therefore retain desired and resolved profiles
separately; it must not silently translate ultra to another API value.
[Sol API reference](https://developers.openai.com/api/docs/models/gpt-5.6-sol).

Luna/high is a proposed Junior starting point because the official model is
intended for cost-sensitive, high-volume work. Its suitability for our bounded
task classes still needs future evidence; lower price alone does not establish
lower cost per accepted result.
[Luna API reference](https://developers.openai.com/api/docs/models/gpt-5.6-luna).

## V05. Token economics begins before a worker exists

Producer and acceptor separation is bound to trusted actor and operation
identities, even when their model names coincide. The producer attempt cannot
emit acceptance for its own candidate. A later coordinator invocation of the
same model still needs separate acceptance capability and adequate evidence.
Capability observations can narrow routing; a model cannot expand its own
authority or certify itself for a higher-impact class merely by claiming skill.

The packet builder selects and deduplicates required rules, contract fragments,
source captures, local graph context, relevant previous decisions, known risks,
and selected checks. It records why each item is present, its provenance,
whether it is required or optional, and its measured or estimated token cost.
Stable content-addressed fragments can be reused without regenerating prose.

Ordinary workers receive an explicit quiet packet and exact instruction
surface. They do not read the project's full boot lane. An architectural
Senior may request a wider view, including full boot when necessary, with the
reason and cost visible. The role alone does not trigger a giant boot.
Source data, including external Markdown and XML, must not become instructions
merely because its syntax resembles specifications.

The harness may inject instructions before a packet runs. Therefore discovery
must establish whether isolated instruction loading is actually supported.
A quiet sentence is not proof that injection was suppressed. If the selected
adapter cannot provide isolation, report the real inherited context and use
an Owner-permitted alternative or a revised task shape. Do not silently launch
the same expensive boot repeatedly.

There is no universal product-imposed boot or token ceiling. Owner preferences,
actual model windows, and configured resource policies determine the packing
target. If mandatory context does not fit, split or relower the task, provide
bounded retrieval, or use a permitted stronger profile. Never truncate
essential rules to claim a successful packet build.

Economics records input, cached input when observable, output, retries,
orchestration and review cost, elapsed time, and accepted-result cost. Missing
telemetry stays unknown. A cheaper model that causes repeated rework can be
more expensive than a stronger model on the original task.

## V06. Abstraction is a typed operation, not vague anonymization

Some work can use an abstract packet: for example, an isolated graph algorithm
or a transformation over a specified algebraic data type. The packet carries
the preserved invariants, assumptions, concrete-to-abstract mapping, and
reconstruction obligations. The answer is checked against the abstract
contract, then translated back and checked against the actual consumer.

Use abstraction only when the mapping preserves the properties under review.
Authority, data migration effects, concurrency, or domain constraints must not
disappear behind generic names. Semantic equivalence of an arbitrary
abstraction is not automatically provable. If it is uncertain, retain the
necessary concrete context and route the judgment appropriately.

## V07. Strategic forks are reusable conditional knowledge

A material fork stores the question, relevant facts and unknowns, alternatives,
preconditions, expected value and cost, hazards, a recommendation with reasons,
selection authority, rejection conditions, and the next diagnostic action.
It identifies the problem independently of the task or worker name.

The executor selects a prepared alternative only while its preconditions and
authority remain applicable. A condition may evaluate true, false, or unknown;
unknown requests the named evidence rather than guessing. If no prepared
alternative fits, the agent records the encounter and requests further
refinement. It may perform an already-authorized bounded diagnostic while
independent work continues.

Do not enumerate every possible future path. Prepare high-impact uncertainty
and common failure remedies, then reuse templates and branch references.
Unselected routes are dormant options, not active obligations, queued jobs,
cost holds, or failed tasks. The viewer may show them as untraveled paths.
Switching a route preserves the old evidence, choice reason, and attempt
history. It cannot reset the Owner's two-failed-approaches condition.

## V08. Strong planning and weak execution form a round trip

The external strong-model environment produces a portable execution bundle:
strategic revision, lowered graph, required rules and sources, capabilities,
forks and recommendations, acceptance criteria, permitted decisions, stop
conditions, and content identities. It contains no cloud credential and needs
no live GitHub or external model connection to be read or executed locally.

The on-prem executor records successful candidates, selected branches,
observations, contradictions, unavailable capabilities, failed methods, and
unresolved questions in an encounter journal. It receives small packets and
uses graph queries. A weak-only installation never fabricates a stronger
reviewer; when judgment exceeds its prepared envelope, the work becomes
awaiting-refinement or an explicit capability wait, not falsely accepted.

The return bundle contains the changes since the exported revision plus
referenced evidence and unresolved encounters. Export scope is explicit;
on-prem project data is not uploaded automatically. Import verifies identity,
lineage, and artifact hashes. It reconciles against a strategic revision that
may have advanced elsewhere; it does not overwrite it with an older fork.
Conflicting findings become review inputs rather than last-writer-wins facts.

The strong model then receives a bounded report of changed assumptions and
unresolved decisions, revises strategy if warranted, and produces the next
lowering revision. The previous strategy, attempts, and findings remain
inspectable. First MVP operation can use explicit bundle exchange; it does
not require a distributed consensus service.

An entirely local installation can also use its selected weak model as the
coordinator for decisions inside its declared capability and authority. The
round trip to a stronger model is an available operating mode, not a mandatory
cloud dependency or an approval after every local result.

**Current test prohibition:** no installed local Qwen inference is invoked,
including capability pings and smoke tests. Only a future explicit Owner
command authorizes that work. Offline protocol fixtures and simulated weak
executor transcripts can validate mechanics but do not prove real Qwen
execution quality.

## V09. One algorithmic service for agents and IDEs

The Rust service should provide bounded operations such as locating a node,
searching facts, showing ancestors or dependents, explaining readiness,
enumerating a frontier, extracting an affected subgraph, comparing revisions,
reading a fork, generating a packet, and resuming an attempt. These are
conceptual operations here, not claims of existing CLI command spellings.

Every query binds a revision or snapshot, accepts an explicit scope and page
cursor, and returns completeness information. A limited traversal says what
was omitted and how to continue. No result may claim that an unseen part of a
large graph is empty. The agent can request the exact evidence or source
fragment behind a summary without loading the full campaign.

The same typed service serves CLI, native agent tool adapters, machine streams,
and future IDE clients. Public read, worker proposal, coordinator acceptance,
and Owner control routes have distinct capabilities. Structured error codes
include the violated contract, stale inputs, relevant differences, and the
next admissible operation.

Exact lookups, sorting, deduplication, dependency traversal, reference
validation, coverage, diffing, scheduling constraints, and supported policy
expressions are algorithmic. Semantic equivalence, new causal relationships,
value judgments, and sufficiency of evidence may need a model. Such judgments
enter as attributed, revisable records with sources; they do not hide inside
an apparently deterministic score.

## V10. Concurrent execution with serialized admission

The scheduler continuously admits independent ready work up to configured
executor, resource, and integration capacity. One model call does not block
dispatch, collection, or recovery of other jobs. A small serialized transaction
boundary for graph changes is compatible with many concurrent executors.

Work claims bind relevant revisions, read/write subjects, interface ownership,
resource reservations, attempt IDs, leases, and an integration owner. Different
filenames do not establish independence. Code may be isolated in worktrees or
another selected workspace mechanism; the runtime must report which isolation
it actually provides. Heavy builds and tests have their own resource queue.

A Senior reviews one completed branch while Middle implements another and
Junior performs independent bounded preparation. Dispatch is limited by the
capacity to review and integrate, so a flood of unaccepted patches does not
become the throughput metric. No unrelated branch is invalidated merely
because another worker committed progress.

Only one authorized controller should admit changes for a campaign at a time,
using a durable epoch or equivalent fencing. Recovered or duplicate controllers
cannot reuse stale admission authority. External transports that cannot fence
effects require reconciliation before reissue; ZAP cannot promise universal
exactly-once external execution.

## V11. Native subagents are the default transport

The Owner-selected subagent mechanism is used. ZAP defines a transport contract
and capability requirements without making an external launcher, provider SDK,
particular account, or CLI subprocess mandatory. Native subagents are the
default. Claude Code, Codex, OpenCode, and Qwen Code remain the primary harness
targets; support claims are per capability and evidenced adapter, not a single
optimistic supported/unsupported label.

A standalone Rust binary cannot directly call a tool that exists only inside
the enclosing agent session. The required bridge must therefore have two
halves: the Rust runtime will prepare and persist an exact dispatch intent,
and the harness driver will invoke the selected native tool and report its
actual result. Where a programmable host interface exists, the driver can be
mechanical. Otherwise a small agent-mediated bridge must perform the prepared
call and record the receipt. The inspected prototype currently provides an
argv/subprocess bridge; this native bridge is required future work.

An intent without a receipt is not a launched worker. After interruption the
bridge reconciles actual native job state before retrying. It never falls back
silently to codexrunner or another transport. If the harness cannot remain
alive unattended, the system persists a resumable wait; week-long background
execution requires an actually configured running supervisor or host capability.

## V12. A compact machine protocol

A packet has a stable packet_id, parent lowering revision, semantic-contract
identity, capability/render profile, and explicit derived_from or supersedes
links. Its byte digest protects content but does not replace lineage. Two
renders can share admitted meaning while having different bytes; a superseded
render does not erase its attempts or returned evidence.

Every message belongs to a campaign, task contract, logical operation,
attempt, packet digest, relevant input basis, role, resolved profile, and
protocol version. Stable message IDs and sequence/cursor information permit
deduplication and ordered reconciliation. Source paths and artifact hashes
are explicit references, not prose that another model must parse.

The protocol distinguishes accepted assignment, running observation,
checkpoint, evidence request, proposed branch choice, candidate result,
verification result, provider/resource wait, failure, and safe-stop receipt.
A heartbeat says the channel or job was observed alive; a checkpoint says
which useful work and effects are durably known. One is not proof of the other.

Candidate results contain changed artifacts, satisfied and unsatisfied
contract items, selected verification receipts, discoveries, unresolved
questions, proposed follow-up, and effect status. Most fields are generated
from durable state; an agent writes only missing semantic information.
Malformed responses receive precise repair feedback while preserving useful
artifacts. Formatting failure does not justify repeating successful coding.

Transport completion, verified artifacts, semantic acceptance, and safe state
are different events. Exiting with code zero does not mean the task is accepted;
a missing heartbeat does not prove that a worker or its external effect ended.

Liveness telemetry can use a compact operational table with coalesced updates.
Persist meaningful checkpoints and transitions in the semantic journal rather
than adding a literary or permanent event for every heartbeat. Repeated
observations with no material change do not each trigger replanning.

## V13. Compaction and crash survival are architectural invariants

The system persists every decision needed to continue: charter and outcome
revisions, strategic and lowered graph lineage, active fork and its grounds,
task contract, source captures, stage and deferral state, evidence, active
claims, dispatch receipts, pending effects, holds, and the next admissible
action. Records are written at meaningful action boundaries rather than at
the end of a long session.

A bounded resume view is generated from that state. It contains the current
assignment, exact accepted boundary, unresolved effects, relevant decisions,
applicable rules, pending Owner conditions, and navigation handles. It is
available to a new model, account, process, or compacted session. Recovery
rechecks actual workspace and transport state; a summary is not authority.

No claim is made that unrecorded internal reasoning survives. The guarantee
is that accepted state and committed decisions survive and that the agent can
reconstruct the next valid step from evidence. An interrupted unrecorded idea
may be recomputed. Code or other work already performed is inspected before
being repeated.

Use write-ahead intent for external effects and collect durable outcomes. A
crash between launching a worker and recording its ID produces an unknown
outcome to reconcile, not permission to launch a duplicate. Quota exhaustion,
provider overload, a product defect, and a rejected request have different
retry rules. Backoff, retry ownership, and attempts survive account changes;
the runtime respects provider restrictions and uses only configured authorized
credentials. A context reset never resets an architectural failure counter.

## V14. GOAL.md reinforces the protocol

GOAL.md is a deterministic projection of the active charter, valuable outcome,
current scope, explicit stop conditions, and required completion evidence.
It points to the resume/query interface. It is regenerated on relevant changes,
not rewritten by a model after every event. Each worker also has a scoped goal
derived from its contract.

For a harness with one goal, use a concise campaign umbrella goal and references
to active assignments. Do not concatenate the complete text of every child
task into a growing context. A harness with per-agent goals may apply the
scoped projections in addition to the umbrella goal.

Capability discovery records goal read/create/update/clear support, whether
application is agent-callable or Owner-only, goal scope, native worker support,
instruction isolation, structured output, liveness, cancellation, and actual
model/effort support. Prefer exposed tool metadata or static configuration;
do not spend an inference call discovering what is already known.

Cache discovery under harness/toolset/version/configuration identity and
invalidate it only on material change or evidence of staleness. Once per
effective environment is useful; once forever is unsafe, and once per worker
or turn wastes tokens. Local inference discovery remains prohibited now.

When supported and authorized, the adapter applies the goal and records an
acknowledgment of the actual revision. In Owner-only mode it produces the exact
manual instruction once per relevant goal change. Unsupported mode uses the
packet/resume contract without claiming a native goal was set. An unfinished
native goal must never be marked complete merely to replace its text. Native
goal state is reinforcement, not a substitute for ZAP's acceptance and holds.

## V15. Dreamer mode uses isolated planning branches

The conversational entry point distinguishes a live scope-change instruction
from a request to explore a possibility. Explicit add/remove requests prepare
a scoped proposal for the live plan. “Imagine”, “simulate”, “think through”,
and similar requests create a hypothetical branch from an exact live revision.
If attachment to the root or a subgoal is unclear, ask where it belongs.

Ask whether the new goal should be grilled. If yes, resolve the material intent
questions before admission: who benefits, observable success, essential
constraints, non-goals, tradeoffs, and consequential forks. Use existing facts
first; ask plain-language choices with a recommendation. Discovery of factual
information is work for the system, not a questionnaire for the Owner.
Answers and unresolved questions persist across compaction.

A planning branch contains only its delta over a base revision and the
associated assumptions, questions, estimates, and alternatives. It can add,
remove, move, or replace proposed goals; calculate the affected dependency,
knowledge, value, evidence, and lowering changes; and show consequences.
Simulation does not dispatch work, alter the live frontier, consume live
approvals, or put live work on an economics hold.

When the Owner explicitly applies a proposal, the service verifies the current
base, resolves relevant concurrent changes, checks authority and cost, and
reconciles affected live work. A stale hypothetical branch does not overwrite
new live progress. Unaffected work and valid proof survive. One accepted
decision is reused within its exact scope rather than being asked repeatedly.

Where a scope change needs both a charter amendment and an above-threshold
decision, one exact Owner response can bind both prepared results. An earlier
request to explore or add an idea does not approve costs discovered later, but
the system must not ask twice for an identical already-reviewed envelope.
Hypothetical planning branches, strategic options, execution sandboxes, and
Git branches are distinct typed concepts.

Removing a goal records the disposition of its obligations, dependent work,
evidence, artifacts, and outstanding effects. It does not erase history or
remove a necessary prerequisite still used elsewhere. “Unzap” may propose a
smaller outcome; achieving that outcome is reported as an authorized revision,
not retrospective success at the original goal.

## V16. Adaptation, uncertainty, and change economics are one loop

Every material plan delta reuses the existing adaptive and economics model.
Known dependency closure and source invalidation are computed. A bounded
semantic pass examines potentially missing links, changed value, new unknown
regions, and plausible consequences outside the old graph. Fog can increase
after learning; “irrelevant to this route” is not “known”.

Compare alternatives against the same approved baseline. Include new
implementation, tests, migration, documentation, invalidated proof, consumers,
operations, and bounded uncertainty. Retained approved work and sunk cost are
not charged again. Estimation itself is bounded and records what remains
unknown; an analysis task should not cost as much as the decision it informs.

Keep utility and cost separate. Reject expensive optional work with little
benefit; prefer an evidenced cheaper method that preserves the same required
benefit. A high-value change can be worth substantial effort. A mandatory
problem with no acceptable cheaper remedy remains recommended. Necessity does
not grant authority to bypass the Owner's threshold.

The confirmed default is Owner review above four hours of expected elapsed
time to a verified result at the meaningful configured team capacity. Exactly
four hours is not above the threshold. Total agent-hours, passive waiting,
uncertainty, and engineering consequences remain separate diagnostics.
Unknown impact cannot be treated as free. Cumulative forecasts prevent
splitting one expensive change into repeated small allowances. Temporary
provider overload is operational waiting, not invented engineering complexity.

A selected live change requiring the Owner holds affected work and dependent
starts while safely reconciling in-flight work; proved-independent work may
continue. A general Owner “stop” still pauses the whole campaign by default.
Dormant forks, hypothetical changes, and rejected alternatives create no live
hold. Above-threshold review applies to a new change's incremental scope; it
does not repeatedly reauthorize an already accepted baseline campaign.

## V17. Enforce transitions at the service boundary

The official mutation service checks typed contracts, exact authority,
relevant input versions, readiness, policy, economics admission, required
reviews, evidence applicability, stage duties, and live-effect reconciliation.
Scheduler and clients use the same predicates. The agent cannot gain authority
by writing “Owner approved” in a result or moving a row to done.

Mechanical enforcement covers protocol and admissible state transitions.
Truth of a semantic assessment still depends on evidence and appropriate
review. Stronger isolation can restrict workers to proposal channels and their
assigned workspace. With unrestricted filesystem privileges, ZAP cannot make
an agent physically unable to bypass every utility; deviations must instead
remain unaccepted and visible. No prompt is described as a security boundary.

Campaign closure uses one shared predicate across runtime and direct command
routes. It considers active obligations, integration, applicable deferrals,
pending selected changes and Owner decisions, incomplete admitted effects,
holds, and unresolved external outcomes. An empty ready queue is not success.
Dormant alternatives and hypothetical branches do not block unrelated closure.

The existing Python economics candidate has an open P1: completion readiness
and direct campaign close can overlook economics holds. Preserve it as a Rust
acceptance requirement, not behavior to port. See
[REVIEW-CE-INTEGRATION.md](../../development/zap/REVIEW-CE-INTEGRATION.md).

## V18. Production Rust and storage architecture

All ZAP-owned production executable source should be Rust: domain kernel,
storage, query/index engine, scheduler, protocol validation, CLI, backend, and
executable adapter helpers. English prompts/specifications and XML/JSON/TOML
data remain data. Python may remain in explicitly nonproduction prototype or
investigation material; normal installation and execution must not need it.

Use a package-owned Cargo workspace with a pure domain layer, transactional
application layer, storage/query layer, runtime/adapters, and thin entrypoints.
These are architectural boundaries, not a demand to create a separate crate
for every small concept. Existing VibeVM rules for production Rust, public
contracts, and provenance apply during productization.

Recommended storage baseline for design: an embedded transactional database
with an append-only logical event journal and materialized graph/index tables
updated in the same transaction. Prefer a package-owned embedded Rust store;
redb is the current candidate pending a focused storage ADR and compatibility
checks, not a measured winner or a mandatory external service. Its official
documentation describes a Rust transactional key-value engine with concurrent
readers and crash recovery; that makes it worth evaluating, not automatically
correct for ZAP. [redb documentation](https://docs.rs/redb/latest/redb/).
Content-addressed large artifacts live outside rows with a recoverable
publication protocol.
Portable JSONL/XML exports are derived from committed revisions; avoid dual
authorities in separately written files and tables.

The preferred commit point is one database transaction containing the immutable
logical event, idempotency result, and required current-state indexes. The event
sequence remains semantic authority; indexes remain rebuildable projections.
An alternative is an independently durable segmented journal followed by a
derived database at an explicit replay cursor. That can preserve the old storage
shape but introduces a second recovery boundary. Choose one model in the ADR;
never describe both independent writes as a single atomic commit.

Index stable identities, typed edges in both directions, source-to-dependent
links, readiness, evidence applicability, and open encounters. Maintain scoped
generation/fingerprint information and incrementally update affected views.
Do not replay the whole journal on each CLI command or duplicate the entire
plan for each Dreamer branch. Snapshots and caches bind exact input identity,
schema, reducer/query version, and committed cursor. Stale caches are replaced,
not trusted because their timestamps look recent.

Do not materialize every transitive closure eagerly; it can grow quadratically.
Keep adjacency, reverse dependencies, local blocker counts, and incremental
invalidation, with more elaborate reachability indexes justified by measurements.
The existing Python search/frontier/history paths include whole-store scans;
the current small pilot provides no evidence of the proposed large-scale speed.

Cold-start integrity is a deliberate design choice. Existing prototype snapshots
rederive state from canonical history on cold load. Retain that check for legacy
import and explicit distrustful audit. For normal operation, the proposed Rust
store uses crash-safe transactions and a version-bound durable checkpoint under
the documented trusted-local-store boundary. The store and checkpoint must sit
outside every worker write subject and supported writes must be mediated by
the trusted Rust service. A version label or worker promise alone is insufficient.
Deployments without that confinement need an independent trust anchor or a
distrustful derivation audit before trusting stored projections. Arbitrary
same-user or OS modification remains outside the claimed isolation boundary.
This permits fast reopen in the declared trusted mode without
claiming independent verification of all historical bytes on every launch.
Detecting arbitrary malicious rewrites of every local file needs an independent
trust anchor or a complete audit. A self-hash alone supplies neither. The exact
checkpoint trust model must be settled in the storage ADR before implementation.

Rust alone does not make full-graph traversal instantaneous. Small queries
should be interactive; large impact closures, initial imports, and index
rebuilds may require streaming progress and resumable work. A proposed future
benchmark envelope is 100,000 work/knowledge nodes, 1,000,000 edges, and a long
event history, with warm bounded queries targeting p95 below 100 ms on a named
reference machine. This is a hypothesis to measure, not a promise already
verified. Cold start, write latency, memory, storage growth, and large-fanout
invalidation must be reported separately.

## V19. VibeVM integration without dependence on its source checkout

ZAP must consume project capabilities: spec and fact resolution, source identity,
package metadata, rule selection, action descriptions, graph queries, and
verification target discovery. It should use stable VibeVM protocol/SDK seams
where available, behind adapters where they are not. Do not copy a second
complete fact engine or assume that another project's root has VibeVM's
internal Cargo layout.

The bounded source review found useful fact witness and source-address APIs,
indexing patterns, client projection helpers, and wire-generation patterns.
It also found that the current host graph crate is a stub, client projection
does not launch native agents, and the current LLM seam is a synchronous text
request rather than a durable agent host. These are reusable ideas or adapter
inputs, not evidence that the required ZAP runtime already exists. AgentHost
and SemanticProvider must remain distinct contracts. Direct linking to current
unpublished host crates would couple the installed package to this checkout;
use stable installed interfaces or properly distributable dependencies instead.

The target package must be installable into any VibeVM project. Creating or
promoting the necessary stable installed interfaces is part of future work;
the current host crates do not establish this capability. Paths, project identity,
user-local state, toolchain and verification commands are discovered or bound
from that project. VibeVM's own campaign IDs, 425-node pilot, local Windows
paths, and repository-specific commands belong to fixtures or configuration,
never to the generic runtime. Rootless home environments and Docker execution
are valid workspace adapters; a full Linux distribution is not a ZAP MVP
requirement. Networked services can contribute typed capability and state
observations through the same model.

An offline installation can import local bundles and run local tools with no
GitHub account or reachable central service. A configured GitHub registry is
one distribution route, not a runtime dependency or data model assumption.

## V20. Extreme total observability supports the future canvas

Every important public state and internal diagnostic has a versioned machine
view: strategy and execution links, lowering provenance, stages, debt, fog,
sources, proof applicability, forks, costs, role routing, packet composition,
native dispatch receipts, liveness, retries, decisions, holds, and invalidation
reasons. Secrets and private credentials remain outside public projections.
Semantic rationale is recorded; private unrecorded model reasoning is not
invented or required for replay.

The future canvas can query semantic levels of detail, expand a region,
navigate dependencies, search, focus a path, inspect a node in place, compare
revisions, and follow an event to affected nodes. Snapshot-plus-event-tail
delivery is cursor-consistent and detects gaps. Opening the viewer does not
call a model to reconstruct missing explanations.

Fog, readiness, maturity, risk, cost, selected/unselected routes, and acceptance
are independent axes. They must not collapse into one green/red status. The
Owner's Heroes III / StarCraft direction remains: bright explored regions,
legible uncertainty and untraveled paths, meaningful geometry, and rich click
details on a serious large-graph canvas.

The Rust requirement applies to ZAP's production implementation. The previously
requested Qwik viewer is a separate future client of this protocol, outside
the Rust package. This proposal preserves that separation; no viewer or IDE
plugin is implemented or scheduled here.

## V21. Verification economy and honest MVP evidence

Each work contract names its verification scope before execution. The kernel
selects affected units, material negative cases, invariants, and known interface
consumers from source/contract relationships. Unknown coverage is explicit.
The responsible reviewer may broaden scope for a named concern; a task, commit,
stage change, or new worker does not automatically trigger the full panel.

Evidence is keyed by relevant code, contract, toolchain/environment, inputs,
and required guarantees. Reuse it while applicable. Run a check again only
after a relevant change, failure, expiry, or unresolved concern. A full panel
is exceptional: a genuinely required release/final gate or a justified
system-wide uncertainty. Mutation testing requires a concrete suspicion and
an expected decision benefit; it is not a default sweep.

The MVP acceptance story should include these end-to-end outcomes, using
reusable evidence rather than multiplying nearly identical tests:

| Scenario | Evidence required later |
| --- | --- |
| Parallel native work | At least two independent jobs progress; one waiting model does not freeze the other; conflicts and acceptance are respected |
| Compaction/crash/quota loss | Resume reconstructs a correct next step, preserves holds and source identities, and reconciles a possibly completed external effect before retry |
| Lowering and adaptive change | Strategy survives; changed premises invalidate only affected execution work; required meaning and applicable evidence remain traceable |
| Weak execution round trip | Export, bounded navigation, prepared fork selection, encounter recording, return import, and relowering preserve lineage; simulation is labeled as simulation |
| Dreamer scope work | Hypothetical changes leave the live campaign untouched; application resolves concurrent drift; removal preserves obligation history |
| Economics and stop | Above-threshold live change holds the right scope; campaign stop dominates; neither direct close nor scheduler bypasses unresolved decisions |
| Generic project | Installed ZAP works in a small independent VibeVM project without access to VibeVM source paths or Python |
| Large graph | Bounded queries and incremental updates meet measured targets; expensive operations disclose progress and incomplete results |

Writing the vision itself did not authorize execution. The subsequent Owner
acceptance now authorizes implementation and necessary scoped verification.
Installed local Qwen inference still requires a future explicit Owner command.

## V22. Migration and the path to the agent MVP

The Python prototype supplies experience with trusted control, journaled
recovery, proof applicability, adaptive changes, transport receipts, and
machine views. Preserve those semantics and their evidence. Do not port file
layout, incidental Python APIs, global scans, or known defects mechanically.

After design review, the implementation campaign should first reconcile the
normative XML with this architecture and record the few unresolved technical
ADRs. Then construct a Rust vertical slice with durable state, bounded queries,
packets, native dispatch receipts, acceptance, and recovery. Add capability-aware
lowering/roles, portable weak execution bundles, and Dreamer scope changes to
that same kernel. Finalize generic installation and the VibeVM pilot at the
accepted integration boundary. These are outcome gates, not a newly scheduled
task tree in this document.

Migration imports into a new store and preserves the original bytes, IDs,
contracts, obligation mappings, evidence, and authority classifications.
Legacy canonical JSON hashing cannot be assumed identical across Python and
Rust: escaping, numeric rendering, ordering, and invalid values need an
explicit versioned codec/import rule. Keep original digest domains and an
auditable mapping; do not silently rehash old history and call it unchanged.

Use an explicit new store/codec epoch, provisionally zap/2, instead of changing
zap/1 beneath existing hashes. Legacy compatibility must account for tagged
date/time and non-finite values, duplicate-member refusal, reserved tag keys,
Unicode edge cases, trailing newlines, and machine-local path spelling. New
portable identity separates logical source handles and raw-content identity
from a machine's local locator. Reducer identity must cover the actual semantic
implementation epoch, not merely a list of registered event names. Preserve
legacy replay as history; a deliberate correction such as the closure P1 must
not be misreported as byte-for-byte unchanged behavior.

Validate imported state before switching the configured store pointer. Keep
recovery material. Existing dormant NEXT migration data remains dormant;
importing or upgrading ZAP never activates NEXT. Current unaccepted economics
edits are preserved as candidate evidence, not silently blessed by the port.
Package version remains 1.0.0 under the Owner's prototype policy; internal
schema/protocol versions may change explicitly when necessary.

The finished package should run without production Python. Temporary campaign
specifications disappear only after their requirements, unique decisions,
source provenance, and durable evidence have moved into normal ZAP/VibeVM
specifications and project records. Deleting a temporary file must not delete
the only explanation of an architectural choice.

## V23. Relationship to existing contracts

| Existing foundation | Treatment in this vision |
| --- | --- |
| ZAP-METHODOLOGY.xml | Retain intent/charter, Owner stop semantics, obligations, recursive stages and central acceptance; make role, packet and lowering mechanics explicit |
| ZAP-ADAPTIVE-CYCLE.xml | Retain bounded feedback, nonmonotonic fog, value/feasibility review, authorized outcome revisions and live reconciliation |
| ZAP-DATA-AND-VIEWER.xml | Retain machine-first provenance, graph views and event replay; add lowering, packets, capabilities, encounters and Dreamer overlays as inspectable subjects |
| ZAP-CHANGE-ECONOMICS.xml | Retain the four-hour policy and incremental attribution; apply coherently to admitted live changes and preserve pending-decision closure obligations |
| ZAP-RUNTIME.xml | Replace Python production and reference-launcher assumptions after review; retain pure domain/service/effect separation and explicit capability evidence |
| Multi-user-planning | Preserve imported contracts, permanent obligations, local continuity, scoped verification and change/migration documentation lineage |

The narrow technical choices still needing evidence are the embedded store
implementation, precise current VibeVM adapter seams, each harness's native
capabilities, and achievable performance on declared hardware. None requires
the Owner to choose a database or solve an implementation detail now. A future
material decision outside the agreed intent or cost policy is presented as a
concrete choice with consequences.

## V24. Current disposition

This vision and the exact Owner source are saved. The Owner accepted the complete
vision and instructed implementation with frequent durable checkpoints. The
execution plan is ../../development/zap/rust-campaign/PLAN.md; its campaign.json,
RESUME.md, packets and checkpoints carry actual progress. ZAP implementation and
scoped verification are active, and the earlier publication authorization
continues. NEXT product execution and installed local Qwen inference remain
prohibited. Stop after the accepted implementation/publication boundary and
await further Owner improvements.
