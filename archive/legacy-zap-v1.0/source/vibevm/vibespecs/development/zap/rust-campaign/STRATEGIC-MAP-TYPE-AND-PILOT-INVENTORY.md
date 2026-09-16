# Strategic-map type and pilot inventory

Read-only English inventory; dependency-kind correction verified at
2026-09-14 13:11:30 UTC. Existing ZAP already separates
campaign outcomes, strategic work, executable contracts, constrained
capacity, evidence and uncertainty. The proposed town/task/resource vocabulary
crosses those existing boundaries; it is not yet an accepted new algorithm.

The [Owner commission](../../../research/zap/ZAP-STRATEGIC-MAP-FOLLOWUP-2026-09-14.md)
and its later amendment authorize analysis/design followed by discussion.
No product, canonical graph, pilot task, migration, metadata or publication
change is made here. The Owner's effectiveness hypothesis is not empirical
evidence that the game metaphor improves neural-agent planning.

## Existing type support

The table lists relevant actual fields, not an exhaustive public API catalog.
Record construction does not by itself activate authority or perform a
registered transition.

| Existing concept | Actual types and fields | Meaning already available |
| --- | --- | --- |
| Intent and campaign authority | [IntentRecord and CharterRecord](../../../../../crates/zap-domain/src/intent/records.rs): intent revision/predecessor, summary, beneficiaries, values, constraints, source_refs, fingerprint and owner_binding; charter campaign/intent/outcome binding, allowed_actions, mutable/essential obligations, completion_duty_policy, status and digest | A valuable direction and an exact authority envelope. A town label does not replace these bindings. |
| Observable outcome | [OutcomeRecord](../../../../../crates/zap-domain/src/intent/records.rs): outcome_id, revision, previous_outcome_id, intent_id, summary, benefits, guarantees, tradeoffs, proposed_obligations, required_final_gate_evidence_ids, required_promotions and their dispositions | A revisioned result linked to intent and completion duties. It carries more authority/acceptance meaning than a visual point of interest. |
| Strategic structure | [StrategicPlanRecord](../../../../../crates/zap-domain/src/lowering/records.rs) binds intent/outcome, previous strategy, nodes, obligations, forks, risks, integration conditions and semantic digest. [StrategicNode](../../../../../crates/zap-domain/src/lowering/model.rs) has work_id, title, obligation_ids, depends_on and refinement_trigger | A strategic layer already survives executable lowering. Its nodes use WorkId rather than a separate MilestoneId. |
| Work hierarchy and execution status | [WorkRecord](../../../../../crates/zap-domain/src/control/records.rs): work_id, parent_id, kind, work_type, state, order, depends_on, acceptance, required_stage, validation_generation, active_job and revision | Hierarchy, dependency, purpose, maturity and execution state are separate fields. A new town/task hierarchy would overlap this structure. |
| Existing work categories | [seams/model.rs](../../../../../crates/zap-domain/src/seams/model.rs): WorkKind = Portfolio, Campaign, Phase, Workstream, Group, Atom, Gate, Horizon; WorkType = Evidence, Decision, Change, Verification, Integration; WorkState = Planned, Ready, Active, Candidate, Accepted, Blocked, Deferred, Dropped, Superseded | Containers, gates, coarse horizons and small actions already have distinct representations. Purpose is independent of hierarchy and current state. |
| Executable task meaning | [TaskContractRecord](../../../../../crates/zap-domain/src/control/records.rs) binds contract_id/work_id/version/digest/active to [TaskContract](../../../../../crates/zap-domain/src/seams/model.rs): goal, read/write subjects, resource IDs, steps, positive/negative cases, checks, acceptance, safe_stop, integration_owner, delivery_route, source_handles and obligation_ids | A task's contract is not just its title or position. Contract resource IDs are distinct from quantified runtime resource claims. |
| Obligation conservation | [ObligationRecord](../../../../../crates/zap-domain/src/control/records.rs): created_for_outcome, current_outcomes, essential, owners, status, disposition, successors and unmet_portion. [ObligationOwner](../../../../../crates/zap-domain/src/seams/model.rs) binds work_id/role; [ObligationTrace](../../../../../crates/zap-domain/src/lowering/model.rs) names implementation, verification, integration and route | Commitments can cross outcome/work groupings. A milestone grouping is not a substitute for their preserved ownership and disposition. |
| Checked lowering and deferred refinement | [LoweringRecord and BoundedHorizon](../../../../../crates/zap-domain/src/lowering/records.rs): strategy/previous lowering, target WorkId, source captures, work bindings, obligation traces, stage debt, deferrals, forks, verification, unresolved horizons and review cause. BoundedHorizon holds subject/question/refinement_trigger | The model already distinguishes coarse unknown work from materialized executable work. A question need not immediately become another executable task. |
| Maturity and postponed duties | [MaturityStage](../../../../../crates/zap-domain/src/seams/model.rs) = Prototype, Functional, Productized; [StageDebt](../../../../../crates/zap-domain/src/lowering/model.rs) binds work/stage to Required, Accepted or Deferred. [DeferralRecord](../../../../../crates/zap-domain/src/control/records.rs) retains scope, reason, current guarantees, responsible party, closure requirement and evidence | Delivery maturity and remaining proof are separate from grouping. Completing a displayed town cannot silently erase these duties. |
| Verification/evidence selection | [VerificationSelection](../../../../../crates/zap-domain/src/lowering/model.rs) names plans, affected/consumer subjects, negative cases, reused evidence and optional full-panel/mutation reasons. [ProofReuseRecord](../../../../../crates/zap-domain/src/knowledge/records.rs) binds review and outcome lineage to evidence and acceptance IDs, captured sources and relevant basis | Existing evidence can be reused with applicability; an aggregate task count alone does not express proof. |
| Sources and facts | [SourceRecord and FactRecord](../../../../../crates/zap-domain/src/knowledge/records.rs): captured source versions/scope/status; fact statement/address, origin, epistemic status, acceptance status, subjects, evidence, sources and source applicability | Useful information already has identity and provenance. Observed, accepted and currently applicable are separate conditions. |
| Fog and information needs | [RegionRecord and KnowledgeClosureRecord](../../../../../crates/zap-domain/src/knowledge/records.rs): bounded question, subjects/work, parent/child regions, state/relevance/evidence; closure boundary/missing endpoints/evidence/basis | Fog has typed epistemic structure. RegionState distinguishes Unexamined, Bounded, Evidenced and Invalidated; RegionRelevance independently distinguishes Relevant, Irrelevant and Unknown. |
| Dependency meanings | [KnowledgeDependencyRecord](../../../../../crates/zap-domain/src/knowledge/records.rs) relates typed endpoints using DependsOn, DerivedFrom, Supports, Verifies, Affects or Consumes | A road cannot indiscriminately mean execution prerequisite, information support and artifact consumption. These relations already differ from WorkRecord.depends_on. |
| Alternatives and routes | [PreparedFork, ForkAlternative and ForkCondition](../../../../../crates/zap-domain/src/lowering/model.rs): premises, three-valued conditions, evidence request, alternatives/value/cost/risks, recommendation, delegated alternatives, rejection conditions, diagnostic action and safe stop | Routes can represent contingent alternatives without treating every unselected branch as ready work. |
| Packet and attempt lineage | [WorkerPacketRecord](../../../../../crates/zap-domain/src/lowering/records.rs): parent/supersedes packet, exact strategy/lowering/work/contract revisions and digests, generation, render basis, obligations, rules, captures, forks and candidate-result template | Any future hero/task view can retain the actual assignment identity rather than copying a title into a new identity. WorkRecord.active_job already links work to a job; live hero rendering is outside this inventory. |
| Available context and omitted context | [PacketFragment, FragmentAvailability and ContextOmission](../../../../../crates/zap-domain/src/lowering/model.rs): artifact/source identity, class, reason, authority-bearing use, required flag, retrieval handle, byte/token estimate and unavailable/unknown/excluded states | An unloaded fragment is not the same as unexplored or invalidated knowledge. Resource-looking icons could otherwise collapse these distinctions. |
| Quantified execution capacity | [ResourceClaim](../../../../../crates/zap-core/src/execution_views/work.rs): resource_id and NonZeroU32 units. [RuntimeClaim and SchedulingCapacity](../../../../../crates/zap-runtime/src/claims.rs): resource/host/integration-owner maps, read/write subject sets, review capacity and occupied_review | These resources constrain concurrent execution. They are not collected facts, research questions or rewards. |
| Nominal team and economic capacity | [ExecutorCapacity, ResourceCapacity and TeamCapacityModel](../../../../../crates/zap-domain/src/economics/model.rs): capabilities, nominal capacities/parallelism, scheduling assumptions, profile digest and evidence | Planning assumptions and their evidence are distinct from current execution occupancy. |
| Cost and information value inputs | [IncrementalCost and CostUnknown](../../../../../crates/zap-domain/src/economics/model/cost.rs) separate elapsed/passive-wait/agent-hours, intervals, categories, material unknowns, resolution actions and evidence. [ChangeAssessmentRecord and CostForecastRecord](../../../../../crates/zap-domain/src/economics/records.rs) retain alternative utility/cost, selected scope, original baseline, cumulative actual and remaining forecast | “Resources that help” already overlap investigable unknowns and evidence-backed decision inputs; they are not automatically fungible quantities. |
| Stops and controlled change | [PauseRecord, StopRuleRecord, ApproachEpochRecord and OwnerChangeDecisionRecord](../../../../../crates/zap-domain/src/owner_control/records.rs) bind campaign/work/subject scope, exact charter/assessment/forecast/policy evidence and approach counters. [ChangeHoldRecord](../../../../../crates/zap-domain/src/economics/records.rs) retains affected/dependent subjects, incomplete boundary, drain jobs, unknown effects and independence basis | A visible route blockage must distinguish Owner stop, economic hold, uncertainty and ordinary capacity wait; changing the projection cannot clear them. |
| Detached questions and opportunities | [DreamBranchRecord](../../../../../crates/zap-domain/src/dreamer/records.rs) retains attachment, intent, grill, assumptions, unknowns, alternatives and estimate. [GrillQuestion, DreamUnknown and DreamProjection](../../../../../crates/zap-domain/src/dreamer/model.rs) retain question kind, recommendation/source input, resolution action, affected state, value/cost/burden and stale/promotable flags | There is already a detached place for useful questions and prospective changes. Grill distinguishes placement, factual discovery and Owner preference. Projection is separate from live application. |

