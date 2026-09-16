# Install Zap from local source with Vibe {#root}

The normal user command is `vibe install -g org.vibevm.zap/zap`, with matching
native `update -g` and `uninstall -g` operations. It requires a Vibe version
that supports global user applications. The Node workflow below is the advanced
local-registry bootstrap for source development, isolated acceptance and
explicit registry overrides; it is not the primary end-user spelling.

This path installs Zap Quick Lens as a user-local Vibe application from an
explicit local source registry. It does not publish or download a Zap binary
release, change `PATH`, start an agent, or alter provider account homes. The
installed runtime remains usable if the development checkout is later moved or
removed.

The normal host workspace is `<settings-dir>/opt/apps/zap`. Zap project data
continues to live separately under the selected user's Zap data directory.

## Native commands {#native}

With an updated source-installed Vibe:

```text
vibe install -g org.vibevm.zap/zap
zap-quicklens
```

For a server that serves the same workspace without opening a viewer, run
`zap-server`. Neither launch command starts agent work automatically.

```text
vibe update -g org.vibevm.zap/zap
vibe uninstall -g org.vibevm.zap/zap
```

`-g` selects the user application and leaves the current project's manifest and
dependencies alone. A source-installed Vibe uses its embedded source registry.
For a standalone developer build, add `--registry <local-registry>` to install
or update. This first global application implementation supports local and
embedded sources; remote-only application resolution is not implemented.
Uninstall uses the retained management entry and does not require the original
source registry. The public application recipe selects the public npm registry.

## Prerequisites {#prerequisites}

Install Node.js 24 or later and npm. The default installation also builds the
Zap engine and therefore requires the Rust and Cargo toolchain expected by the
local Vibe registry. Use `--lens-only` when only the TypeScript application is
needed.

On Windows, the default engine build also requires Visual Studio Build Tools
with **Desktop development with C++** and a compatible Windows SDK. Vibe passes
that toolchain environment into its build child. A Lens-only installation does
not build the Rust engine and does not require this C++ linker toolchain.

Use a Vibe executable that can materialize packages from the local registry.
An explicit `--vibe` path is useful for isolated verification. The installer
does not run a shell command by name when it can address the native executable
directly.

## Advanced source bootstrap {#install}

From the materialized Lens package or its source checkout, run:

```text
node tooling/install-source.mjs install --registry <local-registry> --settings-dir <settings-dir> --vibe <vibe-executable>
```

`install` is the default operation, so it may be omitted. Add `--offline` when
the Vibe, npm and Cargo dependency caches needed by this installation are
already present. Add `--lens-only` to omit the Rust engine.

The locked source dependencies use the canonical public npm registry. npm still
honors its ordinary protected configuration; for an isolated build, pass an
explicit credential-free HTTP(S) registry:

```text
node tooling/install-source.mjs install --registry <local-registry> --npm-registry https://registry.npmjs.org/ --settings-dir <settings-dir> --vibe <vibe-executable>
```

The URL is normalized and persisted in the marked installer host so an update
that omits `--npm-registry` retains the same choice. URLs containing a user,
password, query or fragment are refused; authentication belongs in npm's
protected configuration, never in installer argv or the marker.

## Manage an installation after moving the checkout {#management}

The installed Lens source slot retains the management entrypoint. After the
original checkout is removed, run status or uninstall through:

```text
node <settings-dir>/opt/apps/zap/vibevm/vibedeps/org.vibevm.zap.lens/0.1.0/tooling/install-source.mjs status --settings-dir <settings-dir>
node <settings-dir>/opt/apps/zap/vibevm/vibedeps/org.vibevm.zap.lens/0.1.0/tooling/install-source.mjs uninstall --settings-dir <settings-dir>
```

