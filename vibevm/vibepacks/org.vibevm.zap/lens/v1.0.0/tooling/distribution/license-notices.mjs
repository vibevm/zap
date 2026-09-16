/** Actual third-party notice staging. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import { createHash } from "node:crypto";
import { copyFile, lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";

export async function stageLicenseNotices(input) {
  const rust = await stageRustLicenseFiles(input.cargoMetadata, input.licensesRoot);
  const lock = JSON.parse(await readFile(join(input.lensBuild, "package-lock.json"), "utf8"));
  const npm = Object.entries(lock.packages ?? {})
    .filter(([path, record]) => path !== "" && record.dev !== true)
    .map(([path, record]) => ({
      path: path.replaceAll("\\", "/"),
      version: record.version ?? null,
      license: record.license ?? null,
    }))
    .sort((left, right) => left.path.localeCompare(right.path, "en"));
  const notices = {
    protocol: "zap-third-party-notices/1",
    node: { version: input.nodeVersion, licenseFile: "NODE-LICENSE.txt" },
    npm,
    rust,
  };
  await writeFile(
    join(input.licensesRoot, "THIRD-PARTY-NOTICES.json"),
    `${JSON.stringify(notices, null, 2)}\n`,
    "utf8",
  );
  return notices;
}

async function stageRustLicenseFiles(cargoMetadata, licensesRoot) {
  const destinationRoot = join(licensesRoot, "rust");
  await mkdir(destinationRoot, { recursive: true });
  const rows = [];
  const destinations = new Set();
  for (const record of cargoMetadata.packages) {
    if (
      typeof record.name !== "string" ||
      typeof record.version !== "string" ||
      typeof record.manifest_path !== "string" ||
      !isAbsolute(record.manifest_path)
    )
      failure("cargo metadata package identity or manifest path is invalid");
    const packageRoot = dirname(resolve(record.manifest_path));
    const candidates = new Set();
    if (typeof record.license_file === "string") {
      const declared = isAbsolute(record.license_file)
        ? resolve(record.license_file)
        : resolve(packageRoot, record.license_file);
      candidates.add(declared);
    }
    for (const entry of await readdir(packageRoot, { withFileTypes: true }))
      if (entry.isFile() && licenseFileName(entry.name))
        candidates.add(join(packageRoot, entry.name));
    if (candidates.size === 0) {
      const authors = join(packageRoot, "AUTHORS");
      try {
        const metadata = await lstat(authors);
        if (!metadata.isSymbolicLink() && metadata.isFile()) candidates.add(authors);
      } catch (error) {
        if (typeof error !== "object" || error === null || Reflect.get(error, "code") !== "ENOENT")
          throw error;
      }
    }
    if (candidates.size === 0)
      failure(`Rust package ${record.name}@${record.version} has no actual license notice file`);
    const packageId = safeLicenseDirectory(record.name, record.version, record.id);
    const packageDestination = join(destinationRoot, packageId);
    await mkdir(packageDestination, { recursive: true });
    const licenseFiles = [];
    for (const source of [...candidates].sort((left, right) => left.localeCompare(right, "en"))) {
      const fileName = basename(source);
      if (!licenseFileName(fileName) && fileName !== "AUTHORS")
        failure(`Rust package ${record.name}@${record.version} declares an unsafe license file`);
      const metadata = await lstat(source);
      if (metadata.isSymbolicLink() || !metadata.isFile() || metadata.size > 2_000_000)
        failure(`Rust package ${record.name}@${record.version} license notice is unsafe`);
      const relativePath = `rust/${packageId}/${fileName}`;
      const identity = relativePath.toLocaleLowerCase("en-US");
      if (destinations.has(identity)) continue;
      destinations.add(identity);
      await copyFile(source, join(packageDestination, fileName));
      licenseFiles.push(relativePath);
    }
    rows.push({
      name: record.name,
      version: record.version,
      license: record.license ?? null,
      source: record.source ?? null,
      licenseFiles,
    });
  }
  return rows.sort((left, right) =>
    `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`, "en"),
  );
}

function licenseFileName(value) {
  return /^(LICENSE|LICENCE|COPYING|NOTICE)(?:[._-].*)?$/iu.test(value);
}

function safeLicenseDirectory(name, version, id) {
  const base = `${name}-${version}`.replace(/[^A-Za-z0-9._-]/gu, "_");
  const suffix = createHash("sha256")
    .update(String(id ?? `${name}@${version}`))
    .digest("hex")
    .slice(0, 8);
  return `${base}-${suffix}`;
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#binary: ${message}; fix surface: include actual bounded Rust license notice files`,
  );
}
