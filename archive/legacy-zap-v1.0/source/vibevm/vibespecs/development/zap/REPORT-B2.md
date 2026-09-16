# ZAP-B2 selective reuse and sparse review report

## Delivered boundary

Outcome pivots now preserve only explicitly selected proof whose material
meaning remains current. Evidence, achieved stages, work acceptance and parent
integration keep their original outcome/event records. Applying the review
adds versioned reuse witnesses for the new outcome. Readiness, dependency
evaluation, central acceptance and campaign closure resolve those witnesses
and recheck source hashes and applicability, evidence identity, obligations,
ownership, task contract, stage and integration graph.

The reducer refuses carryover when required guarantees change, source content
or applicability changes, a selected work contract or subject changes,
revalidation is requested, a covered obligation is disposed, selected work
gains an uncovered obligation, proof ownership changes, or an acceptance or
integration omits part of its proof/dependency graph. Historical records remain
readable after refusal and after pivots with no selected reuse.

`build_sparse_review_transition(state, request)` and
`SPARSE_REVIEW_TRANSITION_SCHEMA` provide the model/IDE expansion seam. The
helper reads, but does not exercise, the active charter delegation. It expands
omitted active obligations to sorted retained dispositions and returns the
exact full nested transition for `domain.review-proposed`. The journal receives
only that expanded transition. `DOMAIN_OPERATIONS` publishes the operation as
`domain.materialize-review-transition`.

`current_acceptance_coverage(state)` is the runtime/viewer query seam. It
returns only work, obligations and integrations whose central acceptance is
valid for the active outcome after following and revalidating any reuse
witnesses.

## Capability-to-test matrix

| B2 property | Focused evidence |
| --- | --- |
| Deterministic sparse expansion with a large denominator | `DomainTests.test_sparse_builder_expands_large_denominator_deterministically` creates 1,292 obligations and verifies full sorted retention from a small request |
| Explicit selective transfer without rewriting evidence/stage/acceptance history | `DomainTests.test_pivot_selectively_reuses_proof_without_rewriting_history` |
| Current readiness and revised closure can use a valid witness | same test exercises `work_is_accepted`, `current_acceptance_coverage` and `domain.campaign-closed` |
| Old outcome proof is not silently current | `DomainTests.test_pivot_without_explicit_reuse_does_not_make_old_acceptance_current` |
| Guarantee, new-obligation and disposed-obligation changes refuse reuse | `DomainTests.test_sparse_reuse_refuses_changed_guarantee_new_obligation_and_disposition` |
| Changed contract/revalidation and stale applicability refuse reuse | `DomainTests.test_sparse_reuse_refuses_changed_contract_and_stale_source` |
| Real B/C/D two-branch preservation and parent integration accounting | `DomainReuseIntegration.test_local_pivot_reuses_stable_branch_and_invalidates_parent_integration` preserves Y, invalidates changed X, and keeps parent R unaccepted |
| Changed source after review capture cannot produce a witness | `DomainReuseIntegration.test_source_change_after_review_capture_refuses_reuse` uses real knowledge recapture and control admission |
| A stale source can still be truthfully adjudicated inapplicable | `DomainSourcesIntegration.test_inapplicable_adjudication_retains_a_stale_source_identity` |
| Complementary scoped checks cover stage/work without overstating each proof | `DomainTests.test_scoped_checks_collectively_cover_stage_and_work_obligations` includes missing-proof refusals |
| Complementary task proofs cover the campaign final gate as a union | `DomainReuseIntegration.test_complementary_final_gate_proofs_cover_the_full_closure_union` includes a missing-task refusal |
| Strict schemas and public operation registry | `DomainTests.test_machine_schemas_cover_exact_handler_registry` |

## Validation

Focused command from `vibevm/vibespecs/skills/zap-state/scripts`:

```text
python -B -m unittest test_domain.py test_domain_control.py test_domain_sources.py test_domain_reuse_integration.py
```

Result: `Ran 33 tests in 37.202s` — `OK`.

Legacy compatibility command:

```text
python -B -m unittest test_zap_state.py
```

Result: `Ran 18 tests in 0.932s` — `OK`.

The read-only migrated campaign exercise loaded an immutable store into memory,
derived the domain projection, and added only test-local intent/outcome/policy
metadata. With `$ZAP_MIGRATION_STORE` pointing to the migrated store, the
exercise used `load_store(Path(os.environ['ZAP_MIGRATION_STORE']),
CORE_HANDLERS)`, `domain_state`, and `build_sparse_review_transition` under a
test policy view. Result:

```text
nodes=425 tasks=212 mandates=64 obligations=1292
materialized=1292 retained=1292 sorted=True
```

No store, source plan, runtime, runner or transport operation was mutated or
invoked.
