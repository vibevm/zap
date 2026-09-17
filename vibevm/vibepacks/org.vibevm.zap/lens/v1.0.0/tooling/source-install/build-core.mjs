/** Immutable source-install runtime generation. @scope spec://org.vibevm.zap/lens/PROP-016#payload */
import { randomUUID } from "node:crypto";
import { chmod, copyFile, cp, mkdir, readFile, readdir, rename, writeFile } from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { BUILD_ROOT_RELATIVE, SOURCE_INSTALL_ID, SOURCE_INSTALL_PROTOCOL } from "./contract.mjs";
import {
  PACKAGE_COMMANDS,
  launcherFileNames,
  renderLaunchers,
  validatePackageCommands,
} from "./launchers.mjs";
import { stageManagementTools } from "./management-tools.mjs";
import {
  canonicalExistingDirectory,
  contained,
  createCommandRunner,
  digestJson,
  digestPayloadTree,
  digestSourceTree,
  ensureOwnedDirectory,
  executableFile,
  failure,
  forward,
  ownedDirectory,
  regularFile,
  removeOwnedTree,
  resolveNpmCli,
  runChecked,
} from "./support.mjs";

export const PREPARED_PROTOCOL = "zap-source-install-prepared/1";
export const GENERATION_PROTOCOL = "zap-source-install-generation/1";
export async function buildSourceInstallation(input, ports = {}) {
  requireNode24();
  const hostRoot = await canonicalExistingDirectory(input.projectRoot);
  const lensRoot = await canonicalExistingDirectory(input.lensRoot);
  const engineRoot =
    input.engine === null ? null : await canonicalExistingDirectory(input.engine.slot);
  const nativeNode = resolve(ports.nativeNode ?? process.execPath);
  if (!(await executableFile(nativeNode))) failure("selected native Node runtime is unavailable");
  const npmCli = resolve(ports.npmCli ?? (await resolveNpmCli(nativeNode)));
  const packageDocument = JSON.parse(await readFile(join(lensRoot, "package.json"), "utf8"));
  validatePackageCommands(packageDocument);
  if (packageDocument.version !== input.config.lens.version)
    failure("Lens package version differs from the installer marker");
  const lensDigest = await digestSourceTree(lensRoot);
  const engineDigest = engineRoot === null ? null : await digestSourceTree(engineRoot);
  const sourceDigest = digestJson({
    protocol: SOURCE_INSTALL_PROTOCOL,
    installationId: SOURCE_INSTALL_ID,
    lensDigest,
    lensVersion: input.config.lens.version,
    engineDigest,
    engineVersion: input.engine?.version ?? null,
    engineEnabled: input.config.engine.enabled,
    npmRegistry: input.config.npmRegistry ?? null,
    nodePath: forward(nativeNode),
    nodeVersion: process.versions.node,
    platform: process.platform,
    architecture: process.arch,
  });
  const generationId = digestJson({ protocol: GENERATION_PROTOCOL, sourceDigest });
  const buildRoot = resolve(hostRoot, BUILD_ROOT_RELATIVE);
  const generationsRoot = join(buildRoot, "generations");
  const generationRoot = join(generationsRoot, generationId);
  await ensureOwnedDirectory(hostRoot, buildRoot);
  await ensureOwnedDirectory(buildRoot, generationsRoot);
  const runner = ports.runner ?? createCommandRunner();
  const generationPresent = await ownedDirectory(generationsRoot, generationRoot);
  const generation = await validGeneration(hostRoot, generationRoot, generationId, sourceDigest);
  if (generationPresent && generation === null)
    failure(
      `immutable generation ${generationId} failed payload validation and was retained for inspection`,
    );
  const record =
    generation ??
    (await createGeneration({
      input,
      hostRoot,
      lensRoot,
      engineRoot,
      nativeNode,
      npmCli,
      runner,
      buildRoot,
      generationsRoot,
      generationRoot,
      generationId,
      sourceDigest,
    }));
  const prepared = await publishPrepared(hostRoot, buildRoot, generationRoot, record);
  return { generation: record, prepared, reused: generation !== null };
}

