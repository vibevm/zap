/** User-local Vibe source-install bootstrap. @scope spec://org.vibevm.zap/lens/PROP-016#commands */
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  renderBootstrapHost,
  ENGINE_COORDINATE,
  LENS_COMMANDS,
  LENS_COORDINATE,
} from "./bootstrap-manifest.mjs";
import { prepareSourceRegistrySnapshot } from "./bootstrap-snapshot.mjs";
import {
  binaryGenerationCacheOnly,
  directoryEntries,
  exists,
  HOST_MARKER_RELATIVE,
  inspectBootstrap,
  PREPARED_RELATIVE,
  readJson,
  SOURCE_INSTALL_ID,
  SOURCE_INSTALL_PROTOCOL,
  validateMarker,
} from "./bootstrap-status.mjs";
export {
  inspectBootstrap,
  SOURCE_INSTALL_ID,
  SOURCE_INSTALL_PROTOCOL,
} from "./bootstrap-status.mjs";
export { renderBootstrapHost } from "./bootstrap-manifest.mjs";
export const MARKER_RELATIVE = HOST_MARKER_RELATIVE;
const COMMANDS = new Set(["install", "update", "status", "uninstall"]);

export function parseBootstrapArgs(argv, env = {}) {
  const values = [...argv];
  let command = "install";
  if (values[0] !== undefined && !values[0].startsWith("-")) {
    command = values.shift();
    if (!COMMANDS.has(command)) return failure(`unknown operation '${command}'`);
  }
  const options = {
    command,
    registry: null,
    npmRegistry: null,
    npmRegistrySpecified: false,
    settingsDir: null,
    vibe: null,
    offline: false,
    offlineSpecified: false,
    lensOnly: false,
    lensOnlySpecified: false,
    help: false,
    env,
  };
  for (let index = 0; index < values.length; index += 1) {
    const argument = values[index];
    if (argument === "--offline") {
      options.offline = true;
      options.offlineSpecified = true;
    } else if (argument === "--lens-only") {
      options.lensOnly = true;
      options.lensOnlySpecified = true;
    } else if (argument === "--help" || argument === "-h") options.help = true;
    else if (["--registry", "--npm-registry", "--settings-dir", "--vibe"].includes(argument)) {
      const value = values[index + 1];
      if (value === undefined || value.startsWith("-"))
        return failure(`${argument} requires a value`);
      index += 1;
      const field =
        argument === "--registry"
          ? "registry"
          : argument === "--npm-registry"
            ? "npmRegistry"
            : argument === "--settings-dir"
              ? "settingsDir"
              : "vibe";
      if (options[field] !== null) return failure(`${argument} may be supplied only once`);
      options[field] = value;
      if (field === "npmRegistry") options.npmRegistrySpecified = true;
    } else return failure(`unknown option '${argument}'`);
  }
  return { ok: true, value: options };
}

export async function resolveBootstrapPlan(options, ports) {
  const nodeMajor = Number.parseInt(String(ports.nodeVersion).split(".")[0] ?? "0", 10);
  if (!Number.isInteger(nodeMajor) || nodeMajor < 24)
    return failure("Zap source installation requires Node.js 24 or later");
  const settingsInput =
    options.settingsDir ?? nonempty(options.env.VIBE_SETTINGS) ?? join(ports.homeDir, ".vibe");
  const settingsDir = resolve(settingsInput);
  const installerRoot = join(settingsDir, "opt", "apps", "zap");
  const registry =
    options.registry ?? (await discoverRegistry(dirname(ports.modulePath), ports.fs));
  if (registry === null)
    return failure(
      "no local Vibe registry was found; pass --registry PATH for a standalone installer",
    );
  const registryDir = await existingDirectory(registry, "local Vibe registry", ports.fs);
  if (!registryDir.ok) return registryDir;
  const npmRegistry = validateNpmRegistry(options.npmRegistry);
  if (!npmRegistry.ok) return npmRegistry;
  const vibe = await resolveVibeExecutable(options.vibe, options.env, ports);
  if (!vibe.ok) return vibe;
  const profile = ports.platform === "win32" ? "windows" : "posix";
  const plan = {
    operation: options.command,
    settingsDir,
    installerRoot,
    registryDir: registryDir.value,
    registryUrl: pathToFileURL(join(installerRoot, "vibevm", "vibepacks")).href,
    vibeExecutable: vibe.value,
    nodeExecutable: resolve(ports.nodeExecutable),
    offline: options.offline,
    npmRegistry: npmRegistry.value,
    lensOnly: options.lensOnly,
    profile,
    markerPath: join(installerRoot, MARKER_RELATIVE),
    preparedPath: join(installerRoot, PREPARED_RELATIVE),
    lensSourceRoot: join(
      registryDir.value,
      LENS_COORDINATE.group,
      LENS_COORDINATE.name,
      `v${LENS_COORDINATE.version}`,
    ),
    engineSourceRoot: join(
      registryDir.value,
      ENGINE_COORDINATE.group,
      ENGINE_COORDINATE.name,
      `v${ENGINE_COORDINATE.version}`,
    ),
    localRegistryRoot: join(installerRoot, "vibevm", "vibepacks"),
    temporarySuffix: String(ports.processId ?? "bootstrap"),
    marker: markerFor({
      settingsDir,
      registryDir: registryDir.value,
      offline: options.offline,
      npmRegistry: npmRegistry.value,
      lensOnly: options.lensOnly,
      vibeExecutable: vibe.value,
    }),
  };
  return { ok: true, value: plan };
}

