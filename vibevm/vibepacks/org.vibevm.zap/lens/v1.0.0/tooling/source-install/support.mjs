/** Owned build filesystem and process boundary. @scope spec://org.vibevm.zap/lens/PROP-016#ownership */
import { createHash } from "node:crypto";
import {
  access,
  lstat,
  mkdir,
  readlink,
  readdir,
  readFile,
  realpath,
  rm,
  stat,
} from "node:fs/promises";
import { constants } from "node:fs";
import { spawn } from "node:child_process";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

const SOURCE_EXCLUSIONS = new Set([
  ".git",
  ".vibe",
  "coverage",
  "dist",
  "node_modules",
  "target",
  "vibedeps",
]);

export async function digestSourceTree(root) {
  const hash = createHash("sha256");
  await digestDirectory(resolve(root), "", hash);
  return hash.digest("hex");
}

export async function digestPayloadTree(root) {
  const canonical = await canonicalExistingDirectory(root);
  const hash = createHash("sha256");
  await digestPayloadDirectory(canonical, "", hash);
  return hash.digest("hex");
}

export async function fileDigest(path) {
  const hash = createHash("sha256");
  hash.update(await readFile(path));
  return hash.digest("hex");
}

export function digestJson(value) {
  return createHash("sha256").update(canonicalJson(value)).digest("hex");
}

export async function resolveNpmCli(nativeNode = process.execPath) {
  const nodeDir = dirname(resolve(nativeNode));
  const candidates = [
    process.env.npm_execpath,
    join(nodeDir, "node_modules", "npm", "bin", "npm-cli.js"),
    join(dirname(nodeDir), "lib", "node_modules", "npm", "bin", "npm-cli.js"),
    join(dirname(nodeDir), "node_modules", "npm", "bin", "npm-cli.js"),
  ].filter((candidate) => typeof candidate === "string" && candidate.length > 0);
  for (const candidate of candidates) {
    const absolute = resolve(candidate);
    if (await regularFile(absolute)) return absolute;
  }
  failure("native npm-cli.js could not be resolved beside the selected Node runtime");
}