async function createGeneration(state) {
  const stagingRoot = state.generationsRoot;
  const staging = join(stagingRoot, `.pending-${randomUUID()}`);
  if (!contained(stagingRoot, staging)) failure("generation staging path escaped its owner");
  await ensureOwnedDirectory(stagingRoot, staging);
  try {
    await buildLens(state);
    const runtimeRoot = join(staging, "runtime");
    await stageLensRuntime(
      state.lensRoot,
      runtimeRoot,
      state.nativeNode,
      state.npmCli,
      state.runner,
      state.input.config.offline,
      state.input.config.npmRegistry ?? null,
    );
    const enginePath =
      state.engineRoot === null ? null : await stageEngine(state, join(runtimeRoot, "bin"));
    const finalRuntime = join(state.generationRoot, "runtime");
    const launchers = renderLaunchers({
      runtimeRoot: finalRuntime,
      nativeNode: state.nativeNode,
      enginePath: enginePath === null ? null : join(finalRuntime, "bin", basename(enginePath)),
    });
    const finalLauncherRows = launchers.map((launcher) => ({
      command: launcher.command,
      platform: launcher.platform,
      path: projectRelative(
        state.hostRoot,
        join(state.generationRoot, "launchers", launcher.fileName),
      ),
    }));
    await writeLaunchers(join(staging, "launchers"), launchers);
    const payloadDigest = digestJson({
      runtime: await digestPayloadTree(join(staging, "runtime")),
      launchers: await digestPayloadTree(join(staging, "launchers")),
    });
    const record = generationRecord(state, finalLauncherRows, payloadDigest);
    await writeFile(join(staging, "generation.json"), `${JSON.stringify(record, null, 2)}\n`, {
      flag: "wx",
    });
    try {
      await rename(staging, state.generationRoot);
    } catch (error) {
      const raced = await validGeneration(
        state.hostRoot,
        state.generationRoot,
        state.generationId,
        state.sourceDigest,
      );
      if (raced === null) throw error;
      await removeOwnedTree(stagingRoot, staging);
      return raced;
    }
    return record;
  } catch (error) {
    await removeOwnedTree(stagingRoot, staging);
    throw error;
  }
}

async function buildLens(state) {
  const environment = npmEnvironment(
    state.input.config.offline,
    state.input.config.npmRegistry ?? null,
  );
  await runChecked(
    state.runner,
    command(
      state.nativeNode,
      [
        state.npmCli,
        "ci",
        "--no-audit",
        "--no-fund",
        ...offlineNpm(state.input.config.offline),
        ...npmRegistryArguments(state.input.config.npmRegistry ?? null),
      ],
      state.lensRoot,
      environment,
    ),
    "locked Lens dependency installation",
  );
  await ensureElectronRuntime(
    state.lensRoot,
    state.nativeNode,
    state.runner,
    state.input.config.offline,
    environment,
  );
  await runChecked(
    state.runner,
    command(
      state.nativeNode,
      [
        state.npmCli,
        "run",
        "build",
        ...npmRegistryArguments(state.input.config.npmRegistry ?? null),
      ],
      state.lensRoot,
      environment,
    ),
    "Lens production build",
  );
}

async function stageLensRuntime(
  lensRoot,
  runtimeRoot,
  nativeNode,
  npmCli,
  runner,
  offline,
  npmRegistry,
) {
  await mkdir(runtimeRoot, { recursive: true });
  for (const file of ["package.json", "package-lock.json", "LICENSE.md", "README.md", "vibe.toml"])
    await copyFile(join(lensRoot, file), join(runtimeRoot, file));
  await stageManagementTools(lensRoot, runtimeRoot);
  for (const directory of ["dist", "integrations", "vibevm/vibespecs"]) {
    const destination = join(runtimeRoot, directory);
    await mkdir(dirname(destination), { recursive: true });
    await cp(join(lensRoot, directory), destination, {
      recursive: true,
      force: false,
      errorOnExist: true,
    });
  }
  const optionalDocs = join(lensRoot, "docs");
  if (await ownedDirectory(lensRoot, optionalDocs))
    await cp(optionalDocs, join(runtimeRoot, "docs"), {
      recursive: true,
      force: false,
      errorOnExist: true,
    });
  for (const [, entry] of PACKAGE_COMMANDS) {
    if (!(await regularFile(join(runtimeRoot, entry))))
      failure(`built runtime entry ${entry} is missing`);
  }
  await runChecked(
    runner,
    command(
      nativeNode,
      [
        npmCli,
        "ci",
        "--omit=dev",
        "--no-audit",
        "--no-fund",
        ...offlineNpm(offline),
        ...npmRegistryArguments(npmRegistry),
      ],
      runtimeRoot,
      npmEnvironment(offline, npmRegistry),
    ),
    "locked production dependency staging",
  );
  await stageElectronPackage(lensRoot, runtimeRoot);
  await verifyRuntimePayload(runtimeRoot);
}