export function renderHostFiles(plan) {
  const rendered = renderBootstrapHost(plan);
  return [
    { relative: "vibe.toml", content: rendered.manifest },
    { relative: "hooks/build-zap-source.ps1", content: rendered.powershellHook },
    { relative: "hooks/build-zap-source.sh", content: rendered.posixHook },
    { relative: MARKER_RELATIVE, content: `${JSON.stringify(plan.marker, null, 2)}\n` },
  ];
}

export async function executeBootstrap(options, ports) {
  try {
    return await executeBootstrapInner(options, ports);
  } catch (error) {
    return resultFailure(
      options.command,
      `source installation failed: ${safeMessage(error)}`,
      null,
      [],
    );
  }
}

async function executeBootstrapInner(options, ports) {
  if (options.command === "status") return statusResult(await inspectBootstrap(options, ports));
  const ownership = await inspectBootstrap(options, ports);
  if (!ownership.ok) return resultFailure(options.command, ownership.error.message, null, []);
  if (options.command === "uninstall") {
    if (ownership.value.marker === null)
      return resultFailure(
        "uninstall",
        "Zap source installation is absent",
        ownership.value.installerRoot,
        [],
      );
    const vibe = await resolveVibeExecutable(
      options.vibe ?? ownership.value.marker.engine.vibeExecutable,
      options.env,
      ports,
    );
    if (!vibe.ok)
      return resultFailure("uninstall", vibe.error.message, ownership.value.installerRoot, []);
    const command = vibeCommand(
      vibe.value,
      "undeploy",
      ownership.value.installerRoot,
      ownership.value.marker.offline,
      profileFor(ports.platform),
    );
    const executed = await ports.process.run(command.file, command.args, {
      cwd: ownership.value.installerRoot,
      env: childEnvironment(options.env, ownership.value.settingsDir),
    });
    return executed.code === 0
      ? {
          ok: true,
          code: 0,
          operation: "uninstall",
          state: "undeployed",
          installerRoot: ownership.value.installerRoot,
          message:
            "receipt-owned Zap launchers were undeployed; cached sources and user data were retained",
          commands: [command],
        }
      : resultFailure(
          "uninstall",
          processFailure("vibe undeploy", executed),
          ownership.value.installerRoot,
          [command],
        );
  }
  if (options.command === "update" && ownership.value.marker === null)
    return resultFailure(
      "update",
      "Zap source installation is absent; run install first",
      ownership.value.installerRoot,
      [],
    );
  const existingMarker = ownership.value.marker;
  const effectiveOptions = {
    ...options,
    registry: options.registry ?? existingMarker?.registryDir ?? null,
    vibe: options.vibe ?? existingMarker?.engine.vibeExecutable ?? null,
    offline:
      options.offlineSpecified || existingMarker === null
        ? options.offline
        : existingMarker.offline,
    npmRegistry:
      options.npmRegistrySpecified || existingMarker === null
        ? options.npmRegistry
        : existingMarker.npmRegistry,
    lensOnly:
      options.lensOnlySpecified || existingMarker === null
        ? options.lensOnly
        : !existingMarker.engine.enabled,
  };
  const planned = await resolveBootstrapPlan(effectiveOptions, ports);
  if (!planned.ok) return resultFailure(options.command, planned.error.message, null, []);
  const admitted = await admitInstallRoot(planned.value, ports.fs);
  if (!admitted.ok)
    return resultFailure(options.command, admitted.error.message, planned.value.installerRoot, []);
  await writeHostFiles(planned.value, ports.fs);
  const snapshot = await prepareSourceRegistrySnapshot(planned.value, ports.fs);
  if (!snapshot.ok)
    return resultFailure(options.command, snapshot.error.message, planned.value.installerRoot, []);
  const commands = [
    vibeCommand(
      planned.value.vibeExecutable,
      "install",
      planned.value.installerRoot,
      planned.value.offline,
      planned.value.profile,
    ),
    vibeCommand(
      planned.value.vibeExecutable,
      "build",
      planned.value.installerRoot,
      planned.value.offline,
      planned.value.profile,
    ),
    vibeCommand(
      planned.value.vibeExecutable,
      "deploy",
      planned.value.installerRoot,
      planned.value.offline,
      planned.value.profile,
    ),
  ];
  for (const command of commands) {
    const executed = await ports.process.run(command.file, command.args, {
      cwd: planned.value.installerRoot,
      env: childEnvironment(options.env, planned.value.settingsDir),
    });
    if (executed.code !== 0)
      return resultFailure(
        options.command,
        processFailure(`vibe ${command.operation}`, executed),
        planned.value.installerRoot,
        commands,
      );
    if (command.operation === "build") {
      const built = await inspectBootstrap(
        { settingsDir: planned.value.settingsDir, env: options.env },
        ports,
      );
      if (!built.ok || built.value.preparedReady !== true)
        return resultFailure(
          options.command,
          built.ok
            ? "vibe build returned without a valid prepared Zap runtime generation"
            : built.error.message,
          planned.value.installerRoot,
          commands,
        );
    }
  }
  const inspected = await inspectBootstrap(options, ports);
  return inspected.ok && inspected.value.state === "ready"
    ? {
        ok: true,
        code: 0,
        operation: options.command,
        state: "ready",
        installerRoot: planned.value.installerRoot,
        message: "Zap source installation is ready",
        commands,
      }
    : resultFailure(
        options.command,
        inspected.ok
          ? "Vibe deploy completed but installed launchers or prepared evidence are incomplete"
          : inspected.error.message,
        planned.value.installerRoot,
        commands,
      );
}

