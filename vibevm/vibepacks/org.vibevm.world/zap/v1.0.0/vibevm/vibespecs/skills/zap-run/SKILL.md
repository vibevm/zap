---
name: zap-run
description: Import, configure, explicitly activate, and operate a trusted ZAP campaign, coordinator, and authenticated graph backend.
---

# ZAP runtime

Use this skill for execution, reconciliation, backend serving and trusted
control. Read [the runtime contract](../../flows/zap/ZAP-RUNTIME.xml), the
[backend API](../../flows/zap/ZAP-BACKEND-API.md), and the
[CLI reference](../../flows/zap/ZAP-CLI.md). An embedding host also reads the
[Python API](../../flows/zap/ZAP-PYTHON-API.md).

Run `python -B scripts/zap-run.py -- <command> ...`. The wrapper resolves the
package runtime from an explicit `--package-root`, the installed package slot,
or an unambiguous sibling `zap-state` projection. If several installations are
visible, pass `--package-root` rather than guessing.

Import and trust bootstrap are separate. Import never activates or runs work:

```text
python -B scripts/zap-run.py -- init --plan PLAN --tasks-dir TASKS --out STORE
python -B scripts/zap-run.py -- trust-bootstrap --store STORE --trust-dir PRIVATE_TRUST
```

Prepare a full charter with exact campaign/base, legacy classifications and
intent fingerprint. Submit its draft through the data route, then activate the
exact charter hash through the owner credential. Do not put credential values
in JSON, argv, environment, packets or the campaign journal; CLI flags name
protected credential files.

Use `tick` for one reconciliation pass and `run` for the persistent loop.
Worker and semantic commands come from an explicit runtime profile. Custom
commands are argv arrays with one whole `{packet_file}` element; task prose is
never shell evaluated. The ready `codex-sol-xhigh` worker/semantic kinds resolve the
package bridges and discover `codexrunner` on PATH unless the profile supplies
an explicit launcher. A profile with verifications also configures a guarded
artifact root and the CLI private artifact store, so accepted evidence can bind
captured output bytes. Transport receipts are trusted observations, while dispatch,
verification, adjudication and acceptance remain privileged actions.

If the active stop policy uses semantic `eq` fields, configure the paired
trusted JSON-process assessment adapter/transport from the CLI reference.
Pending, failed or null observations remain unknown and block only the affected
action; the adapter cannot supply authority or drain targets.

Serve the backend on localhost by default. A nonlocal bind requires the explicit
flag and exact allowed origins. Readers use a separate campaign-bound read
credential. The viewer application is not part of this package; the backend
provides its paginated graph, inspector, content and event-stream data.

Never start or migrate the live NEXT campaign merely because this package is
installed. Use a separately imported store until the owner explicitly selects
and activates a migration.