The requested seam types are defined in
[seams/model.rs](../../../../../crates/zap-domain/src/seams/model.rs) and
reexported by [seams/mod.rs](../../../../../crates/zap-domain/src/seams/mod.rs).
There is no `seams.rs` file. Runtime scheduling capacity is defined in
[claims.rs](../../../../../crates/zap-runtime/src/claims.rs), not a
`scheduling.rs` file.

## Naming and conceptual collisions

| Proposed name | Existing overlap | Boundary the design review needs to distinguish |
| --- | --- | --- |
| Milestone / Town | OutcomeRecord; StrategicNode; WorkKind Phase/Group/Gate/Horizon; Work acceptance and StageDebt | A navigation aggregate, an observable intermediate result and a completion/authority object are different choices. No nominal Milestone or Town type occurs in the inspected model/record definitions. |
| Task | WorkRecord; TaskContract/TaskContractRecord; LoweredWorkBinding; WorkerPacketRecord; existing campaign task IDs | A work identity, executable contract, rendered assignment and bookkeeping task are separate. Renaming all of them Task would erase lineage/version distinctions. No plain Task nominal type was found in this bounded surface. |
| Resource | TaskContract.resources; ResourceId-based capacity/claims; SourceRecord/FactRecord/RegionRecord/CostUnknown/GrillQuestion | Execution capacity is quantified and occupied; an information opportunity may be available, uncertain, investigated or invalidated. No generic Resource record in this bounded model unifies those meanings. |
| Route / obstacle | Work.depends_on; knowledge dependency relations; prepared fork conditions; delivery_route; hold/pause/wait | A prerequisite edge, contingent alternative, execution transport and temporary refusal are not one edge/state. A visual route can represent them only with the distinction retained. |
| Capture / completion | WorkState.Accepted; outcome final-gate/promotion duties; evidence applicability; safe-state and economic gates | A group whose displayed members are finished does not by itself describe accepted obligations, remaining deferrals or unresolved effects. The inventory does not introduce a new completion predicate. |
| Fog | RegionState/relevance; fact epistemic/acceptance/applicability status; packet omission state | Unexamined, unknown, invalidated, deliberately irrelevant and unloaded are materially different. Hiding detail is not a knowledge-state change. |

