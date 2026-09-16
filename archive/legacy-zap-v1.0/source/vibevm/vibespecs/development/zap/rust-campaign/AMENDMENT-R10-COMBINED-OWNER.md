# Accepted R10 amendment: one combined charter and economics Owner response

Root architecture decision, 2026-09-14.

An outside-current-charter Dream scope change uses one dedicated
`CombinedCharterChangeDecision` Owner control route. Charter-only and
economics-only credentials do not imply it. The combined cell atomically:

1. validates the saved Dream requirement against the exact original charter;
2. permits only the union of its named actions and mutable obligations, with no
   other charter field change;
3. executes the existing pure `amend_charter` transition;
4. executes the same factored economics Owner-decision kernel as the ordinary
   `ChangeDecisionRecorded` cell; and
5. stores one immutable combined authorization binding original/replacement
   charter, Dream revision/projection, assessment revision/digest/comparison
   basis, selected alternative, decision and ordered effect fingerprints/items.

The combined cell refuses under an active campaign Owner/stop-rule pause. It
does not grant a generic batch, use `plan.lower`, consume a product admission,
or let the old charter authorize the permission being added.

The later `DreamApplied` command remains an ordinary privileged semantic
product under the existing economics admission, prefix, pause and held-job
gates. It requires the exact active replacement charter, combined record,
approved decision, assessment and effect payload.

Preparation across the Owner commit has one explicit exception to normal Dream
policy capture. A `DreamApplied` payload carrying the exact
`CombinedCharterBinding` uses `ContextRequirement::NotApplicable` for the
generic policy fingerprint; the payload and authorization record instead bind
both charter identities/revisions/digests. `NotApplicable` actually omits an
otherwise present charter fingerprint. Every ordinary Dream still requires the
active policy fingerprint, and the normal Owner-decision kernel still checks
the independent current `ChangePolicyRecord` revision and digest.

The combined assessment comparison also uses policy NotApplicable. Its roots,
affected scope, products, costs, bounded fog and all non-charter semantic inputs
remain exact. Therefore the identical projection, payload, effect item, local
basis and comparison basis derive before and after the Owner charter commit.
There is no intervening `DreamRecalculated` write and no second approval.

Frozen zap/1 types and bytes are unchanged. The additive control class and
combined records are current zap/2 surfaces.
