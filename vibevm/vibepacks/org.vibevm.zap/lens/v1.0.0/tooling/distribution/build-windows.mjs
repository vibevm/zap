#!/usr/bin/env node
/** Windows x64 standalone Zap distribution builder. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import { createHash, randomUUID } from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  DISTRIBUTION_DESCRIPTOR,
  sealDistributionDirectory,
  verifyDistributionDirectory,
} from "./manifest.mjs";
import { stageLicenseNotices } from "./license-notices.mjs";
import {
  inspectSourceWitness,
  portableContentHash,
  verifySourceWitness,
} from "./source-witness.mjs";

export { inspectSourceWitness, portableContentHash } from "./source-witness.mjs";

const VERSION = "1.0.0";
const ARCHIVE_NAME = `zap-windows-x64-${VERSION}.zip`;
const DIRECTORY_NAME = `zap-windows-x64-${VERSION}`;
const CARGO_TARGET = "x86_64-pc-windows-msvc";
const SOURCE_REPOSITORY = "https://github.com/vibevm/zap.git";
const PUBLIC_COMMANDS = Object.freeze([
  ["zap-quicklens", "dist/zap-quick-lens.js"],
  ["zap-server", "dist/zap-server.js"],
]);

export async function buildWindowsDistribution(input, ports = {}) {
  const options = validateInput(input);
  const runner = ports.runner ?? createRunner();
  const zipper = ports.zip ?? createZip;
  const witness = await verifySourceWitness(options, {
    runner,
    sourceSnapshot: ports.sourceSnapshot,
  });
  options.lensRoot = join(witness.root, relative(options.sourceRoot, options.lensRoot));
  options.engineRoot = join(witness.root, relative(options.sourceRoot, options.engineRoot));
  try {
    const outputRoot = resolve(options.outputDirectory);
    await mkdir(outputRoot, { recursive: true });
    const finalDirectory = join(outputRoot, DIRECTORY_NAME);
    const finalArchive = join(outputRoot, ARCHIVE_NAME);
    const finalReceipt = join(outputRoot, `${DIRECTORY_NAME}.build.json`);
    for (const path of [finalDirectory, finalArchive, finalReceipt])
      if ((await pathKind(path)) !== "absent") failure(`output already exists: ${basename(path)}`);

    const staging = await mkdtemp(join(outputRoot, `.zap-distribution-${randomUUID()}-`));
    try {
      const lensBuild = join(staging, "lens-build");
      const cargoTarget = join(staging, "cargo-target");
      const bundle = join(staging, DIRECTORY_NAME);
      await copySourceTree(options.lensRoot, lensBuild);
      const nodeExecutable = join(options.nodeRoot, "node.exe");
      const npmCli = join(options.nodeRoot, "node_modules", "npm", "bin", "npm-cli.js");
      await requireFile(nodeExecutable, "portable Node executable");
      await requireFile(npmCli, "portable npm CLI");
      const nodeVersion = (
        await runChecked(runner, command(nodeExecutable, ["--version"], lensBuild), "Node version")
      ).stdout.trim();
      if (!/^v24\./u.test(nodeVersion)) failure("portable runtime must be Node.js 24");
      const nodeArchitecture = (
        await runChecked(
          runner,
          command(nodeExecutable, ["-p", "process.arch"], lensBuild),
          "Node architecture",
        )
      ).stdout.trim();
      if (nodeArchitecture !== "x64") failure("portable runtime must be Windows x64 Node.js");

      const npmEnvironment = bundledNodeEnvironment(options.nodeRoot);
      const npmBase = [
        npmCli,
        "--no-audit",
        "--no-fund",
        ...(options.offline ? ["--offline"] : []),
      ];
      await runChecked(
        runner,
        command(nodeExecutable, [npmBase[0], "ci", ...npmBase.slice(1)], lensBuild, npmEnvironment),
        "locked Lens dependency installation",
      );
      await runChecked(
        runner,
        command(nodeExecutable, [npmCli, "run", "build"], lensBuild, npmEnvironment),
        "Lens production build",
      );
      await runChecked(
        runner,
        command(
          nodeExecutable,
          [npmBase[0], "ci", "--omit=dev", ...npmBase.slice(1)],
          lensBuild,
          npmEnvironment,
        ),
        "locked Lens production dependency installation",
      );
      await verifyLensRuntime(lensBuild);

      const cargoEnvironment = { ...process.env, CARGO_TARGET_DIR: cargoTarget };
      const cargoArguments = [
        "build",
        "--locked",
        "--release",
        "--target",
        CARGO_TARGET,
        "--bin",
        "zap",
        ...(options.offline ? ["--offline"] : []),
      ];
      await runChecked(
        runner,
        command(options.cargoExecutable, cargoArguments, options.engineRoot, cargoEnvironment),
        "Rust Zap release build",
      );
      const zapExecutable = join(cargoTarget, CARGO_TARGET, "release", "zap.exe");
      await requireFile(zapExecutable, "Rust Zap executable");
      const metadata = await runChecked(
        runner,
        command(
          options.cargoExecutable,
          [
            "metadata",
            "--locked",
            "--format-version",
            "1",
            ...(options.offline ? ["--offline"] : []),
          ],
          options.engineRoot,
          cargoEnvironment,
        ),
        "Rust license metadata",
      );

      await stageBundle({
        bundle,
        lensBuild,
        engineRoot: options.engineRoot,
        nodeRoot: options.nodeRoot,
        nodeExecutable,
        nodeVersion,
        zapExecutable,
        cargoMetadata: parseCargoMetadata(metadata.stdout),
      });
      const launchers = [];
      for (const [publicCommand] of PUBLIC_COMMANDS) {
        const path = `launchers/${publicCommand}.cmd`;
        await writeFile(join(bundle, ...path.split("/")), publicLauncher(publicCommand), "utf8");
        launchers.push({ command: publicCommand, path, destination: `${publicCommand}.cmd` });
      }
      const descriptor = await sealDistributionDirectory(bundle, {
        sourceCommit: options.sourceCommit,
        sourceTree: options.sourceTree,
        launchers,
      });
      await verifyDistributionDirectory(bundle);
      const pendingArchive = join(staging, ARCHIVE_NAME);
      await zipper(bundle, pendingArchive, runner);
      await requireFile(pendingArchive, "distribution ZIP");
      const archiveState = await stat(pendingArchive);
      const receipt = {
        protocol: "zap-distribution-build/1",
        application: "org.vibevm.zap/zap@1.0.0",
        target: { os: "windows", arch: "x86_64", format: "zip" },
        source: {
          repository: SOURCE_REPOSITORY,
          commit: options.sourceCommit,
          tree: options.sourceTree,
        },
        descriptorSha256: await sha256File(join(bundle, DISTRIBUTION_DESCRIPTOR)),
        archive: {
          file: ARCHIVE_NAME,
          sha256: await sha256File(pendingArchive),
          size: archiveState.size,
        },
        files: descriptor.files.length,
      };
      await rename(bundle, finalDirectory);
      await rename(pendingArchive, finalArchive);
      await writeFile(finalReceipt, `${JSON.stringify(receipt, null, 2)}\n`, {
        encoding: "utf8",
        flag: "wx",
      });
      await rm(staging, { recursive: true, force: true });
      return {
        directory: finalDirectory,
        archive: finalArchive,
        receipt: finalReceipt,
        descriptor,
      };
    } catch (error) {
      const failurePath = `${staging}.failed`;
      try {
        await rename(staging, failurePath);
      } catch {
        // Preserve the original unique staging path if the diagnostic rename fails.
      }
      throw error;
    }
  } finally {
    await witness.cleanup();
  }
}

async function stageBundle(input) {
  const app = join(input.bundle, "payload", "app");
  const node = join(input.bundle, "payload", "node");
  const binary = join(input.bundle, "payload", "bin");
  const licenses = join(input.bundle, "licenses");
  for (const directory of [
    app,
    node,
    binary,
    licenses,
    join(input.bundle, "launchers"),
    join(input.bundle, "management"),
  ])
    await mkdir(directory, { recursive: true });
  for (const directory of ["dist", "node_modules"])
    await cp(join(input.lensBuild, directory), join(app, directory), {
      recursive: true,
      force: false,
      errorOnExist: true,
    });
  for (const file of ["package.json", "package-lock.json", "LICENSE.md", "README.md"])
    await copyFile(join(input.lensBuild, file), join(app, file));
  await copyFile(input.nodeExecutable, join(node, "node.exe"));
  await chmod(join(node, "node.exe"), 0o755);
  const nodeLicense = await nodeLicensePath(input.nodeRoot);
  await copyFile(nodeLicense, join(licenses, "NODE-LICENSE.txt"));
  await copyFile(join(input.engineRoot, "LICENSE.md"), join(licenses, "ZAP-LICENSE.md"));
  await copyFile(input.zapExecutable, join(binary, "zap.exe"));
  await chmod(join(binary, "zap.exe"), 0o755);
  await writeFile(join(app, "distribution-launch.mjs"), distributionDispatcher(), "utf8");
  await writeFile(join(input.bundle, "management", "launch.cmd"), managementLauncher(), "utf8");
  await stageLicenseNotices({
    cargoMetadata: input.cargoMetadata,
    licensesRoot: licenses,
    lensBuild: input.lensBuild,
    nodeVersion: input.nodeVersion,
  });
}

async function verifyLensRuntime(root) {
  for (const path of [
    "dist/zap-quick-lens.js",
    "dist/zap-server.js",
    "node_modules/electron/dist/electron.exe",
    "node_modules/node-pty/package.json",
  ])
    await requireFile(join(root, ...path.split("/")), `Lens runtime ${path}`);
  if (!(await containsNativeAddon(join(root, "node_modules", "node-pty"))))
    failure("Lens runtime has no native node-pty addon");
}

async function containsNativeAddon(root) {
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory() && (await containsNativeAddon(path))) return true;
    if (entry.isFile() && entry.name.endsWith(".node")) return true;
  }
  return false;
}

async function copySourceTree(source, destination) {
  const excluded = new Set([".git", ".vibe", "dist", "node_modules", "target"]);
  await cp(source, destination, {
    recursive: true,
    force: false,
    errorOnExist: true,
    filter: (path) => !excluded.has(basename(path)),
  });
}

function publicLauncher(publicCommand) {
  return `@echo off\r
setlocal\r
set "ZAP_OPT_ROOT=%~dp0.."\r
call "%ZAP_OPT_ROOT%\\apps\\zap\\management\\launch.cmd" ${publicCommand} %*\r
exit /b %ERRORLEVEL%\r
`;
}

function managementLauncher() {
  return `@echo off\r
setlocal\r
set "ZAP_GENERATION_ROOT=%~dp0.."\r
"%ZAP_GENERATION_ROOT%\\payload\\node\\node.exe" "%ZAP_GENERATION_ROOT%\\payload\\app\\distribution-launch.mjs" %*\r
exit /b %ERRORLEVEL%\r
`;
}

function distributionDispatcher() {
  const rows = PUBLIC_COMMANDS.map(
    ([commandName, entry]) => `  ${JSON.stringify(commandName)}: ${JSON.stringify(entry)},`,
  ).join("\n");
  return `import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const entries = Object.freeze({
${rows}
});
const [command, ...args] = process.argv.slice(2);
const entry = entries[command];
if (entry === undefined) {
  process.stderr.write("Zap distribution command is unavailable\\n");
  process.exitCode = 64;
} else {
  const generation = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
  const node = resolve(generation, "payload", "node", "node.exe");
  const script = resolve(generation, "payload", "app", ...entry.split("/"));
  const engine = resolve(generation, "payload", "bin", "zap.exe");
  const child = spawn(node, [script, ...args], {
    cwd: process.cwd(),
    env: { ...process.env, ZAP_ENGINE_BINARY: engine },
    stdio: "inherit",
    windowsHide: false,
  });
  child.once("error", (error) => {
    process.stderr.write("Zap distribution launch failed: " + error.message + "\\n");
    process.exitCode = 1;
  });
  child.once("exit", (code, signal) => {
    process.exitCode = signal === null ? (code ?? 1) : 1;
  });
}
`;
}

async function nodeLicensePath(nodeRoot) {
  for (const name of ["LICENSE", "LICENSE.txt"])
    if ((await pathKind(join(nodeRoot, name))) === "file") return join(nodeRoot, name);
  failure("portable Node license is unavailable");
}

function parseCargoMetadata(value) {
  try {
    const parsed = JSON.parse(value);
    if (!Array.isArray(parsed.packages)) failure("cargo metadata packages are unavailable");
    return parsed;
  } catch (error) {
    if (error instanceof SyntaxError) failure("cargo metadata is not JSON");
    throw error;
  }
}

function bundledNodeEnvironment(nodeRoot) {
  const environment = Object.fromEntries(
    Object.entries(process.env).filter(([name]) => name.toLowerCase() !== "path"),
  );
  const inherited = Object.entries(process.env).find(
    ([name]) => name.toLowerCase() === "path",
  )?.[1];
  return { ...environment, PATH: inherited ? `${nodeRoot};${inherited}` : nodeRoot };
}

function contained(parent, child) {
  const path = relative(resolve(parent), resolve(child));
  return path !== "" && path !== ".." && !path.startsWith(`..${sep}`) && !isAbsolute(path);
}

function validateInput(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    failure("input is invalid");
  for (const field of ["sourceRoot", "lensRoot", "engineRoot", "nodeRoot", "outputDirectory"])
    if (typeof value[field] !== "string" || !isAbsolute(value[field]))
      failure(`${field} must be absolute`);
  if (typeof value.sourceCommit !== "string" || !/^[a-f0-9]{40,64}$/u.test(value.sourceCommit))
    failure("sourceCommit is invalid");
  if (
    typeof value.sourceTree !== "string" ||
    !/^sha256-tree\/1:[a-f0-9]{64}$/u.test(value.sourceTree)
  )
    failure("sourceTree is invalid");
  const sourceRoot = resolve(value.sourceRoot);
  const lensRoot = resolve(value.lensRoot);
  const engineRoot = resolve(value.engineRoot);
  for (const [label, path] of [
    ["lensRoot", lensRoot],
    ["engineRoot", engineRoot],
  ])
    if (!contained(sourceRoot, path)) failure(`${label} must be inside sourceRoot`);
  if (contained(sourceRoot, resolve(value.outputDirectory)))
    failure("outputDirectory must be outside sourceRoot");
  return {
    sourceRoot,
    lensRoot,
    engineRoot,
    nodeRoot: resolve(value.nodeRoot),
    outputDirectory: resolve(value.outputDirectory),
    sourceCommit: value.sourceCommit,
    sourceTree: value.sourceTree,
    cargoExecutable: value.cargoExecutable ?? "cargo",
    gitExecutable: value.gitExecutable ?? "git",
    tarExecutable: value.tarExecutable ?? "tar.exe",
    offline: value.offline === true,
  };
}

async function createZip(source, destination, runner) {
  const script = fileURLToPath(new URL("./create-zip.ps1", import.meta.url));
  await runChecked(
    runner,
    command("powershell.exe", ["-NoProfile", "-NonInteractive", "-File", script], dirname(source), {
      ...process.env,
      ZAP_DISTRIBUTION_SOURCE: source,
      ZAP_DISTRIBUTION_ZIP: destination,
    }),
    "distribution ZIP creation",
  );
}

function createRunner() {
  return {
    async run(request) {
      const { spawn } = await import("node:child_process");
      return new Promise((accept, reject) => {
        const child = spawn(request.executable, request.args, {
          cwd: request.cwd,
          env: request.environment,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
        let stdout = "";
        let stderr = "";
        child.stdout.setEncoding("utf8").on("data", (chunk) => (stdout += chunk));
        child.stderr.setEncoding("utf8").on("data", (chunk) => (stderr += chunk));
        child.once("error", reject);
        child.once("close", (code) => accept({ code: code ?? -1, stdout, stderr }));
      });
    },
  };
}

function command(executable, args, cwd, environment = process.env) {
  return { executable, args, cwd, environment };
}

async function runChecked(runner, request, label) {
  const result = await runner.run(request);
  if (result.code !== 0) failure(`${label} failed with exit ${result.code}`);
  return result;
}

async function requireFile(path, label) {
  if ((await pathKind(path)) !== "file") failure(`${label} is unavailable`);
}

async function pathKind(path) {
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

async function sha256File(path) {
  const digest = createHash("sha256");
  digest.update(await readFile(path));
  return digest.digest("hex");
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#binary: ${message}; fix surface: rebuild the standalone Windows x64 distribution`,
  );
}

function parseArgs(argv) {
  const options = { offline: false, cargoExecutable: "cargo", gitExecutable: "git" };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--offline") options.offline = true;
    else if (argument === "--print-source-witness") options.printSourceWitness = true;
    else if (argument === "--help" || argument === "-h") options.help = true;
    else if (
      [
        "--lens-root",
        "--engine-root",
        "--node-root",
        "--source-root",
        "--output",
        "--source-commit",
        "--source-tree",
        "--cargo",
        "--git",
      ].includes(argument)
    ) {
      const next = argv[index + 1];
      if (next === undefined) failure(`${argument} needs a value`);
      const field = {
        "--lens-root": "lensRoot",
        "--engine-root": "engineRoot",
        "--node-root": "nodeRoot",
        "--source-root": "sourceRoot",
        "--output": "outputDirectory",
        "--source-commit": "sourceCommit",
        "--source-tree": "sourceTree",
        "--cargo": "cargoExecutable",
        "--git": "gitExecutable",
      }[argument];
      options[field] = next;
      index += 1;
    } else failure(`unknown argument ${argument}`);
  }
  return options;
}

function helpText() {
  return `Zap Windows x64 distribution builder

Usage: node tooling/distribution/build-windows.mjs [options]

  --lens-root PATH       Lens 1.0.0 package source
  --engine-root PATH     Rust Zap 1.0.0 package source
  --node-root PATH       Unpacked official Node.js 24 Windows x64 distribution
  --source-root PATH     Clean standalone Zap Git checkout
  --output PATH          Absent/new release output directory
  --source-commit HEX    Exact immutable source commit
  --source-tree DIGEST   Exact sha256-tree/1:<hex> Vibe portable content hash
  --cargo PATH           Cargo executable (default cargo)
  --git PATH             Git executable (default git)
  --print-source-witness Print clean HEAD + Vibe portable tree without building
  --offline              Require npm and Cargo offline resolution
  -h, --help             Read-only help
`;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(helpText());
    return;
  }
  if (options.printSourceWitness) {
    const witness = await inspectSourceWitness(options);
    process.stdout.write(`${JSON.stringify(witness)}\n`);
    return;
  }
  const result = await buildWindowsDistribution(options);
  process.stdout.write(`${JSON.stringify({ archive: result.archive, receipt: result.receipt })}\n`);
}

if (
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
)
  await main();
