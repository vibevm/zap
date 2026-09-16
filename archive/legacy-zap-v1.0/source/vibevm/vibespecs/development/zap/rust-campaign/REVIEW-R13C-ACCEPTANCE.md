# R13C service and transport acceptance

Root accepts the bounded service/transport implementation and repaired report
in REPORT-R13C-FINAL.md. The full requirement denominator remains R16; viewer
scale and the known prefix-scan correctness repair remain R17.

The fixed domain, control, economics and runtime completion providers are wired
into both runtime reads and CommitService. Staged packet and archive evidence
is verified outside writer transactions and rederived logically inside them.
Historical export uses indexed job insertion history and an exact checked event,
not a journal scan or present-day source recapture.

Root reviewed the provider composition, service construction, lease recovery,
ownership-safe Drop and the added archive surface. Recovery retains an existing
identity-matching RedbStore guard through the operation in both Open/Create
modes. Canonical exact ownership binds lease and endpoint; foreign or malformed
bytes are preserved, and interrupted matching recovery markers reconcile.
Endpoint unreachability alone cannot retire a live writer.

The protected service, HTTP and CLI expose commands, controls, observations,
native/runtime operations, effect preparation, reconciliation and archive
publish/verify/bounded entry reading. Archive publication accepts a committed
BundleId and derives its manifest and trusted receipt. The executed public
route journey advances Prepared to Ready and inspects actual packet content.
Timeouts retain exact unresolved command identity while work may still commit.

Accepted receipts: app library13/13; application and packet journeys2/2;
earlier relevant read/compiled-binary, timeout and exact-event lookup receipts;
scoped API/app/CLI clippy. Root reuses the reported relevant checks.

R16 MUST still demonstrate actual CampaignClosed refusal and acceptance through
this complete application, then compare it to runtime completion on identical
states. Wiring alone is not that proof. Native executor invocation, installed
consumer, all279 fact dispositions, remaining query/index work and release are
also unfinished. This acceptance does not close those requirements.
