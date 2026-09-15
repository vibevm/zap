# Agent plan authoring protocol {#root}

`guide r3`

This protocol is the trusted Node boundary between semantic agent input and
ZAP's public reader, data-proposal and coordinator interfaces. It never exposes
those credentials to an MCP model and never uses Owner authority.

## Discover the intent basis {#context}

`guide r2`

Capture the current specification digest and query ZAP capabilities, active
planning context and bounded economics context. A unique applicable baseline is
required before assessment authoring. Absence, ambiguity, truncation, missing
capability and changed source bytes are typed refusals. The resulting intent
basis records the original request boundary; it is not reused as the execution
revision after metadata commits.

## Submit metadata through public routes {#metadata}

`guide r2`

Derive the milestone-plan basis through a no-effect preparation, compute the
canonical plan fingerprint, and submit `MilestonePlanProposed` with the data
credential. Prepare the selected product comparison, consume its backend-derived
effect bases, preflight and aggregate affected closure, then submit
`ChangeAssessmentProposed`. Read the normalized assessment through the public
projected-record operation. No client closure, impact or economics authority
algorithm substitutes for those backend results.

For a successor that adds or revises milestones, first prepare typed precursor
drafts. Create identities and all effect/product identities are deterministic
from the authoring operation; revise drafts query the public milestone head and
bind its revision, current revision identity and semantic fingerprint. The
read-only composite preparation projects those registered effects and validates
the successor against the resulting state. Persist its complete response before
the data-authorized dormant-candidate record call, then reconcile that exact
candidate command before assessment preparation.

## Preserve receipts and prepare each effect {#effects}

`guide r3`

Persist the original intent basis, fresh prepared execution basis, metadata
receipts, normalized assessment digest and immutable selected effects. Before
each admission, refresh context, prepare exactly the remaining ordered effects
with their committed prefix, and build the next product command for the current
revision. A held Owner decision is supplied later only as its public decision
identity. The same assessment and alternative survive every effect.

Milestone creation or revision precursors and final plan adoption remain one
ordered alternative under one assessment. The data-proposal credential records
only the dormant plan candidate and assessment metadata. The coordinator admits
and applies each registered `plan.lower` product in order; it cannot substitute
different precursor bytes or append effects outside the composite binding.
Display names never stand in for public record identities.

## Reconcile without hidden authority {#reconciliation}

`guide r2`

Every protected proposal and product carries a locally recomputed canonical
command digest. Transport loss returns the exact reconciliation identity; it is
never retried blindly. Admission loss retries the byte-identical operation.
Only an authenticated human adapter may create an Owner decision command.

Rust's command-digest preimage serializes arbitrary-precision numeric tokens in
the decoded payload through serde_json's private Number wrapper. The compatible
client digest reproduces that wrapper only in the digest preimage. Canonical
payload bytes sent on the wire remain ordinary codec-2 JSON and are never
rewritten to contain the wrapper object.
