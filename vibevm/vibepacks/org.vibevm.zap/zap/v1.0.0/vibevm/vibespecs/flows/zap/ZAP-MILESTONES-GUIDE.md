# ZAP milestone domain guide {#root}

`guide r2`

This guide describes the shipped Rust domain surface for canonical milestones.
The normative contract is
[`ZAP-MILESTONES.xml`](ZAP-MILESTONES.xml). A milestone is an independently
accepted observable result. It is not a `WorkRecord`, hierarchy container, task
counter, or execution permission.

## Records {#records}

`guide r1`

`MilestoneRecord` is the small mutable head keyed by `MilestoneId`. It points to
the exact current `MilestoneRevisionRecord` and, when one exists, the most recent
`MilestoneAchievementRecord`. The head revision is the compare-and-swap value.

Every `MilestoneRevisionRecord` is immutable and keyed by a unique
`MilestoneRevisionId`. Its `previous_revision_id` preserves lineage. Its
`MilestoneDefinition` binds the exact strategic revision and outcome, a name,
purpose, observable result criterion, consumers, required obligations, typed
contributions, typed milestone prerequisites, and lifecycle. A Work
contribution may name a strategic node before executable Work is materialized.
Such a future contribution remains visible in the definition and is excluded
from the affected running-Work scope until its `WorkRecord` exists.

The semantic fingerprint covers the complete definition. The proof fingerprint
covers result meaning, outcome, consumers, obligations, and achievement
prerequisites. It deliberately excludes display wording, the current Work
decomposition, preparation-only prerequisites, and lifecycle. An admitted route
revision can therefore preserve a current achievement when its proof contract
is unchanged. A changed result or achievement prerequisite requires
revalidation. Strategy provenance remains in every revision and receipt; an
explicit admitted revision performs any transfer to another strategy.

## Relationships {#relationships}

`guide r1`

`MilestoneContribution` distinguishes `Work`, accepted `Evidence`, and another
exact milestone revision. Shared Work may contribute to several milestones;
identity prevents duplicate execution or accounting. `MilestoneDependency`
distinguishes a preparation prerequisite from an achievement prerequisite and
binds the referenced milestone's exact revision and semantic fingerprint.

Consumer subjects are limited to outcome-scoped Outcome, Obligation, Work, and
Decision references that the domain can validate. A foreign Work or obligation,
an unsupported subject kind, an inactive cross-milestone reference, or a stale
revision binding is refused.

## Commands {#commands}

`guide r1`

`MilestoneCreated` (`milestone.created`) creates one stable head and its first
immutable revision. `affected_work_ids` is the exact sorted subset of Work
contributions that are already materialized. Initial creation uses the
registered initial-milestone-plan classification only while the active charter,
intent, outcome, exact candidate strategy, absence of an adopted milestone
plan, and absence of lowering establish the initial baseline. Dynamic creation
after plan adoption or lowering is a semantic change and uses normal economics
admission.

`MilestoneRevised` (`milestone.revised`) binds the head revision, current
revision identity, and current semantic fingerprint. Direct revision is
additive: its `MilestoneConservation` must repeat all prior obligations,
consumers, contributions, and dependencies, and those commitments must remain
present. It cannot pivot outcome, reactivate retirement, or remove route data.
Use a typed transform for a destructive or structural change.

Both commands use privileged planning authority, an exact relevant basis, a
registered semantic effect contract, deterministic simulation, and existing
economics admission. Reusing the same `CommandId` with the exact payload returns
the stored idempotency result. Reusing it with another payload, using stale head
CAS, or changing a simulated payload is refused.

## Achievement {#achievement}

`guide r1`

`MilestoneAchievementAccepted` (`milestone.achievement-accepted`) is a proof
operation under existing acceptance authority. It binds the exact head,
milestone revision, semantic fingerprint, proof fingerprint, evidence IDs, and
relevant basis. The reducer uses the existing current-proof machinery, including
candidate provenance, contract, Work validation generation, source captures,
source applicability, observed pass, and exact evidence basis. Evidence must
belong to the milestone outcome and collectively cover every required
obligation. Achievement prerequisites must themselves have a current receipt at
the exact bound revision. Completed children alone supply none of this proof.

The resulting `MilestoneAchievementRecord` is immutable. It records the exact
evidence, obligations, outcome and strategy provenance, acceptor, semantic and
proof fingerprints, and relevant basis. Recording a new achievement only moves
the head's latest-receipt pointer; older receipts remain queryable by ID.

## Current validity {#current-validity}

`guide r2`