export function createCommandRunner(options = {}) {
  const maximumBytes = options.maximumBytes ?? 1024 * 1024;
  return {
    run(command) {
      return new Promise((resolveResult, reject) => {
        const child = spawn(command.executable, command.args, {
          cwd: command.cwd,
          env: command.environment,
          shell: false,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
        let stdout = Buffer.alloc(0);
        let stderr = Buffer.alloc(0);
        child.stdout.on("data", (chunk) => {
          stdout = retainTail(stdout, chunk, maximumBytes);
        });
        child.stderr.on("data", (chunk) => {
          stderr = retainTail(stderr, chunk, maximumBytes);
        });
        child.once("error", reject);
        child.once("exit", (code, signal) => {
          resolveResult({
            code,
            signal,
            stdout: stdout.toString("utf8"),
            stderr: stderr.toString("utf8"),
          });
        });
      });
    },
  };
}

export async function runChecked(runner, command, label) {
  process.stderr.write(`source-install: ${label}...\n`);
  const result = await runner.run(command);
  if (result.code !== 0) {
    const diagnostic = boundedDiagnostic(result.stderr || result.stdout);
    failure(`${label} failed${diagnostic === "" ? "" : `: ${diagnostic}`}`);
  }
  process.stderr.write(`source-install: ${label} complete\n`);
  return result;
}

export async function regularFile(path) {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

export async function executableFile(path) {
  try {
    await access(path, constants.X_OK);
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

export async function canonicalExistingDirectory(path) {
  const canonical = await realpath(resolve(path));
  if (!(await stat(canonical)).isDirectory()) failure("expected directory is unavailable");
  return canonical;
}

export async function ensureOwnedDirectory(owner, target) {
  const canonicalOwner = await canonicalExistingDirectory(owner);
  if (!contained(canonicalOwner, target)) failure("owned directory escaped the installer host");
  const path = relative(canonicalOwner, resolve(target));
  let current = canonicalOwner;
  for (const component of path.split(separator())) {
    current = join(current, component);
    try {
      const state = await lstat(current);
      if (state.isSymbolicLink() || !state.isDirectory())
        failure("owned directory has a linked or non-directory ancestor");
    } catch (error) {
      if (errorCode(error) !== "ENOENT") throw error;
      await mkdir(current);
    }
  }
  const canonicalTarget = await realpath(current);
  if (!contained(canonicalOwner, canonicalTarget))
    failure("owned directory resolved outside the installer host");
  return canonicalTarget;
}

export async function removeOwnedTree(owner, target) {
  const canonicalOwner = await canonicalExistingDirectory(owner);
  if (!contained(canonicalOwner, target)) failure("cleanup target escaped its owner");
  let state;
  try {
    state = await lstat(target);
  } catch (error) {
    if (errorCode(error) === "ENOENT") return;
    throw error;
  }
  if (state.isSymbolicLink() || !state.isDirectory())
    failure("cleanup target is linked or is not an owned directory");
  const canonicalTarget = await realpath(target);
  if (!contained(canonicalOwner, canonicalTarget))
    failure("cleanup target resolved outside its owner");
  await rm(canonicalTarget, { recursive: true, force: true });
}

export async function ownedDirectory(owner, target) {
  const canonicalOwner = await canonicalExistingDirectory(owner);
  if (!contained(canonicalOwner, target)) failure("owned path escaped its owner");
  try {
    const state = await lstat(target);
    if (state.isSymbolicLink()) failure("owned directory is a symbolic link or reparse point");
    if (!state.isDirectory()) return false;
    const canonicalTarget = await realpath(target);
    if (!contained(canonicalOwner, canonicalTarget))
      failure("owned directory resolved outside its owner");
    return true;
  } catch (error) {
    if (errorCode(error) === "ENOENT") return false;
    throw error;
  }
}

export function contained(parent, child) {
  const from = resolve(parent);
  const to = resolve(child);
  const path = relative(from, to);
  return path !== "" && path !== ".." && !path.startsWith(`..${separator()}`) && !isAbsolute(path);
}

export function forward(path) {
  return path.replaceAll("\\", "/");
}

export function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
    .join(",")}}`;
}

async function digestDirectory(root, relativePath, hash) {
  const directory = relativePath === "" ? root : join(root, relativePath);
  const entries = (await readdir(directory, { withFileTypes: true })).sort((left, right) =>
    left.name.localeCompare(right.name),
  );
  for (const entry of entries) {
    if (SOURCE_EXCLUSIONS.has(entry.name)) continue;
    const childRelative = relativePath === "" ? entry.name : join(relativePath, entry.name);
    const child = join(root, childRelative);
    const state = await lstat(child);
    if (state.isSymbolicLink()) failure("source installation refuses symbolic build inputs");
    hash.update(forward(childRelative));
    hash.update("\0");
    if (state.isDirectory()) {
      hash.update("directory\0");
      await digestDirectory(root, childRelative, hash);
    } else if (state.isFile()) {
      hash.update("file\0");
      hash.update(await readFile(child));
      hash.update("\0");
    } else {
      failure("source installation encountered an unsupported filesystem entry");
    }
  }
}

async function digestPayloadDirectory(root, relativePath, hash) {
  const directory = relativePath === "" ? root : join(root, relativePath);
  const entries = (await readdir(directory, { withFileTypes: true })).sort((left, right) =>
    left.name.localeCompare(right.name),
  );
  for (const entry of entries) {
    const childRelative = relativePath === "" ? entry.name : join(relativePath, entry.name);
    const child = join(root, childRelative);
    const state = await lstat(child);
    hash.update(forward(childRelative));
    hash.update("\0");
    if (state.isSymbolicLink()) {
      const link = await readlink(child);
      const resolved = resolve(dirname(child), link);
      if (!contained(root, resolved)) failure("payload contains a link outside its generation");
      hash.update("link\0");
      hash.update(link);
      hash.update("\0");
    } else if (state.isDirectory()) {
      hash.update("directory\0");
      await digestPayloadDirectory(root, childRelative, hash);
    } else if (state.isFile()) {
      hash.update("file\0");
      hash.update(await readFile(child));
      hash.update("\0");
    } else {
      failure("payload contains an unsupported filesystem entry");
    }
  }
}

function retainTail(current, chunk, maximum) {
  const next = Buffer.concat([current, Buffer.from(chunk)]);
  return next.length <= maximum ? next : next.subarray(next.length - maximum);
}

function boundedDiagnostic(value) {
  return value
    .replace(/[\r\n\t]+/gu, " ")
    .replace(/[\u0000-\u001f]/gu, "")
    .trim()
    .slice(-2_000);
}

function separator() {
  return process.platform === "win32" ? "\\" : "/";
}

function errorCode(error) {
  return typeof error === "object" && error !== null ? Reflect.get(error, "code") : null;
}

export function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-016#root: ${message}; fix surface: repair the source-install build inputs and retry`,
  );
}