async function stageElectronPackage(lensRoot, runtimeRoot) {
  const source = join(lensRoot, "node_modules", "electron");
  const runtimeModules = join(runtimeRoot, "node_modules");
  const destination = join(runtimeModules, "electron");
  if (!(await ownedDirectory(runtimeModules, destination))) {
    await cp(source, destination, { recursive: true, force: false, errorOnExist: true });
    return;
  }
  if (await executableFile(electronExecutable(destination))) return;
  const destinationDist = join(destination, "dist");
  if (await ownedDirectory(destination, destinationDist))
    await removeOwnedTree(destination, destinationDist);
  await cp(join(source, "dist"), destinationDist, {
    recursive: true,
    force: false,
    errorOnExist: true,
  });
  await copyFile(join(source, "path.txt"), join(destination, "path.txt"));
}

async function ensureElectronRuntime(lensRoot, nativeNode, runner, offline, environment) {
  const electronRoot = join(lensRoot, "node_modules", "electron");
  const executable = electronExecutable(electronRoot);
  if (await executableFile(executable)) return;
  if (offline)
    failure("offline source build has no cached Electron runtime after locked npm install");
  const installer = join(electronRoot, "install.js");
  if (!(await regularFile(installer))) failure("Electron postinstall entry is unavailable");
  await runChecked(
    runner,
    command(nativeNode, [installer], dirname(installer), environment),
    "Electron runtime postinstall",
  );
  if (!(await executableFile(executable)))
    failure("Electron postinstall completed without its native runtime");
}

async function verifyRuntimePayload(runtimeRoot) {
  const required = [
    ...PACKAGE_COMMANDS.map(([, entry]) => join(runtimeRoot, entry)),
    join(runtimeRoot, "dist", "quicklens", "browser", "index.html"),
    join(runtimeRoot, "dist", "quicklens", "electron", "main.js"),
    join(runtimeRoot, "node_modules", "electron", "index.js"),
    join(runtimeRoot, "node_modules", "electron", "package.json"),
    join(runtimeRoot, "node_modules", "electron", "path.txt"),
    electronExecutable(join(runtimeRoot, "node_modules", "electron")),
  ];
  for (const path of required) {
    if (!(await regularFile(path)) && !(await executableFile(path)))
      failure(`runtime payload is incomplete at ${forward(relative(runtimeRoot, path))}`);
  }
}

function electronExecutable(electronRoot) {
  if (process.platform === "win32") return join(electronRoot, "dist", "electron.exe");
  if (process.platform === "darwin")
    return join(electronRoot, "dist", "Electron.app", "Contents", "MacOS", "Electron");
  return join(electronRoot, "dist", "electron");
}

async function stageEngine(state, destinationRoot) {
  const vibeArguments = [
    ...(state.input.config.offline ? ["--offline"] : []),
    "bin",
    "build",
    state.input.config.engine.binary,
    "--assume-yes",
  ];
  await runChecked(
    state.runner,
    command(
      state.input.config.engine.vibeExecutable,
      vibeArguments,
      state.hostRoot,
      cargoEnvironment(state.input.config.offline),
    ),
    "Vibe engine binary build",
  );
  const located = await runChecked(
    state.runner,
    command(
      state.input.config.engine.vibeExecutable,
      ["bin", "path", state.input.config.engine.binary],
      state.hostRoot,
      cargoEnvironment(state.input.config.offline),
    ),
    "Vibe engine binary lookup",
  );
  const source = resolve(located.stdout.trim());
  if (!(await executableFile(source)) || !contained(state.engineRoot, source))
    failure("Vibe returned an invalid engine artifact path");
  await mkdir(destinationRoot, { recursive: true });
  const destination = join(destinationRoot, basename(source));
  await copyFile(source, destination);
  await chmod(destination, 0o755);
  return destination;
}

