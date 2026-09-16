# R06 independent review: knowledge, basis, proof, and adaptive application

Status: changes requested. No build or test command was run for this review.

## Findings

1. **[P1] Unrelated evidence can establish closure, source applicability, and
   fact truth.** `knowledge/cells/graph.rs:92`, `:134`, and `:258` all delegate
   to `proof_refs_are_current` (`:299`), which checks only accepted/current/pass.
   It never checks that `EvidenceApplicability`, source captures, outcome,
   obligation, work, or basis covers the closure subject, source, or fact being
   adjudicated; the closure and applicability cells also store the payload's
   basis without a `PayloadBasisScope` recomputation. An accepted proof for
   unrelated work can therefore mark another source applicable, another
   closure complete, and another fact observed and
   accepted. Promotion at `acceptance/cells.rs:452-466` inherits that invalid
   fact/evidence association. Require subject-specific proof coverage and the
   exact current assessment basis at each adjudication boundary.

2. **[P1] A source recapture does not invalidate evidence that directly names
   that source unless a second graph edge happens to exist.**
   `knowledge/cells/source.rs:219` invalidates evidence only when the calculated
   `KnowledgeEndpoint::Evidence` is reachable through registered dependency
   edges. `EvidenceAdjudicationRecord.source_captures` is already a known direct
   dependency but is not used. The passing test at
   `tests/knowledge_contracts.rs:34` manually supplies Source -> Fact -> Evidence
   edges, masking the ordinary record-only case. Derive direct evidence edges or
   invalidate from exact captures, and cover the registered recapture cell.

3. **[P1] Data-proposal routes can erase fog without adjudication.**
   `knowledge/cells/region.rs:22-62` registers state and relevance mutation as
   `DataProposal`, yet `region_transitions.rs:18-33` permits transition to
   `Evidenced` from any different state when evidence IDs are merely nonempty;
   IDs are neither resolved nor checked for applicability. Relevance can also
   be set to `Irrelevant` with no authority or evidence check. Because basis
   logic trusts these fields, an untrusted proposal can remove an unknown from
   admission inputs. Store proposed versus applied region assessments, or gate
   truth-changing transitions through current scoped evidence and authority.

4. **[P1] Recording a dependency invalidates closure in the wrong direction.**
   At `knowledge/cells/graph.rs:60`, a new prerequisite -> dependent edge calls
   `upstream_endpoints` from the prerequisite. This marks the prerequisite and
   its ancestors unknown, while leaving the newly affected dependent and its
   downstream consumers complete. Traverse from the new dependent through
   outgoing dependent edges. Add a case where previously complete B becomes
   incomplete after adding A -> B.

5. **[P1] Affected-job reconciliation is neither dependency-complete nor
   action-safe.** `knowledge/cells/adaptive_apply.rs:154-220` collects directly
   changed work, owners, and deferrals but not work that depends on changed or
   dropped work. `knowledge/review_transitions.rs:240-283` then compares only
   job/attempt/work/generation tuples; it accepts `Continue` for a job whose work
   is dropped, superseded, or invalidated. Derive dependent closure and validate
   each planned action and safe boundary against the admitted transition.

6. **[P1] Revalidation still does not derive authorization from the review.**
   `knowledge/cells/revalidation.rs:60-66` checks a trusted release and only that
   the named review is applied, then calls `ready_revalidation(..., true, ...)`.
   It never proves that the review selected `Revalidate` for this work/job. Any
   applied review plus a matching release can advance an unrelated blocked
   work generation. Match the exact review work change and reconciliation row.

7. **[P1] Non-pivot ownership changes are silently accepted and discarded.**
   Review validation permits `ownership_changes` for any decision, but
   `adaptive_apply.rs:101-111` calls `apply_ownership` only through the pivot
   path (`:270`). A keep-route/reorder/replace-method review can become Applied
   while its declared ownership transition never occurs. Apply ownership to the
   computed post-state for every allowed decision, or reject it outside pivot.

8. **[P1] Promotion does not yet prove the exact repository effect.**
   `acceptance/cells.rs:432-466` checks that the admitted observation reference
   equals the payload receipt, but it loads no durable effect record binding
   that receipt to the destination, target, and content digest. A trusted
   observation from another effect can therefore satisfy the visible cell
   predicate. Until R15 supplies and the cell verifies that exact binding, the
   registered promotion route must remain unavailable rather than count as a
   completed project-fact write.

9. **[P2] Scoped unknown reporting is internally inconsistent.**
   `knowledge/basis_helpers.rs:455-480` adds every relevant unresolved region
   and every unknown/invalidated fact in the store, even when unrelated to the
   selected closure. It also marks selected `Intent`, `Contract`, and `Review`
   subjects unknown although `endpoint_subject` (`:486`) cannot represent them,
   making `AssessedComplete` unattainable for such scopes. Conversely,
   `knowledge/queries.rs:107-116` reports only existing incomplete closure rows,
   so a selected subject with no assessment appears to have no incompleteness.
   Use one selected-endpoint closure model and make absent assessment explicit.

The reported 17 checks mostly exercise pure helpers. They do not exercise
`DomainBasisProvider` or the registered closure/applicability, region,
recapture, adaptive-apply, promotion, and revalidation paths above. The honest
R13/R15 adapter limitations in `REPORT-R06.md` remain valid downstream work,
but they do not account for these in-slice transition defects.
