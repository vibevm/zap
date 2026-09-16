# Accepted R07 amendment: effect approval and bundle preflight identities

Root architecture decision, 2026-09-13. This narrow amendment supersedes any
REPAIR-R07-CORE-API.md instruction equating an individual approved effect with
the digest of a full alternative or its remaining suffix. R07 owns implementation.

The previous wording was inconsistent: a bundle digest includes its committed
prefix and ordered remaining effects, so executing the next effect necessarily
changes that bundle digest. Approval must not be silently recomputed or made
impossible by this expected progression.

1. Assessment still simulates one complete sequential bundle for each
   alternative. Each alternative starts from the same base; effects within it
   use the accepted overlay. Do not simulate each effect independently from
   the original base.
2. Add the typed `EffectItemDigest` to zap-wire. Use it for
   `EffectPreflightView.stable_digest`, retaining the accepted exact field
   hash: effect ID/index, event kind/payload/product event, predecessors,
   derived subjects/work/artifacts, reducer epoch and relevant before/after
   bases. Exclude observed global revision and concrete mutation digest.
3. `ChangeEffect.preflight_digest` becomes `Option<EffectItemDigest>` and
   `OwnerDecisionRecord.effect_preflight_digests` becomes the exact ordered
   `Vec<EffectItemDigest>`. Preserve the separate existing
   `effect_fingerprints: Vec<PayloadDigest>` business fingerprint contract.
   Assessment/alternative identity, complete ordered list and selected decision
   bind the approval; an item digest alone grants no authority.
4. `EffectBundlePreflightView.digest` remains `EffectPreflightDigest`, with
   the accepted prefix/request/before-after/ordered-item hash. This is the
   current simulation envelope's identity, not an individual effect approval.
5. Execution reconstructs the exact remaining suffix of the selected stored
   alternative and the exact persisted applied prefix. Core simulates that
   suffix sequentially from current state. Admission compares its first
   effect's `EffectItemDigest` with the next approved effect, along with the
   selected assessment/alternative, order/index, predecessors and applied
   prefix. It never compares a suffix bundle digest to the original full
   bundle digest. Any relevant business drift refuses rather than refreshing
   approval silently.
6. Admission records the actual current suffix `EffectPreflightDigest` as
   transaction evidence and separately binds the approved `EffectItemDigest`.
   Concrete `EffectMutationDigest` comparison remains within this transaction.
   The after-basis must match the selected effect's derived after-basis.
   Applying the first effect advances the exact persisted prefix once; retry
   does not advance it again. Frozen schema-1 types/bytes remain unchanged.

Required focused evidence: at least two dependent effects in one approved
alternative; first commit; unrelated observation/revision churn; remaining
suffix simulation; second commit; exact retry and cold audit. Show that the
suffix bundle digest differs as expected while approved item identity still
matches. A changed relevant business state, wrong prefix/order/item or an
independently simulated second effect must refuse. No new approval is needed
solely because an earlier approved effect was successfully applied.
