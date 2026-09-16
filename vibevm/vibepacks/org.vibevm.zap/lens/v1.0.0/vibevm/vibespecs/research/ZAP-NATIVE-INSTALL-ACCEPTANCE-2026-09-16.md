# Native application commands acceptance, 2026-09-16 {#root}

The updated Vibe CLI installed the real Zap application using
`install -g org.vibevm.zap/zap` with an explicit private source registry under
isolated user settings. It built and deployed all eleven commands as 22 Windows
launcher files. The generated CLI JSON report parsed as one document, and the
generic application index bound the immutable retained management entry.

Native `update -g` succeeded. Both operations ran from a separate Vibe consumer
project: its manifest, sentinel contents and two-file directory remained
unchanged. No project dependency, lock or boot artifact was introduced there.

Deployed `zap-quicklens` and `zap-server` help returned without creating state.
The headless server served the real onboarding and execution catalog in Chrome
without page errors. Running `zap-quicklens --no-open` against that same state
reused the existing owner and database. `zap capabilities` returned structured
engine capabilities. The test started no agents or model turns, and its owned
server processes were closed.

With the selected private source registry temporarily renamed away,
`uninstall -g org.vibevm.zap/zap` succeeded using the retained management script.
It removed the 22 receipt-owned launchers while preserving the runtime cache,
application state sentinel and unrelated file in the command directory.

The command without `--registry` also installed and uninstalled Zap through
embedded-source discovery. That case used an explicitly labelled isolated VVM
metadata fixture containing the real compiled CLI and an actual source-tree
binding. It verifies default application source selection, not VVM activation
or a two-binary distribution installation. No normal user VVM activation, PATH
or account configuration was changed.

Focused verification passed two manifest tests, two generic fake-application
CLI tests, one registered report wire test, 21 Lens/installer/short-command tests,
and the registered source-install simulation including the application adapter.
Production/test TypeScript, lint, formatting and Lens conform checks passed.
Vibe and vibe-index were built from the changed source. All product and
simulation checks used zero LLM inference.

The broader host specmap regeneration separately exposed existing traceability
debt in untouched embedded-source and submodule-publication code. It reported
five gated orphans and unrelated index drift, with no unresolved host edge.
That work is recorded as host backlog B-126; this acceptance does not claim a
clean repository-wide specmap gate. The Lens-specific map has no suspects or
orphans.

The production scope is local/embedded source application installation on the
updated Vibe implementation, with existing project-scoped command semantics
preserved. Windows x64 was tested directly; real Linux/macOS installations and
remote-only application registries are not claimed here.
