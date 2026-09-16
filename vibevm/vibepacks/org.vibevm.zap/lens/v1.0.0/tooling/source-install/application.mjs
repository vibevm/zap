#!/usr/bin/env node
/** Generic Vibe user-application adapter. @scope spec://org.vibevm.zap/lens/PROP-017#adapter */
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { executeBootstrap, inspectBootstrap, parseBootstrapArgs } from "./bootstrap.mjs";
import { createNodeBootstrapPorts } from "./bootstrap-system.mjs";
import { LENS_COMMANDS } from "./bootstrap-manifest.mjs";

export const APPLICATION_CONTEXT_PROTOCOL = "vibe-application-context/1";
export const APPLICATION_RESULT_PROTOCOL = "vibe-application-result/1";
export const APPLICATION_ID = "zap";
export const APPLICATION_COMMANDS = Object.freeze([...LENS_COMMANDS, "zap"]);
const PUBLIC_NPM_REGISTRY = "https://registry.npmjs.org/";
const OPERATIONS = new Set(["install", "update", "uninstall"]);

export async function runApplicationAdapter(environment = process.env, injected = {}) {
  const contextPath = absoluteEnvironmentPath(
    environment.VIBE_APPLICATION_CONTEXT,
    "VIBE_APPLICATION_CONTEXT",
  );
  const replyPath = absoluteEnvironmentPath(
    environment.VIBE_APPLICATION_REPLY,
    "VIBE_APPLICATION_REPLY",
  );
  const context = parseApplicationContext(JSON.parse(await readFile(contextPath, "utf8")));
  try {
    const system = injected.bootstrapPorts ?? createNodeBootstrapPorts();
    const bootstrap = injected.executeBootstrap ?? executeBootstrap;
    const inspect = injected.inspectBootstrap ?? inspectBootstrap;
    const parsed = parseBootstrapArgs(bootstrapArguments(context), {
      ...environment,
      VIBE_SETTINGS: context.settingsRoot,
    });
    if (!parsed.ok) return writeFailed(replyPath, context, parsed.error.message);
    const outcome = await bootstrap(parsed.value, system);
    if (
      !outcome.ok ||
      outcome.operation !== context.operation ||
      outcome.installerRoot !== context.hostRoot
    )
      return writeFailed(
        replyPath,
        context,
        outcome.message ?? "Zap source bootstrap returned a mismatched result",
      );
    if (context.operation === "uninstall") {
      if (outcome.state !== "undeployed")
        return writeFailed(replyPath, context, "Zap source uninstall did not reach undeployed");
      return writeApplicationReply(replyPath, result(context, "undeployed", null, outcome.message));
    }
    if (outcome.state !== "ready")
      return writeFailed(replyPath, context, "Zap source bootstrap did not reach ready");
    const observed = await inspect(
      { settingsDir: context.settingsRoot, env: { VIBE_SETTINGS: context.settingsRoot } },
      system,
    );
    if (!observed.ok || observed.value.state !== "ready" || observed.value.generation === null)
      return writeFailed(replyPath, context, "Zap immutable runtime is not ready after bootstrap");
    const management = await managementEntry(
      context.hostRoot,
      observed.value.generation.runtimeRoot,
      system.fs,
    );
    const launchers = await applicationLaunchers(
      context.settingsRoot,
      observed.value.launcherPaths,
      system.fs,
    );
    return writeApplicationReply(
      replyPath,
      result(context, "ready", { runtime: "node", entry: management }, outcome.message, launchers),
    );
  } catch (error) {
    return writeFailed(
      replyPath,
      context,
      error instanceof Error ? error.message : "Zap application adapter failed",
    );
  }
}