This is a bounded collision inventory, not a global symbol census or a
recommendation to add these names. The later
[final R16 scope review](REVIEW-R16-FINAL-DISPOSITION.md) explicitly separates
implemented backend data from future interactive canvas presentation and
retains known comparison, import, trust, native-capability and measurement
limits. Its earlier final-gate chronology is historical; current campaign.json
records the main campaign as complete and publication as verified.

## Actual 21-task pilot denominator

The captured [campaign.json](campaign.json) is development orchestration,
schema `zap-rust-development-campaign/1`, ID `ZAP-RUST-MVP-2026-09-13`, revision **210**,
state `main_rust_campaign_complete_followup_analysis`. Its publication state is
`published_verified`; all 21 task rows say
`accepted_for_rust_mvp`.

This is not a canonical ZAP WorkRecord store or the 425-node NEXT import.
The following partition is a retrospective projection candidate. It creates
no WorkId, MilestoneId, record, dependency, approval or migration.

The captured plan has **21 unique IDs**, **49 `depends_on` start/planning
edges**, and **9 separate `acceptance_depends_on` edges across six tasks**.
The acceptance edges have no overlap with the start/planning edges, giving
58 unique endpoint pairs if edge kind is temporarily ignored for counting.
That union is not a replacement scheduling graph. No dependency ID is unknown;
the start graph and the union relation are acyclic. `R01` and
`R01-FOUNDATION` remain separate IDs, both depending on R00, with no stored
start or acceptance edge between them.

