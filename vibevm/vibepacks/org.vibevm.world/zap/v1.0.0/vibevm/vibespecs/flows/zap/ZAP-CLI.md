# ZAP CLI

Run Python 3.11+ with bytecode disabled:

```text
python -B vibevm/vibespecs/skills/zap-state/scripts/zap.py --help
```

`zap_state.py` remains compatible with the original five commands. The
projected `zap-run` skill provides a locator wrapper when package and skill
materializations live in different directories.
`capabilities.operations.runtime_profile_schema` is the machine-readable JSON
Schema draft 2020-12 contract for the profile shown below.
[The examples guide](../../examples/zap/README.md) distinguishes runnable
fixtures from state-bound templates.

## Prepare and inspect

```text
zap.py init --plan PLAN.toml --tasks-dir TASKS --out STORE
zap.py migrate-mup --plan PLAN.toml --tasks-dir TASKS --out STORE
zap.py inspect --store STORE
zap.py capabilities --store STORE
zap.py overview --store STORE --limit 100 --entity-kinds node,region,job,decision
zap.py subgraph --store STORE --id NODE --depth 2
zap.py search --store STORE --query recovery --kinds node,region,decision
zap.py detail --store STORE --kind node --id NODE
zap.py materialize-review-transition --store STORE --request SPARSE-TRANSITION.json
```

Import and migration create a new draft store. They never activate a charter,
start work or edit the MUP source. `record` is data-only and cannot bypass
control, action admission or trusted observation.

`materialize-review-transition` is a pure large-plan authoring helper. Its
sparse request names only changed obligation dispositions and preservation
choices; the output expands every omitted active obligation to a canonical
`retained` row. The sparse request is never journaled. A later
`domain.review-proposed` event contains the complete auditable transition.

## Trust and control

```text
zap.py trust-bootstrap --store STORE --trust-dir PRIVATE_TRUST
```

Trust material must live outside the campaign. Bootstrap creates protected
owner, coordinator and reader credential files and reports paths, never values.
Control/action flags take a credential ID and credential-file path. There is no
`--as-owner` label.

`charter-prepare --store STORE --charter CHARTER.json --out-dir PREPARED`
normalizes and validates the full charter, then writes exact draft and
next-revision activation command files. It does not append or activate them.

```text
zap.py control --store STORE --trust-config PRIVATE_TRUST/trust.json \
  --credential-id owner-credential \
  --credential-file PRIVATE_TRUST/owner-credential.token \
  --command ACTIVATE.json

zap.py action --store STORE --trust-config PRIVATE_TRUST/trust.json \
  --credential-id coordinator-credential \
  --credential-file PRIVATE_TRUST/coordinator-credential.token \
  --command TRANSITION.json --action ACTION.json --assessment ASSESSMENT.json
```

A full charter is drafted as data and activated through owner control at its
exact normalized hash. Imported authority labels do not count. An action binds
campaign/base, charter/policy revision, payload, sources and execution scope.

## Capture, snapshots, repair and promotion

```text
zap.py capture-source ... --artifact-store PRIVATE_ARTIFACTS \
  --path SOURCE --allowed-root ROOT --source-id SOURCE_ID
zap.py snapshot-create --store STORE --out SNAPSHOT.json
zap.py snapshot-load --store STORE --snapshot SNAPSHOT.json
zap.py repair-pending-tail ... --repair-id ID --expected-tail-sha256 HASH
zap.py promote-fact ... --promotion-id ID --fact-id FACT \
  --source-ref SOURCE --proof-ref EVIDENCE --target facts/FACT.json
```

Capture reads actual guarded bytes, stores them privately by hash, then records
the descriptor as a trusted observation. Adjudication remains an action.
Content reads require a registered handle and exact hash. Generic `observe`
refuses source-capture kinds; use `capture-source` so an editable descriptor
cannot stand in for bytes the adapter never read. Capturing changed bytes under
an existing source ID records `knowledge.source-recaptured` and invalidates its
known dependent facts; a newly extracted native-fact set uses a new source ID.