async function publishPrepared(hostRoot, buildRoot, generationRoot, generation) {
  const stableLaunchers = join(buildRoot, "launchers");
  const pending = join(buildRoot, `.launchers-${randomUUID()}`);
  const backup = join(buildRoot, `.launchers-backup-${randomUUID()}`);
  await ensureOwnedDirectory(hostRoot, buildRoot);
  if (!contained(buildRoot, pending)) failure("pending launcher path escaped its owner");
  await cp(join(generationRoot, "launchers"), pending, {
    recursive: true,
    force: false,
    errorOnExist: true,
  });
  let retainedPrior = false;
  try {
    if (await ownedDirectory(buildRoot, stableLaunchers)) {
      await rename(stableLaunchers, backup);
      retainedPrior = true;
    }
    await rename(pending, stableLaunchers);
  } catch (error) {
    if (retainedPrior && !(await ownedDirectory(buildRoot, stableLaunchers)))
      await rename(backup, stableLaunchers);
    await removeOwnedTree(buildRoot, pending);
    throw error;
  }
  if (retainedPrior) await removeOwnedTree(buildRoot, backup);
  const prepared = {
    protocol: PREPARED_PROTOCOL,
    generationId: generation.generationId,
    generationRoot: generation.generationRoot,
    launchers: generation.launchers.map((launcher) => ({
      ...launcher,
      path: projectRelative(hostRoot, join(stableLaunchers, basename(launcher.path))),
    })),
    engineEnabled: generation.engineEnabled,
    lensVersion: generation.lensVersion,
    engineVersion: generation.engineVersion,
  };
  const temporary = join(buildRoot, `.prepared-${randomUUID()}.json`);
  await writeFile(temporary, `${JSON.stringify(prepared, null, 2)}\n`, { flag: "wx" });
  await rename(temporary, join(buildRoot, "prepared.json"));
  return prepared;
}

function generationRecord(state, launchers, payloadDigest) {
  return {
    protocol: GENERATION_PROTOCOL,
    generationId: state.generationId,
    generationRoot: projectRelative(state.hostRoot, state.generationRoot),
    runtimeRoot: projectRelative(state.hostRoot, join(state.generationRoot, "runtime")),
    sourceDigest: state.sourceDigest,
    payloadDigest,
    platform: process.platform,
    architecture: process.arch,
    nodeVersion: process.versions.node,
    lensVersion: state.input.config.lens.version,
    engineVersion: state.input.engine?.version ?? null,
    engineEnabled: state.input.config.engine.enabled,
    launchers,
  };
}

export function parseGenerationRecord(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  const keys = Object.keys(value).sort().join(",");
  if (
    keys !==
    "architecture,engineEnabled,engineVersion,generationId,generationRoot,launchers,lensVersion,nodeVersion,payloadDigest,platform,protocol,runtimeRoot,sourceDigest"
  )
    return null;
  if (
    value.protocol !== GENERATION_PROTOCOL ||
    !hex64(value.generationId) ||
    !hex64(value.sourceDigest) ||
    !hex64(value.payloadDigest) ||
    typeof value.generationRoot !== "string" ||
    typeof value.runtimeRoot !== "string" ||
    typeof value.platform !== "string" ||
    typeof value.architecture !== "string" ||
    typeof value.nodeVersion !== "string" ||
    typeof value.lensVersion !== "string" ||
    typeof value.engineEnabled !== "boolean" ||
    (value.engineVersion !== null && typeof value.engineVersion !== "string") ||
    !Array.isArray(value.launchers)
  )
    return null;
  const launchers = value.launchers.map(parseLauncherRow);
  if (launchers.some((launcher) => launcher === null)) return null;
  return { ...value, launchers };
}