An update needs a currently available local source registry. Run the retained
entrypoint or a new checkout's entrypoint with `update --registry
<existing-local-registry>`. Do not rely on the marker's old registry path after
that checkout or registry has been removed.

When `--registry` is omitted from a monorepo checkout, the bootstrap searches
the package's standard repository ancestry for the local source registry. A
standalone package without that registry refuses with a useful error instead of
guessing a remote source.

The bootstrap creates a marked dedicated Vibe host, pins Lens 0.1.0 and, by
default, the engine 1.1.0, then asks Vibe to materialize those packages. It
explicitly runs the host build phase before deployment; materialization alone
does not create launcher inputs. The host's build extension creates an
immutable runtime generation. Its explicit package and deploy profiles publish
launchers under `<settings-dir>/opt/bin`.
Successful deployment is required before the installation reports ready.

## Inspect status without starting Zap {#status}

```text
node tooling/install-source.mjs status --settings-dir <settings-dir> --vibe <vibe-executable>
```

Status is read-only. It reports the marked host, pinned packages, current
generation and launcher deployment state. It does not start Wayfinder, a
browser, an agent, npm, Cargo or a model.

The command-line help is also read-only:

```text
node tooling/install-source.mjs --help
```

## Update transactionally {#update}

```text
node tooling/install-source.mjs update --registry <local-registry> --settings-dir <settings-dir> --vibe <vibe-executable>
```

Update re-materializes the pinned source packages, invokes the build phase,
creates a new immutable generation when its inputs changed, packages its
launchers and deploys them through Vibe. A failed materialization, build,
package or deploy phase is a
failure. It cannot be labelled ready and does not overwrite files used by an
already running generation.

Even a zero exit from the Vibe build command is insufficient by itself. The
bootstrap validates the prepared record, immutable generation and launcher
integrity before it invokes deploy.

Old generations remain as installer-host build cache. Their retention is
intentional because a global launcher may still address an immutable payload
inside the dedicated host.

## Installed commands {#commands}

The deployed launchers provide the package's ten ordinary commands:

- `codlens`
- `codlens-mcp`
- `quicklens-service`
- `quicklens-web-auth`
- `quicklens-web`
- `zap-wayfinder`
- `zap-server`
- `zap-quicklens`
- `zap-quick-lens`
- `zap-mock-agent`

`zap-quicklens` is the short product command; `zap-quick-lens` is its retained
legacy alias. `zap-server` runs the same normal product/HTTP stack without
opening a viewer. `zap-wayfinder` remains the advanced explicit-config command.

The default engine installation also provides `zap`. The installer generates
Windows command and PowerShell launchers plus POSIX launchers with structured
argument forwarding. It does not add the launcher directory to `PATH`; invoke
the explicit file or add the directory through your ordinary user-managed
environment process.

## Uninstall launchers safely {#uninstall}

```text
node tooling/install-source.mjs uninstall --settings-dir <settings-dir> --vibe <vibe-executable>
```

Uninstall asks Vibe to undeploy only receipt-owned launchers. It retains the
marked installer host, materialized source slots, immutable generation cache,
Zap project state and provider account data. A launcher that is unowned or was
modified outside its receipt is refused rather than deleted.

The bootstrap also refuses an unrelated nonempty application directory or a
marker that names another application. Do not remove or replace the marker to
force adoption of an existing directory; select another settings root or move
the unrelated data explicitly.

## Isolated acceptance {#verification}

For verification, choose an empty temporary settings directory and pass the
local source registry explicitly. Run install, status, installed `--help`,
update and uninstall from that isolated root. Confirm that the installed
command still works after the original checkout is made unavailable, and that
uninstall removes receipt-owned launchers while leaving application cache and
user state intact.

The registered deterministic source-install simulation uses injected process
ports and a fake builder. It covers structured argv, spaces in paths, ownership
refusal, failure boundaries and retained data without performing a real global
installation or model call. Root acceptance remains the evidence for actual
Vibe materialization, package, deploy and undeploy behavior. The completed
[Windows source-install acceptance](research/ZAP-SOURCE-INSTALL-ACCEPTANCE-2026-09-16.md)
records the actual build, deployment, browser startup, update, source-removal
proof and data-preserving uninstall.
The subsequent [native command acceptance](research/ZAP-NATIVE-INSTALL-ACCEPTANCE-2026-09-16.md)
covers `-g`, the short launchers and their shared running server.
