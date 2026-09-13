# R04 source review: official mutation admission

Status: the three findings below are repaired in the current service candidate.
Root reviewed the new identity/basis/action-request gate order and reused the
three real-service/redb test receipts. This accepts these repairs, not the
remaining R04 artifact/index/reconciliation/production boundary. No additional
root product test panel was run.

1. CommitService::execute verifies the opened store against its configured
   identity but does not explicitly bind command-header store/base/campaign/
   protocol to that identity before mutation. A foreign header must refuse;
   typed field shape or a grant matching a caller-supplied campaign is not that
   check. Keep idempotency scoped to the actual store and canonical command.
2. Relevant-basis validation is conditional on the caller choosing Exact.
   Registered payload/route requirements must determine whether a basis is
   mandatory. A required effect cannot choose NotApplicable and skip it.
3. ActionAdmissionProvider receives only action and header. It lacks exact
   command/payload identity and actual payload-derived scope, so it cannot
   validate a full economics envelope or semantic before-action condition.
   Supply an immutable validated action request with the relevant frame/digests/
   scope; command identity alone does not prove the approved effect matches.

Use the real service/store path to distinguish foreign identity, basis opt-out,
and same-header/different-payload authorization from valid commands. Coordinate
the shared typed admission port with its consumers. These are required gates,
not a reason to run the full host test panel.

## Open: trusted grant issuance must remain scoped

The new public service_permit(operation) and trusted_observation_grant(harness,
operation, observation) helpers expose issuance using known identifiers alone.
A service reference must not manufacture arbitrary internal or observed-state
authority. Require an opaque trusted-bootstrap issuer handle or authenticated
scoped principal outside data/request schemas, bind the grant/permit to the
service/store/base/campaign/controller epoch and exact allowed operation/event/
effect, and verify those bindings during admission. ServiceInternal must not
accept a permit merely because it contains an operation ID.

TrustedHostBinding should identify the configured PrincipalId explicitly;
HarnessId is a platform identity and may be shared by multiple drivers. R04 and
R11 coordinate the lawful issuer seam and real-service negative cases.

## Follow-up: scoped handles, durable artifacts and concurrent audit

Opaque bootstrap handles now replace the identifier-only public factories.
Root's remaining source review asks for actual current controller-epoch
fencing, rather than a positive-epoch check alone, and distinct configured
principals sharing a harness without an unintended harness-wide uniqueness
restriction. Service rotation may fence handles only if that lifecycle is
explicit and enforced.

Artifact publication must atomically refuse replacement: checking exists and
then calling rename can overwrite a concurrent destination on Unix. Persist
the directory publication on platforms requiring it before a database commit
can reference the blob. The published witness must remain guarded through the
actual referencing commit; a standalone blob test is not that integration.

Full audit now has a real reducer replay into a fresh sibling. Its initial
hash audit and subsequent tail pages currently open separate snapshots. A
concurrent writer must produce a structured stale/retry outcome or be covered
by one pinned read, rather than being diagnosed as corrupt history.

## Final admission follow-up: exact retry still requires route entitlement

The reviewed execute path returns an existing exact receipt before checking
the principal against the registered route. Exact canonical command knowledge
does not authorize a worker/data principal to retrieve an Owner or trusted
route's result. Perform static authentication and exact route entitlement for
both new commands and retries. Reapply current business/effect gates only to
new effects; an authorized exact retry after a pause must still return the
original committed receipt. Extend the existing service scenario with the
same-frame wrong-principal negative case.