async function validGeneration(hostRoot, root, expectedId, expectedSourceDigest) {
  try {
    if (!(await ownedDirectory(dirname(root), root))) return null;
    const record = parseGenerationRecord(
      JSON.parse(await readFile(join(root, "generation.json"), "utf8")),
    );
    if (
      record === null ||
      record.generationId !== expectedId ||
      record.sourceDigest !== expectedSourceDigest ||
      record.generationRoot !== projectRelative(hostRoot, root) ||
      (record.engineEnabled ? record.engineVersion === null : record.engineVersion !== null) ||
      record.launchers.length !== launcherFileNames(record.engineEnabled === true).length
    )
      return null;
    const runtimeRoot = join(root, "runtime");
    if (
      !(await ownedDirectory(root, runtimeRoot)) ||
      !(await ownedDirectory(runtimeRoot, join(runtimeRoot, "dist"))) ||
      !(await ownedDirectory(runtimeRoot, join(runtimeRoot, "node_modules")))
    )
      return null;
    const expectedLaunchers = new Set(launcherFileNames(record.engineEnabled));
    for (const launcher of record.launchers) {
      const file = basename(launcher.path);
      if (
        !expectedLaunchers.delete(file) ||
        launcher.path !== `${record.generationRoot}/launchers/${file}`
      )
        return null;
      if (!(await regularFile(resolve(hostRoot, launcher.path)))) return null;
    }
    if (expectedLaunchers.size !== 0) return null;
    if (record.engineEnabled) {
      const binaries = await readdir(join(root, "runtime", "bin"), { withFileTypes: true });
      if (binaries.length !== 1 || !binaries[0]?.isFile()) return null;
    }
    await verifyRuntimePayload(runtimeRoot);
    const observedPayload = digestJson({
      runtime: await digestPayloadTree(runtimeRoot),
      launchers: await digestPayloadTree(join(root, "launchers")),
    });
    if (observedPayload !== record.payloadDigest) return null;
    return record;
  } catch {
    return null;
  }
}

function parseLauncherRow(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  if (Object.keys(value).sort().join(",") !== "command,path,platform") return null;
  if (
    typeof value.command !== "string" ||
    typeof value.path !== "string" ||
    !["windows-cmd", "windows-powershell", "posix"].includes(value.platform)
  )
    return null;
  return { command: value.command, platform: value.platform, path: value.path };
}

function hex64(value) {
  return typeof value === "string" && /^[a-f0-9]{64}$/u.test(value);
}

async function writeLaunchers(root, launchers) {
  await mkdir(root, { recursive: true });
  for (const launcher of launchers) {
    const path = join(root, launcher.fileName);
    await writeFile(path, launcher.body, { flag: "wx", mode: launcher.mode });
    await chmod(path, launcher.mode);
  }
}

function command(executable, args, cwd, environment) {
  return { executable, args, cwd, environment: { ...environment } };
}

function cargoEnvironment(offline) {
  return {
    ...process.env,
    ...windowsPathExt(process.env),
    ...windowsToolchainRoots(process.env),
    ...(offline ? { CARGO_NET_OFFLINE: "true" } : {}),
  };
}

function npmEnvironment(offline, registry) {
  return {
    ...withoutNpmRegistry(process.env),
    ...windowsPathExt(process.env),
    npm_config_audit: "false",
    npm_config_fund: "false",
    ...(registry === null ? {} : { NPM_CONFIG_REGISTRY: registry }),
    ...(offline ? { npm_config_offline: "true" } : {}),
  };
}

function withoutNpmRegistry(environment) {
  return Object.fromEntries(
    Object.entries(environment).filter(([name]) => name.toLowerCase() !== "npm_config_registry"),
  );
}

function npmRegistryArguments(registry) {
  return registry === null ? [] : ["--registry", registry];
}

function windowsPathExt(environment) {
  if (process.platform !== "win32") return {};
  const existing = environment.PATHEXT ?? environment.Pathext ?? environment.pathext ?? "";
  const values = existing
    .split(";")
    .map((value) => value.trim().toUpperCase())
    .filter((value) => value !== "");
  for (const required of [".COM", ".EXE", ".BAT", ".CMD"])
    if (!values.includes(required)) values.push(required);
  return { PATHEXT: values.join(";") };
}

function windowsToolchainRoots(environment) {
  if (process.platform !== "win32") return {};
  const windows = environment.SystemRoot ?? environment.WINDIR;
  if (typeof windows !== "string" || windows.length === 0) return {};
  const driveRoot = dirname(resolve(windows));
  const programFiles = environment.ProgramFiles ?? join(driveRoot, "Program Files");
  return {
    ProgramFiles: programFiles,
    "ProgramFiles(x86)": environment["ProgramFiles(x86)"] ?? join(driveRoot, "Program Files (x86)"),
    ProgramW6432: environment.ProgramW6432 ?? programFiles,
  };
}

function offlineNpm(offline) {
  return offline ? ["--offline"] : [];
}

function projectRelative(hostRoot, path) {
  if (!contained(hostRoot, path)) failure("generated output escaped the installer host");
  return forward(relative(hostRoot, path));
}

function requireNode24() {
  const major = Number.parseInt(process.versions.node.split(".")[0] ?? "0", 10);
  if (!Number.isInteger(major) || major < 24) failure("Node 24 or newer is required");
}
