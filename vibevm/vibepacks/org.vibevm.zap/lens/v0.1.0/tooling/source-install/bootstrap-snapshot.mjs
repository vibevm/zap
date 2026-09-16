/** Transactional Lens source snapshot. @scope spec://org.vibevm.zap/lens/PROP-016#ownership */
import { dirname, join, relative, resolve, sep } from "node:path";

const SOURCE_FILES = Object.freeze([
  "LICENSE.md",
  "package-lock.json",
  "package.json",
  "quicklens.preload.vite.config.ts",
  "quicklens.preview.config.ts",
  "quicklens.vite.config.ts",
  "README.md",
  "tsconfig.browser.json",
  "tsconfig.build.json",
  "tsconfig.electron.json",
  "tsconfig.json",
  "tsconfig.quicklens.test.json",
  "tsconfig.test.json",
  "tooling/clean-dist.js",
  "vibe.toml",
  "vitest.config.ts",
]);
const OPTIONAL_SOURCE_FILES = Object.freeze([
  ".prettierrc.json",
  ".vibeignore",
  "conform.toml",
  "eslint.config.js",
]);
const SOURCE_DIRECTORIES = Object.freeze(["integrations", "src", "tooling", "vibevm/vibespecs"]);
const OPTIONAL_SOURCE_DIRECTORIES = Object.freeze(["docs"]);

export async function prepareLensSourceSnapshot(plan, fs) {
  const sourceRoot = plan.sourceRoot;
  const destination = plan.localLensRegistryRoot;
  const parent = dirname(destination);
  const temporary = join(parent, `.v0.1.0.next-${plan.temporarySuffix}`);
  const backup = join(parent, `.v0.1.0.previous-${plan.temporarySuffix}`);
  requireContained(plan.installerRoot, destination);
  requireContained(plan.installerRoot, temporary);
  requireContained(plan.installerRoot, backup);
  await validateLensSource(sourceRoot, fs);
  await fs.mkdir(parent, { recursive: true });
  await rejectLinkedAncestors(plan.settingsDir, parent, fs);
  await rejectUnsafeDestination(temporary, fs);
  await rejectUnsafeDestination(backup, fs);
  await rejectUnsafeDestination(destination, fs);
  await fs.rm(temporary, { recursive: true, force: true });
  await fs.rm(backup, { recursive: true, force: true });
  await fs.mkdir(temporary, { recursive: true });
  try {
    for (const file of SOURCE_FILES)
      await copyRegular(join(sourceRoot, file), join(temporary, file), fs);
    for (const file of OPTIONAL_SOURCE_FILES)
      await copyOptionalRegular(join(sourceRoot, file), join(temporary, file), fs);
    for (const directory of SOURCE_DIRECTORIES)
      await copyTree(join(sourceRoot, directory), join(temporary, directory), fs);
    for (const directory of OPTIONAL_SOURCE_DIRECTORIES)
      await copyOptionalTree(join(sourceRoot, directory), join(temporary, directory), fs);
    const current = await pathKind(destination, fs);
    if (current === "other") throw new Error("Lens source snapshot destination is not a directory");
    if (current === "directory") await fs.rename(destination, backup);
    try {
      await fs.rename(temporary, destination);
    } catch (error) {
      if (current === "directory") await fs.rename(backup, destination);
      throw error;
    }
    await fs.rm(backup, { recursive: true, force: true });
    return { ok: true, value: destination };
  } catch (error) {
    await fs.rm(temporary, { recursive: true, force: true });
    return {
      ok: false,
      error: {
        message: `Lens source snapshot preparation failed: ${safeMessage(error)}`,
      },
    };
  }
}

async function copyOptionalTree(source, destination, fs) {
  try {
    await fs.lstat(source);
  } catch (error) {
    if (errorCode(error) === "ENOENT") return;
    throw error;
  }
  await copyTree(source, destination, fs);
}

async function copyTree(source, destination, fs) {
  const metadata = await safeLstat(source, fs);
  rejectUnsafeSource(source, metadata);
  if (!metadata.isDirectory()) throw new Error(`source closure path is not a directory: ${source}`);
  await fs.mkdir(destination, { recursive: true });
  const entries = await fs.readdir(source, { withFileTypes: true });
  for (const entry of entries) {
    if (excluded(entry.name)) continue;
    const from = join(source, entry.name);
    const to = join(destination, entry.name);
    if (entry.isDirectory()) await copyTree(from, to, fs);
    else if (entry.isFile()) await copyRegular(from, to, fs);
    else throw new Error(`source closure contains a link or special file: ${from}`);
  }
}

