# ZAP-B domain implementation report

## Delivered boundary

The adaptive domain reducer is implemented as 21 strict versioned events. It
adds first-class intent and outcome revisions, obligations and typed ownership,
work overlays and successors, recursive lowering, task-contract versions,
evidence applicability/adjudication, achieved stages, deferrals, adaptive
reviews, central and integration acceptance, fact-promotion metadata and
truthful campaign closure.

`DOMAIN_HANDLERS`, `DOMAIN_EVENT_SCHEMAS`, `DOMAIN_DATA_KINDS`,
`DOMAIN_ACTION_KINDS`, `DOMAIN_CAPABILITIES`, `domain_state`,
`domain_frontier` and `intent_fingerprint` are the stable sibling seams. All
implementation modules are below 500 lines. Core, control and knowledge files
were not edited.

Pure reducers perform no source read, dispatch, filesystem change, model call,
network call or clock read. Reviews record a complete job reconciliation plan
and explicitly mark effects as not performed by the reducer. Fact promotion
records the permanent target identity, exact content hash and effects-adapter
receipt; the separate adapter owns the write.

## Capability-to-test matrix

| Requested domain capability | Focused evidence |
| --- | --- |
| Lazy compatibility with the imported graph; sourced legacy acceptance; no retroactive receipts | `test_legacy_projection_is_lazy_lossless_and_sourced` |
| Machine-readable exact event surface and fail-closed service routing | `test_machine_schemas_cover_exact_handler_registry`; real `DomainControlIntegration` construction |
| Owner-bound intent and outcome revisions | `test_proposals_are_data_only_but_adoption_requires_policy`; `test_intent_revision_requires_atomic_adaptive_outcome_revision`; real B/C service tests |
| Coordinator cannot change intent/values under unchanged charter; exact owner amendment can | negative and positive halves of `test_intent_revision_requires_atomic_adaptive_outcome_revision`; `DomainControlIntegration.test_owner_charter_amendment_enables_exact_new_intent_binding` |
| First-class obligations, essential protection, explicit complete disposition, successor links and unmet portion | `test_outcome_revision_requires_complete_authorized_dispositions`; `test_adaptive_pivot_is_atomic_and_cannot_lose_obligations` |
| Typed ownership transfer and dependency meaning | `test_explicit_ownership_transfer_and_drop_controls_dependency_readiness`; `test_imported_dropped_prerequisite_does_not_unlock_consumer` |
| Recursive lowering, exact current obligation coverage, leaf contracts and inherited dependencies | `test_lowering_requires_current_coverage_and_leaf_contracts` lowers two levels and exercises two refusals |
| Task-contract versions, rename, valid structural transition and separate dispatch transition | `test_contract_rename_and_transitions_use_exact_action_classes` |
| Producer PASS and declared maturity cannot become proof | `test_producer_pass_and_declared_maturity_cannot_accept_work` |
| Real current source applicability, exact work subject, durable artifact and complete closure gate evidence | `test_applicable_evidence_stage_and_central_acceptance`; `test_adjudication_rejects_wrong_subject_and_missing_artifact`; real `DomainSourcesIntegration` |
| Achieved stage and current central work acceptance | `test_applicable_evidence_stage_and_central_acceptance` |
| Integration acceptance distinguishes imported assertions | `test_integration_acceptance_marks_legacy_inputs_as_assertions` |
| Formal deferral create/transfer/close and authorized inapplicability | `test_deferral_covers_transfers_and_closes_with_applicable_evidence`; `test_deferral_can_become_inapplicable_only_after_authorized_obligation_disposition` |
| Review captures, actual fog expansion/reopening, knowledge/value/feasibility alternatives, keep route, atomic pivot and complete job reconciliation without effects | `test_adaptive_pivot_is_atomic_and_cannot_lose_obligations`; `test_keep_route_review_can_reconcile_jobs_without_performing_effects`; real `DomainSourcesIntegration` fabricated/drifted-region refusals and selected-region priority change |
| Fact promotion metadata with separate adapter receipt | `test_fact_promotion_records_external_adapter_metadata_only` |
| Original and revised complete success | `test_original_and_revised_success_require_current_complete_acceptance` |
| Partial and unreachable closure with explicit unmet portions; false success refusal | `test_partial_closure_is_truthful_and_success_cannot_hide_unmet_work` |
| Agent proposal, self-adoption refusal, credentialed adoption and sticky-pause refusal through real control service | `DomainControlIntegration.test_agent_proposals_controlled_adoption_and_sticky_pause` |

## Validation

Command run from `vibevm/vibespecs/skills/zap-state/scripts`:

```text
python -B -m unittest test_domain.py test_domain_control.py test_domain_sources.py
```

Result: `Ran 23 tests in 1.188s` — `OK`.

Legacy compatibility check:

```text
python -B -m unittest test_zap_state.py
```

Result: `Ran 18 tests in 0.942s` — `OK`.

The B/D integration exposed and resolved one contract detail: the frozen
`current_applicability` result uses `status` and `incomplete_closure`; it has no
redundant `applicable` boolean. The real path now passes from captured source,
through applicability and closure assessments, into domain evidence
adjudication.

The owner-control review exposed and resolved an authority defect before
handoff: an `adaptive.apply` grant alone can no longer change owner intent.
Domain intent adoption requires the exact charter intent fingerprint plus the
active charter ID, revision and hash; a later intent requires a changed owner
charter amendment. Ordinary outcome pivots retaining intent remain available
inside the delegated adaptation envelope.
