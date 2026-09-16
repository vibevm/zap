/** Transactional Zap source registry snapshot. @scope spec://org.vibevm.zap/lens/PROP-016#ownership */
import { dirname, join, relative, resolve, sep } from "node:path";

const PACKAGE_VERSION = "1.0.0";
const EXCLUDED_NAMES = new Set([
  ".git",
  ".vibe",
  "dist",
  "node_modules",
  "target",
  "vibedeps",
  "vibepacks",
]);

export async function prepareSourceRegistrySnapshot(plan, fs) {
  const destination = plan.localRegistryRoot;
  const parent = dirname(destination);
  const temporary = join(parent, `.vibepacks.next-${plan.temporarySuffix}`);
  const backup = join(parent, `.vibepacks.previous-${plan.temporarySuffix}`);
  requireContained(plan.installerRoot, destination);
  requireContained(plan.installerRoot, temporary);
  requireContained(plan.installerRoot, backup);
  await validatePackageSource(plan.lensSourceRoot, "lens", fs);
  if (!plan.lensOnly) await validatePackageSource(plan.engineSourceRoot, "zap", fs);
  await fs.mkdir(parent, { recursive: true });
  await rejectLinkedAncestors(plan.settingsDir, parent, fs);
  await rejectUnsafeDestination(temporary, fs);
  await rejectUnsafeDestination(backup, fs);
  await rejectUnsafeDestination(destination, fs);
  await fs.rm(temporary, { recursive: true, force: true });
  await fs.rm(backup, { recursive: true, force: true });
  await fs.mkdir(temporary, { recursive: true });
  try {
    await copyTree(plan.lensSourceRoot, packageDestination(temporary, "lens"), fs);
    if (!plan.lensOnly)
      await copyTree(plan.engineSourceRoot, packageDestination(temporary, "zap"), fs);
    const current = await pathKind(destination, fs);
    if (current === "other") throw new Error("source registry destination is not a directory");
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
        message: `Zap source registry snapshot preparation failed: ${safeMessage(error)}`,
      },
    };
  }
}

function packageDestination(registryRoot, name) {
  return join(registryRoot, "org.vibevm.zap", name, `v${PACKAGE_VERSION}`);
}

async function copyTree(source, destination, fs) {
  const metadata = await safeLstat(source, fs);
  rejectUnsafeSource(source, metadata);
  if (!metadata.isDirectory()) throw new Error(`source closure path is not a directory: ${source}`);
  await fs.mkdir(destination, { recursive: true });
  const entries = await fs.readdir(source, { withFileTypes: true });
  for (const entry of entries) {
    if (EXCLUDED_NAMES.has(entry.name)) continue;
    const from = join(source, entry.name);
    const to = join(destination, entry.name);
    if (entry.isDirectory()) await copyTree(from, to, fs);
    else if (entry.isFile()) await copyRegular(from, to, fs);
    else throw new Error(`source closure contains a link or special file: ${from}`);
  }
}

async function validatePackageSource(sourceRoot, expectedName, fs) {
  const manifest = await fs.readFile(join(sourceRoot, "vibe.toml"), "utf8");
  for (const [field, value] of [
    ["name", expectedName],
    ["group", "org.vibevm.zap"],
    ["version", PACKAGE_VERSION],
  ]) {
    const expression = new RegExp(`^${field}\\s*=\\s*"${value.replaceAll(".", "\\.")}"\\s*$`, "m");
    if (!expression.test(manifest))
      throw new Error(`selected registry ${expectedName} manifest has wrong ${field}`);
  }
}

async function copyRegular(source, destination, fs) {
  const metadata = await safeLstat(source, fs);
  rejectUnsafeSource(source, metadata);
  if (!metadata.isFile()) throw new Error(`source closure path is not a regular file: ${source}`);
  await fs.mkdir(dirname(destination), { recursive: true });
  await fs.copyFile(source, destination);
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
  const path = relative(resolve(root), resolve(target));
  if (
    path === "" ||
    path === ".." ||
    path.startsWith(`..${sep}`) ||
    resolve(target).startsWith(resolve(root)) === false
  )
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

function safeMessage(error) {
  return error instanceof Error ? error.message.slice(0, 2_000) : "unknown filesystem error";
}

function errorCode(error) {
  return typeof error === "object" && error !== null ? Reflect.get(error, "code") : null;
}