Repair requires owner authentication and quarantines original bytes before
switching to the committed prefix. Promotion reobserves sources, embeds portable
proof and records `domain.fact-promotion-recorded`.

## Runtime and backend

```text
zap.py tick --store STORE --trust-config TRUST \
  --host-principal-id coordinator-principal --profile runtime-profile.json \
  --artifact-store PRIVATE_ARTIFACTS
zap.py run --store STORE --trust-config TRUST \
  --host-principal-id coordinator-principal --profile runtime-profile.json \
  --artifact-store PRIVATE_ARTIFACTS
zap.py serve --store STORE --trust-config TRUST \
  --host-principal-id coordinator-principal \
  --artifact-store PRIVATE_ARTIFACTS --profile runtime-profile.json
```

The profile declares transport roots, allowed workspace roots, argv,
environment allowlists, resources, verifications, semantic coordinator and
poll/backoff. Any configured verification requires `artifact_capture.allowed_root`
in the trusted profile and the CLI private artifact store. Successful check
output is captured by bytes and explicitly assessed for applicability/closure
before evidence acceptance. Relative paths resolve against the profile directory. Custom worker argv
and every verification argv contain one whole `{packet_file}` element and are never shell-evaluated.
The example selects the configured gpt-5.6-sol/xhigh adapter without making
that model universal. Both `semantic.kind` and `worker.kind` accept
`codex-sol-xhigh`; `launcher: null` discovers `codexrunner` or
`codexrunner.ps1` on `PATH`, while an explicit launcher path is also accepted.
The loader resolves the package bridge itself and constructs the worker
transport with the provider authentication environment required by E's ready
profile. Its guarded defaults are `sandbox: workspace-write` and
`approval_policy: auto-review`; the ready helper exposes only read-only or
workspace-write and auto-review or never, with no bypass mode. A custom
`kind: argv` worker instead supplies one whole
`{packet_file}` argument.

For a runnable isolated release fixture, change to the installed
`zap-state/scripts` directory and prepare first:

```text
python -B -m zaplib.runtime_live_probe --root ISOLATED_DIRECTORY
python -B -m zaplib.runtime_live_probe --root ISOLATED_DIRECTORY \
  --execute --timeout-seconds 900
```

Preparation activates only the isolated probe campaign and creates no
transport/provider job or result artifact. `--execute` runs real worker/model
effects, generated-output capture, adjudication, acceptance and success-only
closure; it is an explicit release action and never runs during installation.

`serve` reports its active HTTP limits in `/v1/capabilities`.
`--max-body-bytes` selects the request capacity (`0` is unlimited), and
`--max-follow-seconds` bounds one live `/v1/follow` subscription. These are
operator transport settings rather than plan limits.

An active charter whose stop rules use semantic `eq` fields configures a
trusted nonblocking assessment process. Add paired `assessment_transport` and:

```json
{
  "assessment": {
    "kind": "json-process",
    "argv": ["python", "-B", "trusted-assessment.py", "{packet_file}"],
    "cwd": "replace-with-trusted-observer-root"
  }
}
```

The packet binds campaign/base/current revision, exact action/payload/source
captures, active policy revision/rules and only the required `eq` fields. The
response schema is `zap-runtime/assessment-response/1` and binds that request
and action; each required field is a JSON scalar or null. Provider execution is
durably cached by the exact request and polled without blocking coordinator
ticks. Pending or null values remain unknown, so the product action is not
admitted. Drain targets always come from current persisted runtime jobs, never
provider output. Keep the observer and its transport root in trusted startup
configuration outside worker-writable subjects; it receives no owner
credential or actor-label authority.

Installation starts nothing. `run` explicitly starts the persistent loop and
`serve` explicitly starts the HTTP backend. The default bind is localhost.
