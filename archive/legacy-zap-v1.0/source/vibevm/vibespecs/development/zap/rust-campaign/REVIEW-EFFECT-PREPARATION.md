# Effect preparation and stable execution: root integration findings

Status: concrete source-review findings assigned to R07/R08; public preparation
composition also belongs to R13. These are completion requirements, not new
Owner approvals or permission grants.

1. `core/preflight.rs::simulate_effects` was observed validating only the after
   basis. Each effect must validate its registered scope and compute/check the
   actual before basis against the current branch overlay before simulation.
   Copying the caller's declared before digest into context is not validation.
2. Sequential overlay order is not automatically equality of neighboring local
   basis digests. Different effect kinds may have different BasisPurpose and
   root sets (review then lowering, for example). Preserve actual per-effect
   before/after verification; do not weaken it to satisfy an invalid global
   chain assumption. R07 must state the exact relationship in its amendment.
3. R08's lowering kernel was observed requiring the proposed lowering record's
   revision to equal the command's next GLOBAL revision. New graph/contract
   stamps need the same audit. Assessment/adjudication and unrelated events can
   advance that revision before the approved effect executes. The real semantic
   lowering service test must expose this. Use stable local business versions
   or an equally explicit verified solution; global event revision remains
   metadata. Never silently rewrite an approved canonical payload or obtain a
   replacement approval merely to hide global churn.
4. The current public source search found no read-only effect-preparation API;
   preflight is internal. R13 must expose a typed non-authorizing preparation
   path so a caller/IDE can obtain derived scope and before/after/item digests
   from the actual registered kernels, without guessing a future hash or
   constructing a private overlay in ad hoc code. Preparation cannot commit,
   consume approval, launch effects or grant authority. Later submission still
   validates current state and the exact approved request normally.

Required evidence is a real multi-kind sequence including review and lowering,
intervening assessment/decision and unrelated events, changed-business-state
refusal, and public preparation/submit composition. Exempt first lowering and a
fixture-only counter product remain useful bounded tests but do not close this
integration requirement.
