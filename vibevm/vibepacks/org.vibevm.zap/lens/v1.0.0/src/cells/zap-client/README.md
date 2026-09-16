# Trusted ZAP client cell

This Node-only cell calls one explicitly configured loopback ZAP application
endpoint. Construction requires an injected HTTP exchange and one configured
credential ID/Bearer pair. The cell reads no environment variables, files, or
global credential state. Credentials appear only in request headers and are
never included in returned DTOs or canonical payloads.

The public seam covers:

- capabilities, snapshot, bounded event pages, and one-page POST SSE reads;
- generic registered queries and `zap.planning.active-context.v1`;
- effect-bundle, comparison, and projected-record preparation;
- protected `command`, `control`, `agent`, and `observation` submissions;
- public staged change-admission orchestration;
- exact command reconciliation.

`canonicalQueryInput` emits codec-2 UTF-8 byte arrays with sorted object keys.
Callers use `bigint` for unsigned wire inputs. Response integers normalize to
branded decimal strings, so revisions and counters above `2^53` remain exact.
Malformed UTF-8/JSON, duplicate keys, unsafe JavaScript integers, foreign
response kinds, response-size overflow, and mismatched snapshot/cursor identity
all refuse explicitly.

Protected submission accepts the exact Rust `CommandFrameInput` shape plus its
expected `command_id` and `command_digest`. The client recomputes the canonical
SHA-256 command digest before sending. ZAP's current Rust compatibility preimage
has one narrow rule: numeric tokens in the decoded payload are serialized as
serde_json's arbitrary-precision `{"$serde_json::private::Number":"<token>"}`
value before the typed `{header,reason,payload}` frame is canonically encoded.
Header and reason numbers remain ordinary JSON numbers. Generic canonical
payload bytes are neither wrapped nor changed on the wire. This behavior is
isolated to `protectedCommandDigest` and covered by Rust-produced goldens for a
large header revision, nested integers and a u64-max payload value. Rust codec 2
rejects command payload floats and negative zero; the digest helper rejects the
same captured refusal fixtures without narrowing generic query JSON.

The client performs one request only. Transport
loss or an untrusted success response returns `uncertain_submission` with
`reconcile_required: true`; the caller then uses `/v1/reconcile` before deciding
whether another submission is safe.

This seam preserves the ZAP backend's authority, admission, economics, and hold
decisions. Higher-level authoring and workflow cells use only registered public
preparation, protected metadata, change-admission and reconciliation routes;
ServiceInternal frames never cross this client boundary.

Renderer code must consume the browser-safe `QuicklensDataSource` through a
trusted composition adapter. It must not import this cell, its credentials,
Node crypto, or the injected HTTP exchange.