export function parseApplicationContext(value) {
  const object = strictObject(value, "application context", [
    "protocol",
    "operation",
    "application",
    "settingsRoot",
    "hostRoot",
    "registryRoot",
    "vibeExecutable",
    "offline",
  ]);
  if (object.protocol !== APPLICATION_CONTEXT_PROTOCOL) failure("context protocol is unsupported");
  if (!OPERATIONS.has(object.operation)) failure("application operation is unsupported");
  const application = strictObject(object.application, "application identity", [
    "id",
    "package",
    "installerPackage",
    "commands",
  ]);
  if (application.id !== APPLICATION_ID) failure("application id differs from Zap");
  const packageIdentity = parsePackage(application.package, "application.package");
  const installerPackage = parsePackage(
    application.installerPackage,
    "application.installerPackage",
  );
  if (
    packageIdentity.group !== "org.vibevm.zap" ||
    packageIdentity.name !== "zap" ||
    packageIdentity.version !== "1.0.0"
  )
    failure("application package differs from org.vibevm.zap/zap@1.0.0");
  if (
    installerPackage.group !== "org.vibevm.zap" ||
    installerPackage.name !== "lens" ||
    installerPackage.version !== "1.0.0"
  )
    failure("installer package differs from org.vibevm.zap/lens@1.0.0");
  if (
    !Array.isArray(application.commands) ||
    application.commands.length !== APPLICATION_COMMANDS.length ||
    application.commands.some((command, index) => command !== APPLICATION_COMMANDS[index])
  )
    failure("application commands differ from the public Zap command set");
  const settingsRoot = absolutePath(object.settingsRoot, "settingsRoot");
  const hostRoot = absolutePath(object.hostRoot, "hostRoot");
  if (hostRoot !== resolve(settingsRoot, "opt", "apps", APPLICATION_ID))
    failure("application host is not bound to the selected settings root");
  const registryRoot =
    object.registryRoot === null ? null : absolutePath(object.registryRoot, "registryRoot");
  if (object.operation !== "uninstall" && registryRoot === null)
    failure("install and update require the selected local registry root");
  if (object.operation === "uninstall" && registryRoot !== null)
    failure("uninstall must use the retained installer without a registry");
  if (typeof object.offline !== "boolean") failure("offline must be boolean");
  return {
    protocol: APPLICATION_CONTEXT_PROTOCOL,
    operation: object.operation,
    application: {
      id: APPLICATION_ID,
      package: packageIdentity,
      installerPackage,
      commands: [...APPLICATION_COMMANDS],
    },
    settingsRoot,
    hostRoot,
    registryRoot,
    vibeExecutable: absolutePath(object.vibeExecutable, "vibeExecutable"),
    offline: object.offline,
  };
}

function bootstrapArguments(context) {
  const arguments_ = [
    context.operation,
    "--settings-dir",
    context.settingsRoot,
    "--vibe",
    context.vibeExecutable,
  ];
  if (context.registryRoot !== null) arguments_.push("--registry", context.registryRoot);
  if (context.operation !== "uninstall") arguments_.push("--npm-registry", PUBLIC_NPM_REGISTRY);
  if (context.offline) arguments_.push("--offline");
  return arguments_;
}

async function managementEntry(hostRoot, runtimeRoot, fs) {
  if (
    typeof runtimeRoot !== "string" ||
    !/^target\/zap-source-install\/generations\/[a-f0-9]{64}\/runtime$/u.test(runtimeRoot)
  )
    failure("generation runtime root is invalid");
  const runtime = resolve(hostRoot, ...runtimeRoot.split("/"));
  const entry = join(runtime, "tooling", "source-install", "application.mjs");
  if (!inside(hostRoot, runtime) || !inside(runtime, entry))
    failure("management entry escaped the owned immutable runtime");
  const hostState = await fs.lstat(hostRoot);
  const runtimeState = await fs.lstat(runtime);
  if (
    hostState.isSymbolicLink() ||
    !hostState.isDirectory() ||
    runtimeState.isSymbolicLink() ||
    !runtimeState.isDirectory()
  )
    failure("application host or immutable runtime is linked or unavailable");
  const state = await fs.lstat(entry);
  if (state.isSymbolicLink() || !state.isFile())
    failure("management entry is linked or is not a regular file");
  const canonicalHost = await fs.realpath(hostRoot);
  const canonicalRuntime = await fs.realpath(runtime);
  const canonicalEntry = await fs.realpath(entry);
  if (!inside(canonicalHost, canonicalRuntime) || !inside(canonicalRuntime, canonicalEntry))
    failure("management entry resolved outside the immutable runtime");
  return canonicalEntry;
}

function result(context, status, management, message, launchers = []) {
  return {
    protocol: APPLICATION_RESULT_PROTOCOL,
    operation: context.operation,
    applicationId: APPLICATION_ID,
    status,
    hostRoot: context.hostRoot,
    management,
    commands: [...APPLICATION_COMMANDS],
    launchers,
    message: boundedMessage(message),
  };
}

