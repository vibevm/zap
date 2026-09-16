/** Source-install ownership and integrity inspection. @scope spec://org.vibevm.zap/lens/PROP-016#commands */
import { createHash } from "node:crypto";
import { basename, isAbsolute, join, resolve, sep } from "node:path";
import { LENS_COMMANDS, LENS_COORDINATE } from "./bootstrap-manifest.mjs";
import {
  HOST_MARKER_RELATIVE,
  SOURCE_INSTALL_ID,
  SOURCE_INSTALL_PROTOCOL,
  parseMarker,
} from "./contract.mjs";

export const PREPARED_RELATIVE = "target/zap-source-install/prepared.json";

export async function inspectBootstrap(input, ports) {
  const settingsDir = resolve(
    input.settingsDir ?? nonempty(input.env?.VIBE_SETTINGS) ?? join(ports.homeDir, ".vibe"),
  );
  const installerRoot = join(settingsDir, "opt", "apps", "zap");
  const marker = await readJson(join(installerRoot, HOST_MARKER_RELATIVE), ports.fs);
  if (marker === null) {
    const entries = await directoryEntries(installerRoot, ports.fs);
    if (
      entries === null ||
      entries.length === 0 ||
      (await binaryGenerationCacheOnly(installerRoot, entries, ports.fs))
    )
      return {
        ok: true,
        value: {
          state: "absent",
          settingsDir,
          installerRoot,
          marker: null,
          retainedBinaryCache: entries !== null && entries.length > 0,
        },
      };
    return failure("installation directory is nonempty but has no Zap ownership marker");
  }
  const valid = validateMarker(marker, settingsDir);
  if (!valid.ok) return valid;
  const preparedRaw = await readJson(join(installerRoot, PREPARED_RELATIVE), ports.fs);
  const prepared = preparedRaw === null ? null : validatePrepared(preparedRaw, installerRoot);
  if (prepared !== null && !prepared.ok) return prepared;
  const preparedValue = prepared?.value ?? null;
  const generationRaw =
    preparedValue === null
      ? null
      : await readJson(
          join(installerRoot, ...preparedValue.generationRoot.split("/"), "generation.json"),
          ports.fs,
        );
  const generation =
    generationRaw === null || preparedValue === null
      ? null
      : validateGeneration(generationRaw, preparedValue, installerRoot);
  if (generation !== null && !generation.ok) return generation;
  const generationValue = generation?.value ?? null;
  const commands = valid.value.engine.enabled ? [...LENS_COMMANDS, "zap"] : [...LENS_COMMANDS];
  const suffixes = ports.platform === "win32" ? [".cmd", ".ps1"] : [""];
  const launcherPaths = commands.flatMap((command) =>
    suffixes.map((suffix) => join(settingsDir, "opt", "bin", `${command}${suffix}`)),
  );
  const launchers = await Promise.all(launcherPaths.map((path) => exists(path, ports.fs)));
  const preparedReady =
    preparedValue !== null && generationValue !== null
      ? await verifyLauncherIntegrity(
          preparedValue,
          generationValue,
          installerRoot,
          settingsDir,
          ports.platform,
          ports.fs,
          false,
        )
      : false;
  const integrity =
    preparedReady &&
    (await verifyLauncherIntegrity(
      preparedValue,
      generationValue,
      installerRoot,
      settingsDir,
      ports.platform,
      ports.fs,
      true,
    ));
  const generationExists =
    preparedValue !== null &&
    (await exists(join(installerRoot, ...preparedValue.generationRoot.split("/")), ports.fs));
  const installed =
    preparedValue !== null &&
    generationValue !== null &&
    generationExists &&
    launchers.every(Boolean) &&
    integrity;
  const state = installed
    ? "ready"
    : launchers.some(Boolean)
      ? "incomplete"
      : preparedValue === null
        ? "prepared"
        : "undeployed";
  return {
    ok: true,
    value: {
      state,
      settingsDir,
      installerRoot,
      marker: valid.value,
      prepared: preparedValue,
      generation: generationValue,
      preparedReady,
      launcherPaths,
    },
  };
}

export function validateMarker(value, settingsDir) {
  try {
    const marker = parseMarker(value);
    return resolve(marker.settingsDir) === settingsDir
      ? { ok: true, value: marker }
      : failure("installation marker settings root differs from selected settings root");
  } catch (error) {
    return failure(`Zap source installation marker is invalid: ${safeMessage(error)}`);
  }
}

function validatePrepared(value, installerRoot) {
  if (!plainObject(value)) return failure("prepared source-install record is invalid");
  if (
    Object.keys(value).sort().join(",") !==
    "engineEnabled,engineVersion,generationId,generationRoot,launchers,lensVersion,protocol"
  )
    return failure("prepared source-install record has an unsupported shape");
  if (
    value.protocol !== "zap-source-install-prepared/1" ||
    !hexDigest(value.generationId) ||
    value.generationRoot !== `target/zap-source-install/generations/${value.generationId}` ||
    value.lensVersion !== LENS_COORDINATE.version ||
    typeof value.engineEnabled !== "boolean" ||
    (value.engineVersion !== null && typeof value.engineVersion !== "string") ||
    !Array.isArray(value.launchers) ||
    !relativeInside(installerRoot, value.generationRoot)
  )
    return failure("prepared source-install record values are invalid");
  for (const launcher of value.launchers)
    if (
      !validLauncherRow(launcher) ||
      !launcher.path.startsWith("target/zap-source-install/launchers/") ||
      !relativeInside(installerRoot, launcher.path)
    )
      return failure("prepared source-install launcher row is invalid");
  return { ok: true, value };
}

