# Root review of current-record observability

Accepted bounded source/read proof on 2026-09-14. Root inspected the traced
NoOp branch and the complete new direct/HTTP integration test, and reviewed
REPORT-R16-OBSERVABILITY-PROOF.md. No new public read API was necessary.

The existing projected-record method performs one exact registered record
lookup on the unchanged snapshot used by an explicit NoOp preparation. For a
present row, registry decoding validates its concrete type, codec, key and
version, then returns that record's full canonical serialized value. The
selector is not restricted to the basis roots. This supplies a common exact
field-view path for material record families alongside specialized navigation,
summary, runtime and event/history views; it does not claim every family was
individually exercised in a test.

The real application test records a proposed, unactivated Charter through the
Data route, reads its exact canonical bytes and revision1 directly and through
authenticated HTTP, and keeps head1. The preparation contains no effects or
affected scopes and has equal initial/final basis. A missing key in the same
family returns None; the test also preserves that an absent unknown-family row
returns None before registration lookup. Empty and4097-byte keys refuse with
no mutation. The exact case and full application_server3/3 pass with strict
target lint and formatting.

The view remains conditional on a valid basis. Empty-root Mutation is invalid;
empty-root Completion is allowed but can do global, budgeted basis work. A
valid local basis may be independent of the selected record. Neither choice
turns the record's value into its semantic proof. Selector keys are1–4096bytes;
the method has no independent value-byte cap, while HTTP transport has its own
configured request/response bounds. No cheap-global-read, activation, approval,
goal application, model invocation or mutation claim is inferred.

The published machine/backend documentation now states this actual recipe and
its limits. Final runtime-scale and installed-artifact checks remain separate.
