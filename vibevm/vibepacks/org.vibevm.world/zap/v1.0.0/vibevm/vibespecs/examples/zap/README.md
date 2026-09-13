# ZAP examples

The files in this directory have two different purposes.

`stop-rules.json` is an explicitly unapproved legacy simulation fixture.
`command-contract.json` describes the current public command surface.
`runtime-profile.json` is a profile template: replace workspace roots, then use
it with `--artifact-store`. Its ready worker and semantic adapters discover
`codexrunner` on `PATH` when `launcher` is null. The ready worker uses guarded
`workspace-write` plus `auto-review`; no bypass mode is available. A `.ps1`
launcher requires `pwsh`.
For a charter with semantic `eq` stop fields, extend the profile with paired
`assessment_transport` and `assessment` JSON-process objects as documented in
ZAP-CLI; missing assessment values intentionally remain unknown.

The remaining JSON files are state-bound templates and are deliberately not
standalone commands:

- `charter.json` needs the real campaign/base, the fingerprint of an existing
  `domain.intent-proposed` payload, and one classification for every imported
  mandate. `charter-prepare` validates and generates the exact draft and
  activation command files.
- `action.json` shows only the action and assessment fragments. `zap.py action`
  also requires the separate exact product command whose payload hash the
  action binds. Use capabilities for that event's current payload descriptor.
- `backend-config.json` is an illustrative projection of `zap.py serve` flags;
  no CLI command reads this JSON file. Pass those values as documented flags.
- `sparse-review-transition.json` needs the ID of a proposed direct successor
  outcome and actual preservation choices. `materialize-review-transition`
  expands it without appending; only the resulting full transition belongs in
  a later `domain.review-proposed` command.

For a runnable isolated end-to-end fixture, change to the installed
`zap-state/scripts` directory (the directory containing `zap.py`) and use the
packaged live probe:

```text
python -B -m zaplib.runtime_live_probe --root ISOLATED_DIRECTORY
python -B -m zaplib.runtime_live_probe --root ISOLATED_DIRECTORY --execute
```

The first command prepares and activates only the isolated fixture and creates
no transport/provider job or result artifact. `--execute` uses the configured
ready bridges and may run external model/worker effects; the caller supplies an
explicit `--launcher` when PATH discovery is unsuitable. Root owns live release
execution; installing ZAP never runs this probe.