function validateGeneration(value, prepared, installerRoot) {
  if (!plainObject(value)) return failure("source-install generation record is invalid");
  if (
    Object.keys(value).sort().join(",") !==
    "architecture,engineEnabled,engineVersion,generationId,generationRoot,launchers,lensVersion,nodeVersion,payloadDigest,platform,protocol,runtimeRoot,sourceDigest"
  )
    return failure("source-install generation record has an unsupported shape");
  if (
    value.protocol !== "zap-source-install-generation/1" ||
    value.generationId !== prepared.generationId ||
    value.generationRoot !== prepared.generationRoot ||
    value.runtimeRoot !== `${prepared.generationRoot}/runtime` ||
    !hexDigest(value.sourceDigest) ||
    !hexDigest(value.payloadDigest) ||
    value.lensVersion !== prepared.lensVersion ||
    value.engineEnabled !== prepared.engineEnabled ||
    value.engineVersion !== prepared.engineVersion ||
    typeof value.platform !== "string" ||
    typeof value.architecture !== "string" ||
    typeof value.nodeVersion !== "string" ||
    !Array.isArray(value.launchers) ||
    !relativeInside(installerRoot, value.runtimeRoot)
  )
    return failure("source-install generation record values are invalid");
  for (const launcher of value.launchers)
    if (
      !validLauncherRow(launcher) ||
      !launcher.path.startsWith(`${prepared.generationRoot}/launchers/`) ||
      !relativeInside(installerRoot, launcher.path)
    )
      return failure("source-install generation launcher row is invalid");
  return { ok: true, value };
}

async function verifyLauncherIntegrity(
  prepared,
  generation,
  root,
  settings,
  platform,
  fs,
  includeDeployed,
) {
  const accepted =
    platform === "win32" ? new Set(["windows-cmd", "windows-powershell"]) : new Set(["posix"]);
  const rows = prepared.launchers.filter((launcher) => accepted.has(launcher.platform));
  const expectedCommands = prepared.engineEnabled ? [...LENS_COMMANDS, "zap"] : [...LENS_COMMANDS];
  const expectedPlatforms =
    platform === "win32" ? ["windows-cmd", "windows-powershell"] : ["posix"];
  const expected = expectedCommands
    .flatMap((command) =>
      expectedPlatforms.map((launcherPlatform) => `${command}:${launcherPlatform}`),
    )
    .sort();
  const actual = rows.map((row) => `${row.command}:${row.platform}`).sort();
  if (actual.length !== expected.length || actual.some((value, index) => value !== expected[index]))
    return false;
  for (const row of rows) {
    const generated = generation.launchers.find(
      (candidate) => candidate.command === row.command && candidate.platform === row.platform,
    );
    if (generated === undefined) return false;
    const paths = [
      join(root, ...row.path.split("/")),
      join(root, ...generated.path.split("/")),
      ...(includeDeployed ? [join(settings, "opt", "bin", basename(row.path))] : []),
    ];
    const digests = await Promise.all(paths.map((path) => digestFile(path, fs)));
    if (digests.some((digest) => digest === null) || new Set(digests).size !== 1) return false;
  }
  return true;
}

function validLauncherRow(value) {
  return (
    plainObject(value) &&
    Object.keys(value).sort().join(",") === "command,path,platform" &&
    typeof value.command === "string" &&
    ["windows-cmd", "windows-powershell", "posix"].includes(value.platform) &&
    typeof value.path === "string" &&
    !value.path.includes("..")
  );
}
async function digestFile(path, fs) {
  try {
    return createHash("sha256")
      .update(await fs.readFile(path))
      .digest("hex");
  } catch {
    return null;
  }
}
function relativeInside(root, value) {
  if (isAbsolute(value)) return false;
  return resolve(root, ...value.split("/"))
    .slice(resolve(root).length)
    .startsWith(sep);
}
function hexDigest(value) {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
}
function plainObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function nonempty(value) {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}
function safeMessage(error) {
  return error instanceof Error ? error.message.slice(0, 2_000) : "unknown error";
}
function failure(message) {
  return { ok: false, error: { message } };
}

export async function directoryEntries(path, fs) {
  try {
    return await fs.readdir(path);
  } catch (error) {
    if (errorCode(error) === "ENOENT") return null;
    throw error;
  }
}
export async function binaryGenerationCacheOnly(installerRoot, entries, fs) {
  if (entries.length !== 1 || entries[0] !== "generations") return false;
  const root = join(installerRoot, "generations");
  const rootMetadata = await fs.lstat(root);
  if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) return false;
  const generations = await fs.readdir(root);
  if (generations.length === 0) return false;
  for (const generation of generations) {
    if (!/^[a-f0-9]{64}$/u.test(generation)) return false;
    const metadata = await fs.lstat(join(root, generation));
    if (metadata.isSymbolicLink() || !metadata.isDirectory()) return false;
  }
  return true;
}
export async function readJson(path, fs) {
  try {
    return JSON.parse(await fs.readFile(path, "utf8"));
  } catch {
    return null;
  }
}
export async function exists(path, fs) {
  try {
    await fs.access(path);
    return true;
  } catch {
    return false;
  }
}
function errorCode(error) {
  return typeof error === "object" && error !== null ? Reflect.get(error, "code") : null;
}

export { HOST_MARKER_RELATIVE, SOURCE_INSTALL_ID, SOURCE_INSTALL_PROTOCOL };