Historical achievement and current validity are separate. The
`milestone_achievement_validity` helper and registered queries report `Current`,
`NeedsRevalidation`, `Retired`, or `Unavailable`. They compare the current proof
contract, active outcome and obligations, exact prerequisite bindings, accepted
evidence provenance, source captures, and relevant basis. Unrelated journal
commits and the receipt's own insertion do not change that basis. A route-only
revision with the same proof fingerprint can retain validity. Relevant source,
outcome, obligation, evidence, result, or prerequisite drift preserves the old
receipt and reports that revalidation is needed.

## Queries {#queries}

`guide r1`

`MilestoneReadInput` selects one milestone and optionally evaluates its latest
receipt. `MilestoneAchievementInput` retrieves any historical receipt directly
and optionally evaluates it against current state. Both are exact, revision
bound registered queries. The current proof helper presently scans the current
Work, active-contract, source, and acceptance context; responses disclose
`StoreWideCurrentProofContextScan`. The exact milestone and receipt reads are
bounded, but clients must not describe the proof component as a bounded graph
lookup.

`MilestoneRevisionInput` reads any immutable historical revision by
`MilestoneRevisionId` and reports whether it is still current.
`MilestoneTransformInput` reads the immutable transform by `OperationId`,
including the complete original plan, dormant contributions, dependency
dispositions, before/after revision identities, and preview digest.

## Revision conservation {#revision-conservation}

`guide r1`

Direct revision cannot remove a prior commitment. This deliberately makes the
safe, common path small and keeps removal semantics out of an arbitrary payload
authorization string. Scope removal, route replacement, split, merge,
contribution movement, and retirement use the typed transformation protocol.

## Transforms {#transforms}

`guide r1`

`MilestoneTransformPlan` supports `Split`, `Merge`, `MoveContribution`,
`RouteChange`, and `Retire`. Each plan has an `OperationId`, exact CAS for every
changed existing milestone, unique successor revision IDs, complete materialized
affected Work and subject scopes, explicit dormant contributions, explicit
dispositions for every removed dependency edge, and a reason.

The `zap.milestone.transform-preview` query validates the complete proposed
after-state in a bounded in-memory overlay. This permits successors and rewritten
dependents to bind each other's new exact revisions without impossible
successor-before-creation writes. It detects cycles, stale or foreign bindings,
omitted dependent rewrites, dangling references, lost obligations, consumers or
contributions, and incomplete affected scope. It reports added and removed
owned contributions and dependency edges, proof-retained milestones, and a
digest of the exact before state and plan. Finding dependent milestones
currently uses a store-wide milestone-head scan and the preview says so.

Split and merge conserve the union of obligations, consumers, and contributions.
An internal prerequisite between merged sources may disappear only through an
explicit `CollapsedInto` disposition. External prerequisite edges must be
remapped to an exact added edge. Route changes preserve result, outcome,
obligations, consumers, and historical revisions; removed contributions are
listed as dormant, and every removed dependency is remapped or recorded as a
dormant route. Changing an achievement prerequisite changes the proof
fingerprint and makes an old receipt require revalidation. Retirement into an
existing successor conserves the union of the retiring boundary and that
successor, so the successor's independent commitments cannot be overwritten.

`MilestoneTransformApplied` binds the preview digest. Its privileged semantic
effect re-runs preview validation during simulation and application, then
atomically inserts every immutable successor revision, moves the affected heads,
and stores one immutable `MilestoneTransformRecord`. It does not dispatch Work,
rewrite historical events, release effects, or grant campaign completion.

## Minimal use {#minimal-use}

`guide r1`

```rust
use zap_domain::milestones::{
    MilestoneContribution, MilestoneCreated, MilestoneCreatedSchema,
    MilestoneDefinition, MilestoneLifecycle,
};
use zap_wire::{MilestoneId, MilestoneRevisionId, SubjectRef};

let command = MilestoneCreated {
    schema: MilestoneCreatedSchema::V1,
    milestone_id: MilestoneId::parse("milestone.release")?,
    revision_id: MilestoneRevisionId::parse("milestone.release.r1")?,
    affected_work_ids: Vec::new(), // strategic-only contribution is not running Work
    definition: MilestoneDefinition {
        strategic_revision_id,
        strategic_record_revision,
        strategic_semantic_digest,
        outcome_id: outcome_id.clone(),
        outcome_revision,
        name: text("Verified release")?,
        purpose: text("Give consumers an installable checked package")?,
        result_criterion: text("The release obligations have current accepted proof")?,
        consumers: vec![SubjectRef::Outcome(outcome_id)],
        required_obligation_ids: vec![release_obligation],
        contributions: vec![MilestoneContribution::Work { work_id: strategic_work }],
        dependencies: Vec::new(),
        lifecycle: MilestoneLifecycle::Active,
        retirement_reason: None,
    },
};
# Ok::<(), zap_wire::ZapError>(())
```

The application submits this payload through the ordinary authenticated commit
service with the registered basis and admission providers. Constructing or
serializing it does not create authority.