| Candidate group | Observable result under discussion | Exact task members |
| --- | --- | --- |
| G1 — Typed foundation | Declared requirements, API ownership, typed workspace and transactional substrate provide a usable foundation. | R00, R01-FOUNDATION, R01, R02, R03, R04 |
| G2 — Governed campaign | Campaign authority, obligations, knowledge/adaptation and economics have a governed semantic model. | R05, R06, R07 |
| G3 — Durable execution | Checked lowering can produce bounded worker packets and offline/native protocol execution with recovery and truthful profiles. | R08, R09, R11, R12 |
| G4 — Safe adaptation and migration | Detached exploration/adoption and inactive legacy migration preserve commitments and history. | R10, R14 |
| G5 — Operable service | Clients can inspect and operate the packaged service through its declared CLI/backend interfaces. | R13, R15 |
| G6 — Verified release | Integrated behavior, declared scale, production gates and installed publication have evidence. | R16, R17, R18, R19 |

The group size is specific to this pilot, not a universal limit on towns,
children, tasks or context. G4 deliberately groups two independent capabilities
by preservation of meaning/history; its R10 and R14 members have no direct
dependency on each other.

| Task | Stored outcome label | Stored `depends_on` prerequisites | Candidate group |
| --- | --- | --- | --- |
| R00 | Approved plan and recovery checkpoints | none | G1 |
| R01 | Rust integration API and storage ADR | R00 | G1 |
| R02 | Normative requirements and complete denominator | R00 | G1 |
| R03 | Portable Cargo workspace and typed core | R01-FOUNDATION | G1 |
| R04 | Transactional store and indexed queries | R03 | G1 |
| R05 | Domain authority obligations and completion | R01-FOUNDATION, R02 | G2 |
| R06 | Knowledge evidence and adaptive changes | R01, R05 | G2 |
| R07 | Economics forecasts holds and closure | R04, R05 | G2 |
| R08 | Lowering roles packets and verification selection | R05, R06 | G3 |
| R09 | Weak execution roundtrip and encounters | R04, R08 | G3 |
| R10 | Dreamer overlays grill and live adoption | R06, R07, R08 | G4 |
| R11 | Concurrent native host runtime and recovery | R01, R02 | G3 |
| R12 | Capabilities role profiles resume and goals | R01, R02 | G3 |
| R13 | CLI backend observability and typed queries | R04, R05, R11 | G5 |
| R14 | Lossless legacy and actual NEXT migration | R04, R05 | G4 |
| R15 | Installed package skills and production documentation | R02, R03, R08, R11, R12, R13 | G5 |
| R16 | Integrated behavior acceptance | R06, R07, R08, R09, R10, R11, R12, R13, R14, R15 | G6 |
| R17 | Measured graph and history scalability | R04, R06, R13 | G6 |
| R18 | Production discipline and final package gate | R16, R17 | G6 |
| R19 | Publication installed proof and completion | R18 | G6 |
| R01-FOUNDATION | Corrected foundation API sections1-7 and ownership accepted | R00 | G1 |

R14's stored “actual NEXT migration” label refers to the bounded inactive
import evidence. This analysis does not authorize NEXT activation or execution.

## Start/planning dependencies

The 49 `depends_on` edges comprise **12 internal task edges** and
**37 cross-group task edges**, projecting to **14 distinct group pairs**.
This table counts start/planning prerequisites only. An arrow means at least
one listed member prerequisite; it never means that the entire source group
must finish before any destination-group task can begin.