async function validateLensSource(sourceRoot, fs) {
  const manifest = await fs.readFile(join(sourceRoot, "vibe.toml"), "utf8");
  for (const field of [
    ["name", "lens"],
    ["group", "org.vibevm.zap"],
    ["version", "0.1.0"],
  ]) {
    const expression = new RegExp(
      `^${field[0]}\\s*=\\s*"${field[1].replaceAll(".", "\\.")}"\\s*$`,
      "m",
    );
    if (!expression.test(manifest))
      throw new Error(`selected registry Lens manifest has wrong ${field[0]}`);
  }
}

async function copyRegular(source, destination, fs) {
  const metadata = await safeLstat(source, fs);
  rejectUnsafeSource(source, metadata);
  if (!metadata.isFile()) throw new Error(`source closure path is not a regular file: ${source}`);
  await fs.mkdir(dirname(destination), { recursive: true });
  await fs.copyFile(source, destination);
}

async function copyOptionalRegular(source, destination, fs) {
  try {
    const metadata = await fs.lstat(source);
    rejectUnsafeSource(source, metadata);
    if (!metadata.isFile()) throw new Error(`source closure path is not a regular file: ${source}`);
    await fs.mkdir(dirname(destination), { recursive: true });
    await fs.copyFile(source, destination);
  } catch (error) {
    if (errorCode(error) !== "ENOENT") throw error;
  }
}

function rejectUnsafeSource(path, metadata) {
  if (metadata.isSymbolicLink()) throw new Error(`source closure refuses symbolic link: ${path}`);
  if (metadata.isFile() && metadata.nlink > 1)
    throw new Error(`source closure refuses hard-linked file: ${path}`);
}

async function safeLstat(path, fs) {
  try {
    return await fs.lstat(path);
  } catch {
    throw new Error(`required source closure path is missing: ${path}`);
  }
}

async function pathKind(path, fs) {
  try {
    const metadata = await fs.lstat(path);
    if (metadata.isSymbolicLink()) return "other";
    return metadata.isDirectory() ? "directory" : "other";
  } catch (error) {
    if (errorCode(error) === "ENOENT") return "absent";
    throw error;
  }
}

function requireContained(root, target) {
  if (!resolve(target).startsWith(resolve(root)))
    throw new Error(`source snapshot path escapes installer root: ${target}`);
  const path = relative(resolve(root), resolve(target));
  if (path === "" || path === ".." || path.startsWith(`..${sep}`))
    throw new Error(`source snapshot path escapes installer root: ${target}`);
}
async function rejectLinkedAncestors(root, target, fs) {
  const relativePath = relative(resolve(root), resolve(target));
  if (relativePath === ".." || relativePath.startsWith(`..${sep}`))
    throw new Error(`source snapshot ancestor escapes settings root: ${target}`);
  let current = resolve(root);
  const rootMetadata = await safeLstat(current, fs);
  if (rootMetadata.isSymbolicLink())
    throw new Error(`source snapshot refuses linked settings root: ${current}`);
  for (const segment of relativePath.split(sep).filter(Boolean)) {
    current = join(current, segment);
    const metadata = await safeLstat(current, fs);
    if (metadata.isSymbolicLink())
      throw new Error(`source snapshot refuses linked destination ancestor: ${current}`);
  }
}
async function rejectUnsafeDestination(path, fs) {
  try {
    const metadata = await fs.lstat(path);
    if (metadata.isSymbolicLink() || !metadata.isDirectory())
      throw new Error(`source snapshot refuses unsafe destination: ${path}`);
  } catch (error) {
    if (errorCode(error) !== "ENOENT") throw error;
  }
}
function excluded(name) {
  return new Set(["dist", "node_modules", "target", ".vibe", ".git"]).has(name);
}
function safeMessage(error) {
  return error instanceof Error ? error.message.slice(0, 2_000) : "unknown filesystem error";
}
function errorCode(error) {
  return typeof error === "object" && error !== null ? Reflect.get(error, "code") : null;
}
