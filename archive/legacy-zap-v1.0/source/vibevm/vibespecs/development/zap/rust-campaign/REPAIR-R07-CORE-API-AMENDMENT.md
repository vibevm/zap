# R07 core API amendments

Accepted by the root coordinator under the Owner's implementation commission
on 2026-09-13 while implementing
`REPAIR-R07-CORE-API.md`.

## Typed impact request reaches admission needs

`ActionAdmissionRequest` carries the service-derived registered
`ActionImpactRequest` as `impact_request`. Core constructs it only from the
typed cell adapter and requires its digest to equal
`ActionImpactView.request_digest` before calling `needs`, `admit`, or `apply`.
Live execution and replay derive it from the same decoded payload. This is
non-authorizing typed scope evidence and is never accepted from caller JSON.

This field lets the admission provider request current affected closure and
independence evidence for exempt progress/proof actions as well as semantic
actions.

## Per-effect approval identity and current suffix identity

`EffectItemDigest` is the domain-separated type of
`EffectPreflightView.stable_digest`. `ChangeEffect.preflight_digest` and
`OwnerChangeDecisionRecord.effect_preflight_digests` use this per-effect type.
The existing ordered `effect_fingerprints: Vec<PayloadDigest>` remains a
separate business-envelope check.

Assessment preflight remains one sequential full simulation per alternative.
Each effect receives its corresponding stable item digest. At execution, core
simulates the exact remaining suffix from the persisted committed prefix. The
first suffix item's `EffectItemDigest` must equal the approved effect identity.
`EffectBundlePreflightView.digest: EffectPreflightDigest` identifies that
current suffix and may be recorded as transaction evidence; it is not compared
with the original full-alternative bundle digest. `EffectMutationDigest`
continues to compare simulation and actual product mutations only within the
current transaction.