export function helpText() {
  return `Zap Vibe source installer

Usage: node tooling/install-source.mjs [install|update|status|uninstall] [options]

Options:
  --registry PATH      Local Vibe source registry
  --npm-registry URL   Explicit credential-free HTTP(S) npm registry for the build
  --settings-dir PATH  Vibe settings root (default VIBE_SETTINGS or ~/.vibe)
  --vibe PATH          Native Vibe executable or standard VVM vibe shim
  --offline            Require offline Vibe and npm dependency resolution
  --lens-only          Omit the Rust Zap engine and zap launcher
  -h, --help           Show this help without writing files
`;
}

function markerFor(input) {
  return {
    protocol: SOURCE_INSTALL_PROTOCOL,
    installationId: SOURCE_INSTALL_ID,
    settingsDir: input.settingsDir,
    registryDir: input.registryDir,
    npmRegistry: input.npmRegistry,
    offline: input.offline,
    lens: {
      group: LENS_COORDINATE.group,
      name: LENS_COORDINATE.name,
      version: LENS_COORDINATE.version,
    },
    engine: { enabled: !input.lensOnly, binary: "zap", vibeExecutable: input.vibeExecutable },
  };
}

async function admitInstallRoot(plan, fs) {
  const entries = await directoryEntries(plan.installerRoot, fs);
  if (entries !== null && entries.length > 0) {
    const marker = await readJson(plan.markerPath, fs);
    if (marker !== null) return validateMarker(marker, plan.settingsDir);
    if (await binaryGenerationCacheOnly(plan.installerRoot, entries, fs))
      return { ok: true, value: null };
    return failure(`refusing unrelated nonempty installation directory (${entries.join(", ")})`);
  }
  return { ok: true, value: null };
}

async function writeHostFiles(plan, fs) {
  await fs.mkdir(plan.installerRoot, { recursive: true });
  for (const file of renderHostFiles(plan)) {
    const target = join(plan.installerRoot, file.relative);
    await fs.mkdir(dirname(target), { recursive: true });
    const temporary = join(dirname(target), `.${basename(target)}.${plan.temporarySuffix}.new`);
    await fs.rm(temporary, { force: true });
    await fs.writeFile(temporary, file.content, { encoding: "utf8", flag: "wx" });
    await fs.rm(target, { force: true });
    await fs.rename(temporary, target);
  }
}

