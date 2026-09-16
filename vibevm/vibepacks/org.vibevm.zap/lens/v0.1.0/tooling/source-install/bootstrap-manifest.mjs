/** Vibe host manifest renderer. @scope spec://org.vibevm.zap/lens/PROP-016#lifecycle */
export const LENS_COORDINATE = Object.freeze({
  kind: "tool",
  group: "org.vibevm.zap",
  name: "lens",
  version: "0.1.0",
});
export const ENGINE_COORDINATE = Object.freeze({
  kind: "flow",
  group: "org.vibevm.zap",
  name: "zap",
  version: "1.1.0",
});
export const LENS_COMMANDS = Object.freeze([
  "codlens",
  "codlens-mcp",
  "quicklens-service",
  "quicklens-web-auth",
  "quicklens-web",
  "zap-wayfinder",
  "zap-quick-lens",
  "zap-mock-agent",
]);

export function renderBootstrapHost(plan) {
  const commands = plan.lensOnly ? [...LENS_COMMANDS] : [...LENS_COMMANDS, "zap"];
  return {
    manifest: renderManifest(plan, commands),
    powershellHook: renderPowerShellHook(plan),
    posixHook: renderPosixHook(plan),
    commands,
  };
}

function renderManifest(plan, commands) {
  const packages = [
    `"${coordinateKey(LENS_COORDINATE)}" = "=${LENS_COORDINATE.version}"`,
    ...(plan.lensOnly
      ? []
      : [`"${coordinateKey(ENGINE_COORDINATE)}" = "=${ENGINE_COORDINATE.version}"`]),
  ];
  const windows = commands.flatMap((command) => [`${command}.ps1`, `${command}.cmd`]);
  const posix = [...commands];
  const packageRows = [
    ...windows.map((filename) => packageRow(filename, ["windows"])),
    ...posix.map((filename) => packageRow(filename, ["linux", "macos"])),
  ].join("\n");
  const deployRows = [...windows, ...posix].map(deployRow).join("\n");
  const buildInputs = [
    ".vibe/zap-source-install.json",
    "hooks/**",
    "vibe.toml",
    `${lensSlot()}/LICENSE.md`,
    `${lensSlot()}/README.md`,
    `${lensSlot()}/package.json`,
    `${lensSlot()}/package-lock.json`,
    `${lensSlot()}/vibe.toml`,
    `${lensSlot()}/src/**`,
    `${lensSlot()}/integrations/**`,
    `${lensSlot()}/tooling/**`,
    `${lensSlot()}/docs/**`,
    `${lensSlot()}/vibevm/vibespecs/**`,
    `${lensSlot()}/quicklens.*.config.ts`,
    `${lensSlot()}/tsconfig*.json`,
    `${lensSlot()}/vitest.config.ts`,
    `${lensSlot()}/.prettierrc*.json`,
    `${lensSlot()}/.vibeignore*`,
    `${lensSlot()}/conform*.toml`,
    `${lensSlot()}/eslint.config.*`,
    ...(plan.lensOnly
      ? []
      : [
          `${engineSlot()}/vibe.toml`,
          `${engineSlot()}/Cargo.toml`,
          `${engineSlot()}/Cargo.lock`,
          `${engineSlot()}/crates/**`,
        ]),
  ];
  return `[project]
name = "zap-user-install"
version = "1.0.0"

[[registry]]
name = "zap-source"
url = "${escapeToml(plan.registryUrl)}"

[requires]
packages = { ${packages.join(", ")} }

[[extension]]
id = "build-zap-source-install"
point = "phase:build"
handler = { kind = "script", base = "hooks/build-zap-source" }
inputs = [
${buildInputs.map((input) => `  "${input}"`).join(",\n")}
]

${packageRows}
${deployRows}
[deploy.profiles.windows]
targets = [${windows.map((filename) => `"deploy-${targetId(filename)}"`).join(", ")}]

[deploy.profiles.posix]
targets = [${posix.map((filename) => `"deploy-${targetId(filename)}"`).join(", ")}]
`;
}

function packageRow(filename, operatingSystems) {
  const id = targetId(filename);
  return `[[artifacts.package]]
id = "package-${id}"
mechanism = "package:static-file"
when = { os = [${operatingSystems.map((value) => `"${value}"`).join(", ")}] }
inputs = [{ path = "target/zap-source-install/launchers/${filename}" }]
outputs = [{ id = "${filename}", kind = "file" }]
`;
}

function deployRow(filename) {
  const dependency = filename.endsWith(".cmd")
    ? `depends_on = ["deploy-${targetId(filename.replace(/\.cmd$/, ".ps1"))}"]\n`
    : "";
  return `[[deploy.target]]
id = "deploy-${targetId(filename)}"
artifact = "${filename}"
mechanism = "deploy:vibe-opt-launcher"
${dependency}
`;
}

function renderPowerShellHook(plan) {
  const node = quotePowerShell(plan.nodeExecutable);
  const helper = quotePowerShell(`${lensSlot()}\\tooling\\source-install\\build.mjs`);
  return `\uFEFF$ErrorActionPreference = 'Stop'
$root = $env:VIBE_PROJECT_ROOT
if ([string]::IsNullOrWhiteSpace($root)) { $root = (Get-Location).Path }
$env:VIBE_PROJECT_ROOT = $root
$helper = Join-Path $root ${helper}
$startInfo = New-Object System.Diagnostics.ProcessStartInfo
$startInfo.FileName = ${node}
$startInfo.Arguments = '"' + $helper + '"'
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$child = New-Object System.Diagnostics.Process
$child.StartInfo = $startInfo
if (-not $child.Start()) { throw 'Node build helper did not start' }
$child.WaitForExit()
$code = $child.ExitCode
$child.Dispose()
exit $code
`;
}

function renderPosixHook(plan) {
  const node = quotePosix(plan.nodeExecutable);
  return `#!/bin/sh
set -eu
root="\${VIBE_PROJECT_ROOT:-$PWD}"
export VIBE_PROJECT_ROOT="$root"
helper="$root/${lensSlot()}/tooling/source-install/build.mjs"
exec ${node} "$helper"
`;
}

function coordinateKey(value) {
  return `${value.kind}:${value.group}/${value.name}`;
}
function lensSlot() {
  return `vibevm/vibedeps/${LENS_COORDINATE.group}.${LENS_COORDINATE.name}/${LENS_COORDINATE.version}`;
}
function engineSlot() {
  return `vibevm/vibedeps/${ENGINE_COORDINATE.group}.${ENGINE_COORDINATE.name}/${ENGINE_COORDINATE.version}`;
}
function targetId(filename) {
  return filename.replaceAll(".", "-");
}
function quotePowerShell(value) {
  return `'${value.replaceAll("'", "''")}'`;
}
function quotePosix(value) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}
function escapeToml(value) {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