async function applicationLaunchers(settingsRoot, observedPaths, fs) {
  if (!Array.isArray(observedPaths)) failure("source bootstrap launcher paths are unavailable");
  const binRoot = resolve(settingsRoot, "opt", "bin");
  const expected = new Set(
    APPLICATION_COMMANDS.flatMap((command) => [`${command}.cmd`, `${command}.ps1`]),
  );
  const rows = [];
  for (const destination of observedPaths) {
    if (typeof destination !== "string" || !isAbsolute(destination))
      failure("source bootstrap launcher destination is invalid");
    const absolute = resolve(destination);
    const name = basename(absolute);
    if (!insideOrEqual(binRoot, dirname(absolute)) || !expected.has(name)) continue;
    const metadata = await fs.lstat(absolute);
    if (metadata.isSymbolicLink() || !metadata.isFile())
      failure("source bootstrap launcher is linked or unavailable");
    rows.push({
      destination: absolute,
      sha256: createHash("sha256")
        .update(await fs.readFile(absolute))
        .digest("hex"),
    });
  }
  rows.sort((left, right) => left.destination.localeCompare(right.destination, "en"));
  if (rows.length !== expected.size)
    failure("source bootstrap did not deploy every public launcher");
  return rows;
}

async function writeFailed(path, context, message) {
  await writeApplicationReply(path, result(context, "failed", null, message));
  return { ok: false, replyPath: path };
}

async function writeApplicationReply(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify(value), { encoding: "utf8", flag: "w" });
  return { ok: value.status !== "failed", replyPath: path, value };
}

function parsePackage(value, name) {
  const object = strictObject(value, name, ["group", "name", "version"]);
  return {
    group: boundedString(object.group, `${name}.group`, 160),
    name: boundedString(object.name, `${name}.name`, 160),
    version: boundedString(object.version, `${name}.version`, 80),
  };
}

function strictObject(value, name, fields) {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    failure(`${name} must be an object`);
  if (Object.keys(value).sort().join(",") !== [...fields].sort().join(","))
    failure(`${name} has missing or unknown fields`);
  return value;
}

function absoluteEnvironmentPath(value, name) {
  if (typeof value !== "string" || value.length === 0) failure(`${name} is missing`);
  return absolutePath(value, name);
}

function absolutePath(value, name) {
  const text = boundedString(value, name, 32_768);
  if (!isAbsolute(text)) failure(`${name} must be absolute`);
  return resolve(text);
}

function boundedString(value, name, maximum) {
  if (typeof value !== "string" || value.length === 0 || value.length > maximum)
    failure(`${name} is invalid`);
  if ([...value].some((character) => character.charCodeAt(0) < 32)) failure(`${name} is invalid`);
  return value;
}

function boundedMessage(value) {
  const message =
    typeof value === "string" && value.trim() !== "" ? value : "Zap operation completed";
  return message
    .replace(/[\r\n\t]+/gu, " ")
    .replace(/[\u0000-\u001f]/gu, "")
    .slice(0, 4_000);
}

function inside(parent, child) {
  const path = relative(resolve(parent), resolve(child));
  return path !== "" && path !== ".." && !path.startsWith(`..${sep}`) && !isAbsolute(path);
}

function insideOrEqual(parent, child) {
  return resolve(parent) === resolve(child) || inside(parent, child);
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#adapter: ${message}; fix surface: repair the generic Vibe application context or retained Zap runtime`,
  );
}

async function main() {
  let context;
  let replyPath;
  try {
    replyPath = absoluteEnvironmentPath(
      process.env.VIBE_APPLICATION_REPLY,
      "VIBE_APPLICATION_REPLY",
    );
    const contextPath = absoluteEnvironmentPath(
      process.env.VIBE_APPLICATION_CONTEXT,
      "VIBE_APPLICATION_CONTEXT",
    );
    context = parseApplicationContext(JSON.parse(await readFile(contextPath, "utf8")));
    const outcome = await runApplicationAdapter(process.env);
    process.exitCode = outcome.ok ? 0 : 1;
  } catch (error) {
    const message = boundedMessage(error instanceof Error ? error.message : "Zap adapter failed");
    if (context !== undefined && replyPath !== undefined) {
      try {
        await writeApplicationReply(replyPath, result(context, "failed", null, message));
      } catch {
        // The process failure remains authoritative when the owned reply cannot be written.
      }
    }
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  }
}

if (
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
)
  await main();
