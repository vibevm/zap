/** Protected installer marker and lifecycle input. @scope spec://org.vibevm.zap/lens/PROP-016#ownership */
import { readFile } from "node:fs/promises";
import { isAbsolute, relative, resolve } from "node:path";

export const SOURCE_INSTALL_PROTOCOL = "zap-source-install/1";
export const SOURCE_INSTALL_ID = "org.vibevm.zap.source-install";
export const HOST_MARKER_RELATIVE = ".vibe/zap-source-install.json";
export const BUILD_ROOT_RELATIVE = "target/zap-source-install";

export async function loadBuildInput(environment, lensRoot) {
  const projectRoot = requiredAbsolute(environment.VIBE_PROJECT_ROOT, "VIBE_PROJECT_ROOT");
  const contextPath = requiredAbsolute(environment.VIBE_CONTEXT, "VIBE_CONTEXT");
  const replyPath = requiredAbsolute(environment.VIBE_REPLY, "VIBE_REPLY");
  const markerPath = resolve(projectRoot, HOST_MARKER_RELATIVE);
  const [markerRaw, contextRaw] = await Promise.all([
    readFile(markerPath, "utf8"),
    readFile(contextPath, "utf8"),
  ]);
  const config = parseMarker(JSON.parse(markerRaw));
  const context = parseContext(JSON.parse(contextRaw));
  if (!inside(resolve(config.settingsDir, "opt", "apps"), projectRoot))
    fail("installer host is outside the selected settings application root");
  if (resolve(context.project.root) !== projectRoot)
    fail("lifecycle context names a different installer host");
  const lens = packageRow(context, config.lens.group, config.lens.name, config.lens.version);
  if (resolve(lens.slot) !== resolve(lensRoot))
    fail("the executing Lens helper differs from the selected lifecycle slot");
  const engine = config.engine.enabled
    ? packageRowByCoordinate(context, "org.vibevm.zap", "zap")
    : null;
  return { projectRoot, contextPath, replyPath, markerPath, config, context, lens, engine };
}

export function parseMarker(value) {
  const object = record(value, "host marker");
  exactKeys(object, [
    "protocol",
    "installationId",
    "settingsDir",
    "registryDir",
    "npmRegistry",
    "offline",
    "lens",
    "engine",
  ]);
  if (object.protocol !== SOURCE_INSTALL_PROTOCOL) fail("host marker protocol is unsupported");
  if (object.installationId !== SOURCE_INSTALL_ID) fail("host marker identity is invalid");
  const lens = packageIdentity(object.lens, "lens");
  const engineObject = record(object.engine, "engine");
  exactKeys(engineObject, ["enabled", "binary", "vibeExecutable"]);
  return {
    protocol: SOURCE_INSTALL_PROTOCOL,
    installationId: SOURCE_INSTALL_ID,
    settingsDir: absoluteString(object.settingsDir, "settingsDir"),
    registryDir: absoluteString(object.registryDir, "registryDir"),
    npmRegistry: npmRegistry(object.npmRegistry),
    offline: booleanValue(object.offline, "offline"),
    lens,
    engine: {
      enabled: booleanValue(engineObject.enabled, "engine.enabled"),
      binary: boundedString(engineObject.binary, "engine.binary", 160),
      vibeExecutable: absoluteString(engineObject.vibeExecutable, "engine.vibeExecutable"),
    },
  };
}

function npmRegistry(value) {
  if (value === null) return null;
  const text = boundedString(value, "npmRegistry", 2_048);
  let url;
  try {
    url = new URL(text);
  } catch {
    fail("npmRegistry must be an absolute HTTP(S) URL");
  }
  if (
    (url.protocol !== "http:" && url.protocol !== "https:") ||
    url.username !== "" ||
    url.password !== "" ||
    url.search !== "" ||
    url.hash !== ""
  )
    fail("npmRegistry must be credential-free HTTP(S) without a query or fragment");
  return url.href;
}

export function parseContext(value) {
  const object = record(value, "lifecycle context");
  if (object.envelope !== 1) fail("lifecycle context envelope is unsupported");
  const project = record(object.project, "lifecycle project");
  const world = record(object.world, "lifecycle world");
  if (!Array.isArray(world.packages)) fail("lifecycle world packages are missing");
  return {
    envelope: 1,
    project: { root: absoluteString(project.root, "context.project.root") },
    world: {
      packages: world.packages.map((item, index) => {
        const row = record(item, `context.world.packages[${String(index)}]`);
        return {
          group: boundedString(row.group, "package.group", 160),
          name: boundedString(row.name, "package.name", 160),
          version: boundedString(row.version, "package.version", 80),
          slot: absoluteString(row.slot, "package.slot"),
        };
      }),
    },
  };
}

function packageIdentity(value, label) {
  const object = record(value, label);
  exactKeys(object, ["group", "name", "version"]);
  return {
    group: boundedString(object.group, `${label}.group`, 160),
    name: boundedString(object.name, `${label}.name`, 160),
    version: boundedString(object.version, `${label}.version`, 80),
  };
}

function packageRowByCoordinate(context, group, name) {
  const matches = context.world.packages.filter((row) => row.group === group && row.name === name);
  if (matches.length !== 1) fail(`lifecycle world does not contain exactly one ${group}/${name}`);
  return matches[0];
}

function packageRow(context, group, name, version) {
  const matches = context.world.packages.filter(
    (row) => row.group === group && row.name === name && row.version === version,
  );
  if (matches.length !== 1) fail(`lifecycle world does not contain exactly one ${group}/${name}`);
  return matches[0];
}

function requiredAbsolute(value, name) {
  if (typeof value !== "string" || value.length === 0) fail(`${name} is missing`);
  return absoluteString(value, name);
}

function absoluteString(value, name) {
  const text = boundedString(value, name, 32_768);
  if (!isAbsolute(text)) fail(`${name} must be an absolute path`);
  return resolve(text);
}

function boundedString(value, name, maximum) {
  if (typeof value !== "string" || value.length === 0 || value.length > maximum)
    fail(`${name} is invalid`);
  if ([...value].some((character) => character.charCodeAt(0) < 32)) fail(`${name} is invalid`);
  return value;
}

function booleanValue(value, name) {
  if (typeof value !== "boolean") fail(`${name} must be boolean`);
  return value;
}

function record(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    fail(`${name} must be an object`);
  return value;
}

function exactKeys(object, keys) {
  const expected = new Set(keys);
  if (Object.keys(object).some((key) => !expected.has(key)) || keys.some((key) => !(key in object)))
    fail("host marker contains missing or unknown fields");
}

function inside(parent, child) {
  const path = relative(resolve(parent), resolve(child));
  return (
    path !== "" &&
    path !== ".." &&
    !path.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) &&
    !isAbsolute(path)
  );
}

function fail(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-016#root: ${message}; fix surface: repair the marked installer host and rerun its build phase`,
  );
}
