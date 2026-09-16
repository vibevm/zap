# Root and independent review of the in-progress projection

Status: concrete repair queue, not acceptance. Observed source was still being
implemented around14:22–14:31UTC on2026-09-14; later revisions may resolve these
items. No extra implementation scope or repeated host-wide panel is implied.

| Boundary | Observed issue | Required disposition |
| --- | --- | --- |
| Route lower bound | Lexical WorkId iteration loses predecessor cost when dependency order differs from lexical order. | Topological dependency computation; reverse-lexical chain and diamond fixtures. Missing estimates retain explicit partial status. |
| Read accounting | 1 + examined memberships + index rows is not an exact record-read count. | Actual counters, or remove the misleading exact label and describe independently bounded components. Do not reset hidden large per-card budgets. |
| Landmark filter | Unknown/unmaterialized nodes were included as landmarks; Phase/Workstream and larger containers were omitted. | Explicit known container facets; unknown kind diagnostics separate from matching landmarks, with truthful continuation. |
| Blockers | Empty blocker lists or raw dependencies could be interpreted as proved readiness/blockers. | Explicit evaluation/coverage state; preserve source status without claiming runtime admissibility. |
| Optional assessment | Missing descriptive metadata was classified as a relationship gap. | Assessment availability and relationship coverage remain independent. |
| Work reverse joins | Child/dependent indexes were trusted without checking parent_id/depends_on in the fetched record. | Validate the exact current partition predicate and refuse mismatch. |
| Knowledge edge integrity | Incoming/outgoing copies with equal edge IDs were deduplicated despite conflicting values; direction was not checked. | Verify incident endpoint/direction and equality before deduplication; conflicting values refuse. |
| Other reverse joins | Source/Fact/Region/Evidence existence was checked but requested Work membership was not. | Check current source scope, fact subjects, region subject/work union and evidence applies-to Work before emitting a link. |
| Complete card claims | Fact/Region subject and evidence links, project/unassessed scopes and unsupported subject conversions were omitted while completeness was true. | Emit supported direct relations and explicit gaps for unrepresented meanings. A partial projection cannot claim complete relationships. |
| Contract access meaning | Read and write subjects were both labeled Consumes. | Preserve distinct read/write semantics; knowledge Consumes remains its own relation. |

Root also reviewed the first assessment model. Its repair queue requires
nonblank supplied human text, optional rationale/context retained for
Unassessed values, definite passive-wait/elapsed contradiction rejection,
bounded existing evidence references, and the shared complete last-active
contract selection policy. These affect only the newly introduced descriptive
surface; current Work and admission semantics remain unchanged.

The independent read-only reviewer confirmed the five relationship findings
on indexes.rs62FF480992E5…, cards.rsF8483F7761A2… and extracted
relationships.rs79F0A98D385B…. Worker reports are evidence. Root will review the
final diff and focused test receipts before accepting this queue as resolved.
