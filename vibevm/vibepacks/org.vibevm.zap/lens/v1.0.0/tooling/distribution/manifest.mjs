/** Strict Zap binary distribution manifest. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import { createHash } from "node:crypto";
import { lstat, readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

export const DISTRIBUTION_PROTOCOL = "vibe-application-distribution/1";
export const DISTRIBUTION_DESCRIPTOR = "vibe-application-distribution.json";
export const APPLICATION = Object.freeze({
  id: "zap",
  package: Object.freeze({ group: "org.vibevm.zap", name: "zap", version: "1.0.0" }),
  installerPackage: Object.freeze({
    group: "org.vibevm.zap",
    name: "lens",
    version: "1.0.0",
  }),
  commands: Object.freeze([
    "codlens",
    "codlens-mcp",
    "quicklens-service",
    "quicklens-web-auth",
    "quicklens-web",
    "zap-wayfinder",
    "zap-server",
    "zap-quicklens",
    "zap-quick-lens",
    "zap-mock-agent",
    "zap",
  ]),
});

export async function sealDistributionDirectory(root, input) {
  const directory = resolve(root);
  const descriptorPath = resolve(directory, DISTRIBUTION_DESCRIPTOR);
  if ((await kind(descriptorPath)) !== "absent") failure("distribution descriptor already exists");
  const files = await inventoryFiles(directory, new Set([DISTRIBUTION_DESCRIPTOR]));
  const descriptor = distributionDescriptor({ ...input, files });
  await writeFile(descriptorPath, `${JSON.stringify(descriptor, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  return descriptor;
}

export function distributionDescriptor(input) {
  const sourceCommit = digest(input.sourceCommit, "sourceCommit", /^[a-f0-9]{40,64}$/u);
  const sourceTree = digest(input.sourceTree, "sourceTree", /^sha256-tree\/1:[a-f0-9]{64}$/u);
  const launchers = launchersValue(input.launchers);
  const files = filesValue(input.files);
  for (const launcher of launchers)
    if (!files.some((file) => file.path === launcher.path))
      failure(`launcher is absent from files: ${launcher.path}`);
  return {
    protocol: DISTRIBUTION_PROTOCOL,
    application: {
      id: APPLICATION.id,
      package: { ...APPLICATION.package },
      installerPackage: { ...APPLICATION.installerPackage },
      commands: [...APPLICATION.commands],
    },
    os: "windows",
    arch: "x86_64",
    sourceCommit,
    sourceTree,
    management: { runtime: "builtin", entry: "management/launch.cmd" },
    launchers,
    files,
  };
}

export async function verifyDistributionDirectory(root) {
  const directory = resolve(root);
  const descriptor = parseDescriptor(
    JSON.parse(await readFile(resolve(directory, DISTRIBUTION_DESCRIPTOR), "utf8")),
  );
  const observed = await inventoryFiles(directory, new Set([DISTRIBUTION_DESCRIPTOR]));
  if (JSON.stringify(observed) !== JSON.stringify(descriptor.files))
    failure("distribution files differ from the exhaustive descriptor");
  return descriptor;
}

export function parseDescriptor(value) {
  exactObject(value, "descriptor", [
    "protocol",
    "application",
    "os",
    "arch",
    "sourceCommit",
    "sourceTree",
    "management",
    "launchers",
    "files",
  ]);
  if (value.protocol !== DISTRIBUTION_PROTOCOL) failure("descriptor protocol is unsupported");
  exactObject(value.application, "application", ["id", "package", "installerPackage", "commands"]);
  if (value.application.id !== APPLICATION.id) failure("application id differs from Zap");
  packageValue(value.application.package, APPLICATION.package, "application.package");
  packageValue(
    value.application.installerPackage,
    APPLICATION.installerPackage,
    "application.installerPackage",
  );
  if (JSON.stringify(value.application.commands) !== JSON.stringify(APPLICATION.commands))
    failure("application commands differ from Zap");
  if (value.os !== "windows" || value.arch !== "x86_64")
    failure("distribution target is unsupported");
  digest(value.sourceCommit, "sourceCommit", /^[a-f0-9]{40,64}$/u);
  digest(value.sourceTree, "sourceTree", /^sha256-tree\/1:[a-f0-9]{64}$/u);
  exactObject(value.management, "management", ["runtime", "entry"]);
  if (value.management.runtime !== "builtin" || value.management.entry !== "management/launch.cmd")
    failure("distribution management must use the built-in installer");
  const launchers = launchersValue(value.launchers);
  const files = filesValue(value.files);
  for (const launcher of launchers)
    if (!files.some((file) => file.path === launcher.path))
      failure(`launcher is absent from files: ${launcher.path}`);
  return {
    protocol: DISTRIBUTION_PROTOCOL,
    application: {
      id: APPLICATION.id,
      package: { ...APPLICATION.package },
      installerPackage: { ...APPLICATION.installerPackage },
      commands: [...APPLICATION.commands],
    },
    os: "windows",
    arch: "x86_64",
    sourceCommit: value.sourceCommit,
    sourceTree: value.sourceTree,
    management: { runtime: "builtin", entry: "management/launch.cmd" },
    launchers,
    files,
  };
}

export async function inventoryFiles(root, excluded = new Set()) {
  const directory = resolve(root);
  const rows = [];
  const identities = new Set();
  await walk(directory, "", rows, identities, excluded);
  return rows.sort((left, right) => left.path.localeCompare(right.path, "en"));
}

async function walk(root, prefix, rows, identities, excluded) {
  const current = prefix === "" ? root : resolve(root, ...prefix.split("/"));
  const entries = await readdir(current, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
  for (const entry of entries) {
    const path = prefix === "" ? entry.name : `${prefix}/${entry.name}`;
    if (excluded.has(path)) continue;
    portablePath(path);
    const identity = path.normalize("NFC").toLocaleLowerCase("en-US");
    if (identities.has(identity)) failure(`distribution path collides by case: ${path}`);
    identities.add(identity);
    const absolute = resolve(root, ...path.split("/"));
    const metadata = await lstat(absolute);
    if (metadata.isSymbolicLink()) failure(`distribution refuses link: ${path}`);
    if (metadata.isDirectory()) {
      await walk(root, path, rows, identities, excluded);
      continue;
    }
    if (!metadata.isFile()) failure(`distribution refuses special file: ${path}`);
    rows.push({ path, sha256: await sha256File(absolute), size: metadata.size });
  }
}

async function sha256File(path) {
  const digest = createHash("sha256");
  digest.update(await readFile(path));
  return digest.digest("hex");
}

function launchersValue(value) {
  if (!Array.isArray(value) || value.length !== APPLICATION.commands.length)
    failure("launchers must cover every public command exactly once");
  const seen = new Set();
  const destinations = new Set();
  const rows = value.map((launcher) => {
    exactObject(launcher, "launcher", ["command", "path", "destination"]);
    if (!APPLICATION.commands.includes(launcher.command) || seen.has(launcher.command))
      failure("launcher command is unknown or duplicated");
    seen.add(launcher.command);
    const destination = portablePath(launcher.destination);
    if (destination.includes("/") || !destination.startsWith(`${launcher.command}.`))
      failure("launcher destination must be one opt/bin filename for its command");
    const destinationIdentity = destination.toLocaleLowerCase("en-US");
    if (destinations.has(destinationIdentity)) failure("launcher destination is duplicated");
    destinations.add(destinationIdentity);
    return { command: launcher.command, path: portablePath(launcher.path), destination };
  });
  return rows.sort((left, right) => left.command.localeCompare(right.command, "en"));
}

function filesValue(value) {
  if (!Array.isArray(value) || value.length === 0) failure("files must be nonempty");
  const seen = new Set();
  const rows = value.map((file) => {
    exactObject(file, "file", ["path", "sha256", "size"]);
    const path = portablePath(file.path);
    const identity = path.normalize("NFC").toLocaleLowerCase("en-US");
    if (seen.has(identity)) failure(`file path is duplicated by case: ${path}`);
    seen.add(identity);
    if (path === DISTRIBUTION_DESCRIPTOR) failure("descriptor cannot hash itself");
    const sha256 = digest(file.sha256, "file.sha256", /^[a-f0-9]{64}$/u);
    if (!Number.isSafeInteger(file.size) || file.size < 0) failure("file size is invalid");
    return { path, sha256, size: file.size };
  });
  return rows.sort((left, right) => left.path.localeCompare(right.path, "en"));
}

function portablePath(value) {
  if (typeof value !== "string" || value === "" || value.includes("\\"))
    failure("distribution path is invalid");
  if (value.startsWith("/") || value.endsWith("/") || value.includes("//"))
    failure(`distribution path is not relative: ${value}`);
  for (const component of value.split("/")) {
    if (
      component === "." ||
      component === ".." ||
      component.endsWith(".") ||
      component.endsWith(" ")
    )
      failure(`distribution path is not portable: ${value}`);
    if (component.includes(":") || /[\u0000-\u001f<>"|?*]/u.test(component))
      failure(`distribution path is not portable: ${value}`);
    const stem = component.split(".")[0]?.toUpperCase();
    if (/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/u.test(stem))
      failure(`distribution path uses a reserved Windows name: ${value}`);
  }
  return value;
}

function packageValue(value, expected, name) {
  exactObject(value, name, ["group", "name", "version"]);
  if (
    value.group !== expected.group ||
    value.name !== expected.name ||
    value.version !== expected.version
  )
    failure(`${name} differs from the release identity`);
}

function digest(value, name, expression) {
  if (typeof value !== "string" || !expression.test(value)) failure(`${name} is invalid`);
  return value;
}

function exactObject(value, name, fields) {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    failure(`${name} must be an object`);
  if (Object.keys(value).sort().join(",") !== [...fields].sort().join(","))
    failure(`${name} has missing or unknown fields`);
}

async function kind(path) {
  try {
    const metadata = await lstat(path);
    if (metadata.isSymbolicLink()) return "link";
    if (metadata.isFile()) return "file";
    if (metadata.isDirectory()) return "directory";
    return "other";
  } catch (error) {
    if (typeof error === "object" && error !== null && Reflect.get(error, "code") === "ENOENT")
      return "absent";
    throw error;
  }
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#binary: ${message}; fix surface: rebuild the exact Windows x64 Zap distribution`,
  );
}
