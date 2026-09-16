/** User-facing source-install command launchers. @scope spec://org.vibevm.zap/lens/PROP-016#commands */
import { join } from "node:path";

export const PACKAGE_COMMANDS = Object.freeze([
  ["codlens", "dist/cli.js"],
  ["codlens-mcp", "dist/mcp.js"],
  ["quicklens-service", "dist/quicklens.js"],
  ["quicklens-web-auth", "dist/quicklens-web-auth.js"],
  ["quicklens-web", "dist/quicklens-web.js"],
  ["zap-wayfinder", "dist/wayfinder.js"],
  ["zap-quick-lens", "dist/zap-quick-lens.js"],
  ["zap-mock-agent", "dist/zap-mock-agent.js"],
]);

export function validatePackageCommands(packageDocument) {
  const bins = packageDocument?.bin;
  if (bins === null || typeof bins !== "object" || Array.isArray(bins))
    failure("package bin table is missing");
  const expected = new Map(PACKAGE_COMMANDS.map(([name, target]) => [name, `./${target}`]));
  if (Object.keys(bins).length !== expected.size)
    failure("package bin table differs from the source-install launcher set");
  for (const [name, target] of expected) {
    if (bins[name] !== target) failure(`package bin ${name} has an unexpected target`);
  }
}

export function renderLaunchers(input) {
  const rows = [];
  for (const [command, relativeEntry] of PACKAGE_COMMANDS) {
    const entry = join(input.runtimeRoot, relativeEntry);
    rows.push(...launcherFamily(command, input.nativeNode, entry));
  }
  if (input.enginePath !== null) rows.push(...launcherFamily("zap", input.enginePath, null));
  return rows;
}

export function launcherFileNames(engineEnabled) {
  const commands = [
    ...PACKAGE_COMMANDS.map(([command]) => command),
    ...(engineEnabled ? ["zap"] : []),
  ];
  return commands.flatMap((command) => [`${command}.cmd`, `${command}.ps1`, command]);
}

function launcherFamily(command, executable, entry) {
  const args = entry === null ? [] : [entry];
  return [
    {
      command,
      platform: "windows-cmd",
      fileName: `${command}.cmd`,
      mode: 0o644,
      body: cmd(command),
    },
    {
      command,
      platform: "windows-powershell",
      fileName: `${command}.ps1`,
      mode: 0o644,
      body: powershell(executable, args),
    },
    {
      command,
      platform: "posix",
      fileName: command,
      mode: 0o755,
      body: posix(executable, args),
    },
  ];
}

function cmd(command) {
  return [
    "@echo off",
    `rem vibe:zap source-install launcher command=${command}`,
    `powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0${command}.ps1" %*`,
    "exit /b %ERRORLEVEL%",
    "",
  ].join("\r\n");
}

function powershell(executable, prefix) {
  const rendered = [executable, ...prefix].map(psQuote).join(" ");
  return [
    `\uFEFF# vibe:zap source-install launcher command=${prefix.length === 0 ? "zap" : "node"}`,
    "$ErrorActionPreference = 'Stop'",
    `& ${rendered} @args`,
    "$childExit = $LASTEXITCODE",
    "if ($null -eq $childExit) { $childExit = 1 }",
    "exit $childExit",
    "",
  ].join("\r\n");
}

function posix(executable, prefix) {
  return [
    "#!/bin/sh",
    `# vibe:zap source-install launcher command=${prefix.length === 0 ? "zap" : "node"}`,
    `exec ${[executable, ...prefix].map(shQuote).join(" ")} "$@"`,
    "",
  ].join("\n");
}

function psQuote(value) {
  return `'${value.replaceAll("'", "''")}'`;
}

function shQuote(value) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-016#root: ${message}; fix surface: keep package.json bin entries aligned with source-install launchers`,
  );
}
