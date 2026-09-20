#!/usr/bin/env node
/** POSIX Zap distribution builder. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import { randomUUID } from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  mkdir,
  mkdtemp,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { basename, delimiter, dirname, join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import {
  PUBLIC_COMMANDS,
  SOURCE_REPOSITORY,
  VERSION,
  command,
  containsNativeAddon,
  copySourceTree,
  createRunner,
  nodeLicensePath,
  parseCargoMetadata,
  pathKind,
  requireFile,
  runChecked,
  sha256File,
  validateInput,
} from "./build-windows.mjs";
import { stageLicenseNotices } from "./license-notices.mjs";
import {
  DISTRIBUTION_DESCRIPTOR,
  sealDistributionDirectory,
  verifyDistributionDirectory,
} from "./manifest.mjs";
import { inspectSourceWitness, verifySourceWitness } from "./source-witness.mjs";

const TARGETS = Object.freeze({
  "linux-x64-musl": Object.freeze({
    os: "linux",
    arch: "x86_64",
    libc: "musl",
    cargo: "x86_64-unknown-linux-musl",
    electron: "node_modules/electron/dist/electron",
  }),
  "linux-x64-gnu": Object.freeze({
    os: "linux",
    arch: "x86_64",
    libc: "gnu",
    cargo: "x86_64-unknown-linux-gnu",
    electron: "node_modules/electron/dist/electron",
  }),
  "macos-x64": Object.freeze({
    os: "macos",
    arch: "x86_64",
    cargo: "x86_64-apple-darwin",
    electron: "node_modules/electron/dist/Electron.app/Contents/MacOS/Electron",
  }),
  "macos-arm64": Object.freeze({
    os: "macos",
    arch: "aarch64",
    cargo: "aarch64-apple-darwin",
    electron: "node_modules/electron/dist/Electron.app/Contents/MacOS/Electron",
  }),
});

export async function buildPosixDistribution(input, ports = {}) {
  const target = TARGETS[input.target];
  if (target === undefined) failure(`unsupported target ${input.target}`);
  const options = validateInput({ ...input, tarExecutable: input.tarExecutable ?? "tar" });
  const runner = ports.runner ?? createRunner();
  const witness = await verifySourceWitness(options, {
    runner,
    sourceSnapshot: ports.sourceSnapshot,
  });
  options.lensRoot = join(witness.root, relative(options.sourceRoot, options.lensRoot));
  options.engineRoot = join(witness.root, relative(options.sourceRoot, options.engineRoot));
  const slug = `zap-${input.target}-${VERSION}`;
  const archiveName = `${slug}.zip`;
  try {
    const outputRoot = resolve(options.outputDirectory);
    await mkdir(outputRoot, { recursive: true });
    const finalDirectory = join(outputRoot, slug);
    const finalArchive = join(outputRoot, archiveName);
    const finalReceipt = join(outputRoot, `${slug}.build.json`);
    for (const path of [finalDirectory, finalArchive, finalReceipt])
      if ((await pathKind(path)) !== "absent") failure(`output already exists: ${basename(path)}`);

    const staging = await mkdtemp(join(outputRoot, `.zap-distribution-${randomUUID()}-`));
    try {
      const lensBuild = join(staging, "lens-build");
      const cargoTarget = options.cargoTargetDirectory ?? join(staging, "cargo-target");
      const bundle = join(staging, slug);
      await copySourceTree(options.lensRoot, lensBuild);
      const nodeExecutable = join(options.nodeRoot, "bin", "node");
      const npmCli = join(options.nodeRoot, "lib", "node_modules", "npm", "bin", "npm-cli.js");
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
      const expectedNodeArch = target.arch === "aarch64" ? "arm64" : "x64";
      if (nodeArchitecture !== expectedNodeArch)
        failure(`portable runtime must be ${input.target} Node.js`);

      const npmEnvironment = nodeEnvironment(options.nodeRoot);
      const npmBase = [npmCli, "--no-audit", "--no-fund", ...(options.offline ? ["--offline"] : [])];
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
      await ensureElectronRuntime(
        lensBuild,
        nodeExecutable,
        runner,
        npmEnvironment,
        options.offline,
        target.electron,
      );
      await rm(join(lensBuild, "node_modules", ".bin"), { recursive: true, force: true });
      await verifyLensRuntime(lensBuild, target.electron);

      const cargoEnvironment = { ...process.env, CARGO_TARGET_DIR: cargoTarget };
      await runChecked(
        runner,
        command(
          options.cargoExecutable,
          [
            "build",
            "--locked",
            "--release",
            "--target",
            target.cargo,
            "--bin",
            "zap",
            ...(options.offline ? ["--offline"] : []),
          ],
          options.engineRoot,
          cargoEnvironment,
        ),
        "Rust Zap release build",
      );
      const zapExecutable = join(cargoTarget, target.cargo, "release", "zap");
      await requireFile(zapExecutable, "Rust Zap executable");
      const metadata = await runChecked(
        runner,
        command(
          options.cargoExecutable,
          ["metadata", "--locked", "--format-version", "1", ...(options.offline ? ["--offline"] : [])],
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
        target,
        cargoMetadata: parseCargoMetadata(metadata.stdout),
      });
      const launchers = [];
      for (const [publicCommand] of PUBLIC_COMMANDS) {
        const path = `launchers/${publicCommand}`;
        const absolute = join(bundle, ...path.split("/"));
        await writeFile(absolute, publicLauncher(publicCommand), "utf8");
        await chmod(absolute, 0o755);
        launchers.push({ command: publicCommand, path, destination: publicCommand });
      }
      const descriptor = await sealDistributionDirectory(bundle, {
        sourceCommit: options.sourceCommit,
        sourceTree: options.sourceTree,
        os: target.os,
        arch: target.arch,
        libc: target.libc,
        managementEntry: "management/launch.sh",
        launchers,
      });
      await verifyDistributionDirectory(bundle);
      const pendingArchive = join(staging, archiveName);
      await createZip(bundle, pendingArchive, runner);
      await requireFile(pendingArchive, "distribution ZIP");
      const archiveState = await stat(pendingArchive);
      const receipt = {
        protocol: "zap-distribution-build/1",
        application: "org.vibevm.zap/zap@1.0.0",
        target: {
          os: target.os,
          arch: target.arch,
          ...(target.libc === undefined ? {} : { libc: target.libc }),
          format: "zip",
        },
        source: {
          repository: SOURCE_REPOSITORY,
          commit: options.sourceCommit,
          tree: options.sourceTree,
        },
        descriptorSha256: await sha256File(join(bundle, DISTRIBUTION_DESCRIPTOR)),
        archive: {
          file: archiveName,
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
      return { directory: finalDirectory, archive: finalArchive, receipt: finalReceipt, descriptor };
    } catch (error) {
      try {
        await rename(staging, `${staging}.failed`);
      } catch {
        // Keep the original diagnostic path if renaming it fails.
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
  for (const directory of [app, node, binary, licenses, join(input.bundle, "launchers"), join(input.bundle, "management")])
    await mkdir(directory, { recursive: true });
  for (const directory of ["dist", "node_modules"])
    await cp(join(input.lensBuild, directory), join(app, directory), {
      recursive: true,
      dereference: directory === "node_modules",
      force: false,
      errorOnExist: true,
    });
  for (const file of ["package.json", "package-lock.json", "LICENSE.md", "README.md"])
    await copyFile(join(input.lensBuild, file), join(app, file));

  const stagedNode = join(node, "bin", "node");
  await mkdir(dirname(stagedNode), { recursive: true });
  await copyFile(input.nodeExecutable, stagedNode);
  await chmod(stagedNode, 0o755);
  let nodeCommand = "payload/node/bin/node";
  if (input.target.libc === "musl") {
    const lib = join(node, "lib");
    await mkdir(lib, { recursive: true });
    for (const source of [
      "/lib/ld-musl-x86_64.so.1",
      "/usr/lib/libstdc++.so.6",
      "/usr/lib/libgcc_s.so.1",
    ])
      await copyFile(source, join(lib, basename(source)));
    const wrapper = join(node, "run-node.sh");
    await writeFile(wrapper, linuxNodeLauncher(), "utf8");
    await chmod(wrapper, 0o755);
    nodeCommand = "payload/node/run-node.sh";
  }
  await copyFile(await nodeLicensePath(input.nodeRoot), join(licenses, "NODE-LICENSE.txt"));
  await copyFile(join(input.engineRoot, "LICENSE.md"), join(licenses, "ZAP-LICENSE.md"));
  const stagedZap = join(binary, "zap");
  await copyFile(input.zapExecutable, stagedZap);
  await chmod(stagedZap, 0o755);
  await writeFile(join(app, "distribution-launch.mjs"), distributionDispatcher(nodeCommand), "utf8");
  const management = join(input.bundle, "management", "launch.sh");
  await writeFile(management, managementLauncher(nodeCommand), "utf8");
  await chmod(management, 0o755);
  await stageLicenseNotices({
    cargoMetadata: input.cargoMetadata,
    licensesRoot: licenses,
    lensBuild: input.lensBuild,
    nodeVersion: input.nodeVersion,
  });
}

async function verifyLensRuntime(root, electron) {
  for (const path of [
    ...new Set(PUBLIC_COMMANDS.map(([, entry]) => entry).filter((entry) => entry !== null)),
    electron,
    "node_modules/node-pty/package.json",
  ])
    await requireFile(join(root, ...path.split("/")), `Lens runtime ${path}`);
  if (!(await containsNativeAddon(join(root, "node_modules", "node-pty"))))
    failure("Lens runtime has no native node-pty addon");
}

async function ensureElectronRuntime(
  root,
  nodeExecutable,
  runner,
  environment,
  offline,
  executablePath,
) {
  const electronRoot = join(root, "node_modules", "electron");
  const executable = join(root, ...executablePath.split("/"));
  if ((await pathKind(executable)) === "file") return;
  if (offline) failure("offline binary build has no cached Electron runtime");
  const installer = join(electronRoot, "install.js");
  await requireFile(installer, "Electron runtime installer");
  await runChecked(
    runner,
    command(nodeExecutable, [installer], electronRoot, environment),
    "Electron runtime installation",
  );
  await requireFile(executable, "Electron runtime executable");
}

function publicLauncher(publicCommand) {
  return `#!/bin/sh
set -eu
ZAP_OPT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec "$ZAP_OPT_ROOT/apps/zap/management/launch.sh" ${shellWord(publicCommand)} "$@"
`;
}

function managementLauncher(nodeCommand) {
  return `#!/bin/sh
set -eu
ZAP_GENERATION_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec "$ZAP_GENERATION_ROOT/${nodeCommand}" "$ZAP_GENERATION_ROOT/payload/app/distribution-launch.mjs" "$@"
`;
}

function linuxNodeLauncher() {
  return `#!/bin/sh
set -eu
NODE_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exec "$NODE_ROOT/lib/ld-musl-x86_64.so.1" --library-path "$NODE_ROOT/lib" "$NODE_ROOT/bin/node" "$@"
`;
}

function distributionDispatcher(nodeCommand) {
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
  const node = resolve(generation, ...${JSON.stringify(nodeCommand.split("/"))});
  const engine = resolve(generation, "payload", "bin", "zap");
  const executable = entry === null ? engine : node;
  const launchArgs = entry === null ? args : [resolve(generation, "payload", "app", ...entry.split("/")), ...args];
  const child = spawn(executable, launchArgs, {
    cwd: process.cwd(),
    env: { ...process.env, ZAP_ENGINE_BINARY: engine },
    stdio: "inherit",
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

function nodeEnvironment(nodeRoot) {
  const nodeBin = join(nodeRoot, "bin");
  const inherited = process.env.PATH ?? "";
  return { ...process.env, PATH: inherited === "" ? nodeBin : `${nodeBin}${delimiter}${inherited}` };
}

async function createZip(source, destination, runner) {
  const list = `find . -type f -print | sed 's#^\\./##' | LC_ALL=C sort | zip -q -X ${shellWord(destination)} -@`;
  await runChecked(runner, command("sh", ["-c", list], source), "distribution ZIP creation");
}

function shellWord(value) {
  return `'${String(value).replaceAll("'", `'\\''`)}'`;
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#binary: ${message}; fix surface: rebuild the exact Zap POSIX distribution`,
  );
}

function parseArgs(argv) {
  const options = { offline: false, cargoExecutable: "cargo", gitExecutable: "git", tarExecutable: "tar" };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--offline") options.offline = true;
    else if (argument === "--print-source-witness") options.printSourceWitness = true;
    else if (argument === "--help" || argument === "-h") options.help = true;
    else if (["--target", "--lens-root", "--engine-root", "--node-root", "--source-root", "--output", "--source-commit", "--source-tree", "--cargo", "--cargo-target-dir", "--git", "--tar"].includes(argument)) {
      const next = argv[index + 1];
      if (next === undefined) failure(`${argument} needs a value`);
      const field = {
        "--target": "target",
        "--lens-root": "lensRoot",
        "--engine-root": "engineRoot",
        "--node-root": "nodeRoot",
        "--source-root": "sourceRoot",
        "--output": "outputDirectory",
        "--source-commit": "sourceCommit",
        "--source-tree": "sourceTree",
        "--cargo": "cargoExecutable",
        "--cargo-target-dir": "cargoTargetDirectory",
        "--git": "gitExecutable",
        "--tar": "tarExecutable",
      }[argument];
      options[field] = next;
      index += 1;
    } else failure(`unknown argument ${argument}`);
  }
  return options;
}

function helpText() {
  return `Zap POSIX distribution builder

Usage: node tooling/distribution/build-posix.mjs --target TARGET [options]

Targets: linux-x64-musl, linux-x64-gnu, macos-x64, macos-arm64
The remaining source/node/output/witness flags match build-windows.mjs.
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
  const result = await buildPosixDistribution(options);
  process.stdout.write(`${JSON.stringify({ archive: result.archive, receipt: result.receipt })}\n`);
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  await main();