| Group pair | Exact existing task edges represented |
| --- | --- |
| G1→G2 | R01-FOUNDATION→R05; R02→R05; R01→R06; R04→R07 |
| G1→G3 | R04→R09; R01→R11; R02→R11; R01→R12; R02→R12 |
| G1→G4 | R04→R14 |
| G1→G5 | R04→R13; R02→R15; R03→R15 |
| G1→G6 | R04→R17 |
| G2→G3 | R05→R08; R06→R08 |
| G2→G4 | R06→R10; R07→R10; R05→R14 |
| G2→G5 | R05→R13 |
| G2→G6 | R06→R16; R07→R16; R06→R17 |
| G3→G4 | R08→R10 |
| G3→G5 | R11→R13; R08→R15; R11→R15; R12→R15 |
| G3→G6 | R08→R16; R09→R16; R11→R16; R12→R16 |
| G4→G6 | R10→R16; R14→R16 |
| G5→G6 | R13→R16; R15→R16; R13→R17 |

Internal `depends_on` edges remain:
R00→R01; R00→R02; R01-FOUNDATION→R03; R03→R04; R05→R06; R05→R07; R08→R09; R13→R15; R16→R18; R17→R18; R18→R19; R00→R01-FOUNDATION.

## Acceptance prerequisites kept separate

Six task rows have nonempty `acceptance_depends_on` arrays. The other 15 rows
have no additional acceptance prerequisites recorded in that field.

| Task | Additional acceptance prerequisites | Exact acceptance edges and group placement |
| --- | --- | --- |
| R05 | R03 | R03→R05, G1→G2 |
| R06 | R04 | R04→R06, G1→G2 |
| R08 | R07 | R07→R08, G2→G3 |
| R11 | R03, R04, R05 | R03→R11 and R04→R11, G1→G3; R05→R11, G2→G3 |
| R12 | R08, R11 | R08→R12 and R11→R12, both internal to G3 |
| R14 | R06 | R06→R14, G2→G4 |

All **9 acceptance edges are unique** and all **9 are additional endpoint
pairs** relative to the 49 start/planning edges: **0 overlap**. They comprise
**2 internal edges** and **7 cross-group edges**. Their four cross-group pairs
are G1→G2, G1→G3, G2→G3 and G2→G4, already present in the start projection.
Thus the combined coarse projection still has 14 group pairs; **0 new coarse
pairs** does not mean **0 additional constraints**. The count of 58 endpoint
pairs is only an inventory union and does not flatten their different roles.

## Early start does not waive acceptance

Preserve these concrete early-entry distinctions:

- R05 can start from R01-FOUNDATION and R02 without waiting for all G1 members;
  its acceptance additionally depends on R03.
- R11 and R12 can start from R01 and R02 before all governed semantics in G2
  finish. R11 acceptance additionally depends on R03/R04/R05; R12 acceptance
  additionally depends on R08/R11.
- R14 starts from R04/R05 independently of R10; its acceptance additionally
  depends on R06.
- R13 starts from R04/R05/R11 and has no additional recorded acceptance array;
  this does not remove its declared prerequisites or their own proof duties.
- R17 starts from R04/R06/R13 and can begin before R16/R15; it has no additional
  recorded acceptance array. Starting it is not evidence that its scale work
  has passed.

A whole-town start barrier would add constraints absent from the original
`depends_on` graph, while treating early start as early acceptance would drop
recorded proof dependencies. This report preserves both kinds and proposes
neither transformation.

The six-group table and Mermaid arrows in the root's
[design review](../../../research/zap/ZAP-STRATEGIC-MAP-FOLLOWUP-REVIEW-2026-09-14.md)
were previously compared with the start/planning partition: all 21 task
memberships and all 14 coarse pairs agreed. The separate acceptance inventory
adds no new coarse pair, but the coarse arrows alone do not display its nine
additional task-level proof dependencies. The typed tables above retain them.

## Evidence and inspection limits

- Owner input SHA-256:
  `51284C81105C22C28689E9FABD4CB404C51D5FA99CD703A12C2C31445F5A3CE9`.
- Follow-up brief SHA-256:
  `0C426CB427B9AEA5B425DB24C74988EC3ACDAAE17176E9D03BC7CEA130688A10`.
- Captured campaign.json SHA-256:
  `33B822DB7320BD5F5F4E79C0924A60D33DF4617EA8EBCECC5F29ACD9D31FF5B0`.
- This dependency-kind correction supersedes the initial revision-209 count
  when it was described as all constraints. The 49-edge start/planning count
  remains correct; the previously omitted nine acceptance edges are now explicit.

Only the named campaign/research inputs, final R16 scope review and relevant
model/record/seam/resource definitions were inspected. This inventory did not
reverify all reducer algorithms, run tests, measure planning effectiveness,
open a runtime store or browse external research. It supplies source and DAG
facts for root's design judgment. Only this report was written; the campaign,
product and published artifact remain unchanged.
