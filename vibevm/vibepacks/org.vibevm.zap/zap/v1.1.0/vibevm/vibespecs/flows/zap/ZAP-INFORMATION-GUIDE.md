# ZAP information opportunities {#root}

`guide r1`

This guide covers the Rust domain surface for durable information opportunities. An opportunity describes information that could improve a material decision. It is separate from a `Resource`: sources and evidence can be reused, while execution capacity remains subject to ordinary claims and reservations.

## Opportunity {#opportunity}

`guide r1`

`InformationOpportunityRecord` has a stable `InformationOpportunityId`, content and two fingerprints. The semantic fingerprint binds the complete declared opportunity. The basis fingerprint binds the current decision, region and source records that make the opportunity applicable. The record revision supplies compare-and-swap control.

The content names the decision, at least two competing possibilities or an explicit unknown condition, the observation sought, source captures and applicability, acquisition, verification, coordination and delay intervals, decision benefit, and a stopping rule. Existing `RegionId`, strategic forks, `BoundedHorizon`, `CostUnknown`, source captures and evidence IDs are reused. `InformationDecisionBasis::Unresolved` preserves a named decision whose canonical backing is not yet supported; the selection query reports it as unknown and lowering cannot consume it.

Benefit uses either avoided agent hours or avoided elapsed hours. This gives the selection algorithm compatible units for its comparison. A benefit that cannot be expressed honestly in either unit is `DecisionBenefit::Unknown` with a resolution action. The API does not turn qualitative prose or model confidence into a score.

`InformationOpportunityProposed` is a `DataProposal`. Creation requires `expected_opportunity_revision = None`; revision requires the exact stored revision. The caller also supplies the current basis fingerprint returned by `information_opportunity_basis`. A stale decision, region, source capture or applicability record refuses the command. The command persists only the opportunity. It creates no `WorkRecord`, contract, packet, job, completion blocker or authority.

## Commands {#commands}

`guide r1`

The two registered data commands are `InformationOpportunityProposed` and `InformationSelectionProposed`. Both use canonical versioned payloads, exact command idempotency and record CAS. Neither command grants authority to materialize or execute Work.

## Query {#query}

`guide r1`

`InformationOpportunityQuery` is registered as `zap.information.opportunities.v1`. Its input names one `DecisionId`, a page limit, an operation budget and an optional revision-bound cursor. A committed decision-partitioned index supplies only opportunities for that decision; unrelated optional questions do not consume the query budget. The query charges index rows and exact opportunity loads separately and refuses with `LimitExceeded` when the requested decision partition cannot establish the complete peer set needed for dominance. It never labels an unseen peer harmless.

Each recommendation is one of:

- `Worthwhile`: the lowest declared benefit exceeds the highest bounded compatible acquisition, verification, coordination and delay cost.
- `NotWorthwhile`: the highest declared benefit does not exceed the lowest compatible cost.
- `CheaperDecisive`: another opportunity covers the same exact decision basis and alternative meanings, distinguishes the full decision, and has strictly lower bounded cost.
- `Dominated`: another exact-decision peer has no worse ranges and at least one strict benefit or cost improvement.
- `AlreadySatisfied`: retained evidence passes the existing current-proof validation and covers the opportunity's exact source captures.
- `AlreadySelected`: the opportunity already points to its stable candidate Work identity.
- `Unknown`: ranges overlap, a material cost unknown remains, benefit lacks compatible units, or the decision/source basis is stale or unsupported.

No recommendation contains a scalar score or probability. Equal ranges do not mutually dominate. The operation budget meters decision-index rows and exact opportunity loads. `InformationBasisEvaluationCost` separately reports exact decision, source, applicability and region records decoded to verify each fingerprint. `InformationProofValidationCost::StoreWideCurrentProofContextScan` discloses when `AlreadySatisfied` required the existing broader current-proof helper; its work/source/contract/evidence context scan is also separate from the query's opportunity operation count.

## Selection {#selection}

`guide r1`

`InformationSelectionProposed` is also a `DataProposal`. It records the exact query basis, a candidate `WorkId`, `WorkType::Evidence` or `WorkType::Decision`, and the rationale. It grants no execution permission and does not insert the candidate Work. A first selection must precede that Work's materialization. A caller may select either `Worthwhile` or an honest `Unknown` verdict with a nonblank reason when the supported current decision and sources remain applicable; ordinary lowering, economics and authority still decide whether the proposed investigation becomes executable. Stale, unsupported, dominated, already-satisfied and already-selected opportunities cannot use this path.

The selection cell recomputes the deterministic recommendation and source basis. It rejects a verdict that differs from the current result, stale revisions, unsupported decision/source applicability, a different work type, a second selection for the same acquisition, and a retry that changes the stable candidate Work identity. The opportunity and selection records are updated atomically, so bounded reads can follow the opportunity-to-selection association without scanning all selections.

## Execution {#execution}

`guide r1`

Ordinary admitted lowering remains the only product path that creates selected research or decision Work. Planning supplies each selected information rationale to `validate_lowering_information_bindings`. The helper requires the exact current selection, opportunity, decision and source fingerprints; checks the selected `WorkId` and type; refuses duplicate selection IDs in one lowering; and preserves an existing Work identity across retries.

`selected_information_execution_context` returns the exact snapshot that lowering and packet construction retain: selection revision, Work identity and type, opportunity and basis fingerprints, source captures, retained evidence, the structured stopping rule, its fingerprint, and a deterministic safe-stop boundary.

`InformationStopEnforcement::ContractBoundary` means lowering must require that boundary as the ordinary `TaskContract.safe_stop`; packet derivation then carries it through the existing candidate-result safe-stop contract. `DecisionGuidance` is stored and displayed as decision guidance. It is not described as a runtime-enforced attempt or time limit.

An exact repeated command uses the service's ordinary idempotency receipt. A new command with stale CAS fails. Runtime retries retain the same selected Work and ordinary attempt lineage rather than creating another task. Source, applicability or decision drift makes the selection stale, and lowering refuses it until the opportunity and selection are reconsidered.

Optional opportunities add no completion provider. Required knowledge blocks completion only through an existing milestone obligation, proof or acceptance contract. An observation may discover additional regions or uncertainty without being mislabeled as failed acquisition.

## Minimal Rust use {#minimal-rust-use}

`guide r1`

```rust
use zap_domain::information::{
    InformationOpportunityProposed, InformationOpportunityProposedSchema,
    InformationOpportunityQuery, InformationOpportunityQueryInput,
};
use zap_core::QuerySpec;

assert_eq!(
    <InformationOpportunityQuery as QuerySpec>::ID,
    "zap.information.opportunities.v1"
);

// Build typed content from current Decision/Region/Source records, then obtain
// information_opportunity_basis(snapshot, &content). Persist the proposal through
// the ordinary CommitService DataProposal route. Query the bound DecisionId before
// proposing a selection; selection itself still creates no executable Work.
let _schema = InformationOpportunityProposedSchema::V1;
let _ = std::mem::size_of::<InformationOpportunityProposed>();
let _ = std::mem::size_of::<InformationOpportunityQueryInput>();
```
