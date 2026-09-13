# ZAP backend implementation API

Public behavior is maintained in
`../../flows/zap/ZAP-BACKEND-API.md`.

Internal composition:

- `Engine(store, trust, host_principal=None)` composes 82 current core,
  control, domain, knowledge and runtime handlers.
- `ENGINE_DATA_KINDS`, `ENGINE_ACTION_KINDS` and
  `ENGINE_OBSERVATION_KINDS` partition every extension handler exactly.
- `BackendApplication(engine, artifacts=None, coordinator=None)` is
  framework-independent.
- `create_server(application, host='127.0.0.1', port=0,
  allow_nonlocal=False, allowed_origins=())` creates the standard-library
  threaded HTTP adapter.

Every payload descriptor reports its dialect and version. Knowledge events use
strict nested JSON Schema draft 2020-12 from `knowledge_schemas`; other event
maps use `zap-payload-descriptor/1`, a required-field/constraint family that is
not claimed as standards-complete JSON Schema.

The HTTP adapter authenticates before dispatch. Backend reads use committed
replay even with a pending final fragment; service mutations and the coordinator
refuse that fragment. Public-state/event sanitization removes credential-like
material, absolute source roots, raw semantic responses and embedded worker
packets. Generic HTTP/CLI observation routes reject source capture kinds; only
the guarded byte-capture adapter can submit those trusted descriptors.
Validated semantic responses retain only `zap-public-semantic-decision/1`:
request/disposition/selection, bounded public rationale, command reasons,
affected IDs and provenance. Full command payloads and provider material remain
redacted.
Responses and SSE frames use the shared tagged wire codec; request bodies use
the shared duplicate/nonfinite-rejecting parser. `/v1/stream` is the finite
tail batch, while `/v1/follow` is the bounded live subscription. Active body
and follow capacities are operator configuration returned by capabilities.