function vibeCommand(file, operation, root, offline, profile) {
  const args = [
    "--unattended",
    "--invoked-by",
    "zap-source-installer",
    "--agent-mode",
    "agent",
    ...(offline ? ["--offline"] : []),
    operation,
    "--path",
    root,
  ];
  if (operation !== "undeploy") args.push("--prefer-local", "--assume-yes");
  if (operation === "deploy" || operation === "undeploy") args.push("--profile", profile);
  return { file, args, operation };
}

async function resolveVibeExecutable(input, env, ports) {
  const candidate = await findExecutable(input ?? "vibe", env, ports);
  if (candidate === null) return failure("native Vibe executable was not found; pass --vibe PATH");
  if (ports.platform !== "win32" || !candidate.toLowerCase().endsWith(".cmd"))
    return { ok: true, value: candidate };
  const home =
    nonempty(env.VIBEVM_SHELL_HOME) ??
    (await readPointerHome(candidate, ports.fs)) ??
    nonempty(env.VIBEVM_HOME);
  if (home === null)
    return failure("standard VVM vibe.cmd has no active native executable pointer");
  for (const path of [join(home, "bin", "vibe.exe"), join(home, "vibe.exe")])
    if (await exists(path, ports.fs)) return { ok: true, value: path };
  return failure("active VVM instance does not contain native vibe.exe");
}

async function findExecutable(input, env, ports) {
  const hasSeparator = input.includes("/") || input.includes("\\");
  if (isAbsolute(input) || hasSeparator) {
    const path = resolve(input);
    return (await exists(path, ports.fs)) ? path : null;
  }
  const directories = String(env.PATH ?? env.Path ?? "")
    .split(ports.platform === "win32" ? ";" : ":")
    .filter(Boolean);
  const extensions =
    ports.platform === "win32" ? String(env.PATHEXT ?? ".EXE;.CMD;.BAT").split(";") : [""];
  for (const directory of directories)
    for (const extension of extensions) {
      const path = join(directory, `${input}${extension.toLowerCase()}`);
      if (await exists(path, ports.fs)) return path;
    }
  return null;
}

async function readPointerHome(shim, fs) {
  try {
    return nonempty(
      (await fs.readFile(resolve(dirname(shim), "..", "vibevm", "current"), "utf8")).trim(),
    );
  } catch {
    return null;
  }
}
async function discoverRegistry(start, fs) {
  let current = resolve(start);
  for (;;) {
    if (basename(current).toLowerCase() === "vibepacks") return current;
    const candidate = join(current, "vibevm", "vibepacks");
    if (await exists(candidate, fs)) return candidate;
    const parent = dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}
async function existingDirectory(path, label, fs) {
  try {
    const canonical = await fs.realpath(resolve(path));
    return (await fs.stat(canonical)).isDirectory()
      ? { ok: true, value: canonical }
      : failure(`${label} is not a directory`);
  } catch {
    return failure(`${label} does not exist or is unreadable`);
  }
}
function childEnvironment(env, settingsDir) {
  return { ...env, VIBE_SETTINGS: settingsDir };
}
function profileFor(platform) {
  return platform === "win32" ? "windows" : "posix";
}
function processFailure(label, result) {
  const detail = String(result.stderr || result.stdout || "no diagnostic")
    .trim()
    .slice(-2_000);
  return `${label} failed${result.code === null ? " to start" : ` with exit ${String(result.code)}`}: ${detail}`;
}
function statusResult(result) {
  return result.ok
    ? {
        ok: true,
        code: 0,
        operation: "status",
        state: result.value.state,
        installerRoot: result.value.installerRoot,
        message: `Zap source installation is ${result.value.state}`,
        commands: [],
        detail: result.value,
      }
    : resultFailure("status", result.error.message, null, []);
}
function resultFailure(operation, message, installerRoot, commands) {
  return { ok: false, code: 1, operation, state: "failed", installerRoot, message, commands };
}
function failure(message) {
  return { ok: false, error: { message } };
}
function nonempty(value) {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}
function validateNpmRegistry(value) {
  if (value === null || value === undefined) return { ok: true, value: null };
  try {
    const parsed = new URL(value);
    if (
      !["http:", "https:"].includes(parsed.protocol) ||
      parsed.username !== "" ||
      parsed.password !== "" ||
      parsed.search !== "" ||
      parsed.hash !== ""
    )
      return failure("--npm-registry must be a credential-free HTTP(S) URL");
    return { ok: true, value: parsed.href };
  } catch {
    return failure("--npm-registry must be a valid credential-free HTTP(S) URL");
  }
}
function safeMessage(error) {
  return error instanceof Error ? error.message.slice(0, 2_000) : "unknown error";
}
