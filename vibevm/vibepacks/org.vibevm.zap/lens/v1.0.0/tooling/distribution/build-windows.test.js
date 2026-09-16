/** Windows binary bundle journey. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { buildWindowsDistribution, portableContentHash } from "./build-windows.mjs";

test("portable source hash matches the Vibe recipe-1 order-trap golden", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap portable hash "));
  try {
    for (const [path, bytes] of [
      ["spec-x.md", "c2libGluZyBmaWxlIG5hbWVkIHNwZWMteA=="],
      ["specX.md", "c2libGluZyBmaWxlIG5hbWVkIHNwZWNY"],
      ["vibe.toml", "bmFtZSA9ICJnb2xkZW4tb3JkZXItdHJhcCI="],
      ["spec/inner/a.md", "aW5zaWRlIHRoZSBzcGVjIGRpcmVjdG9yeQ=="],
    ]) {
      const target = join(root, ...path.split("/"));
      await mkdir(dirname(target), { recursive: true });
      await writeFile(target, Buffer.from(bytes, "base64"));
    }
    assert.equal(
      await portableContentHash(root),
      "sha256-tree/1:883014931b57171ab81add3cd2183b295768a1e05c01b1290ee0512cbc59eb6e",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("builder binds bundled Node, production dependencies, Rust engine and stable launchers", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap windows bundle "));
  try {
    const sourceRoot = join(root, "Source Snapshot");
    const lensRoot = join(sourceRoot, "Lens Source");
    const engineRoot = join(sourceRoot, "Engine Source");
    const nodeRoot = join(root, "Node Runtime");
    const outputDirectory = join(root, "Output");
    await fixtureSources(lensRoot, engineRoot, nodeRoot);
    const commands = [];
    const sourceCommit = "a".repeat(40);
    const sourceTree = await portableContentHash(sourceRoot);
    const runner = {
      async run(request) {
        commands.push(request);
        if (request.executable === "git" && request.args.includes("rev-parse"))
          return { code: 0, stdout: `${sourceCommit}\n`, stderr: "" };
        if (request.executable === "git" && request.args.includes("status"))
          return { code: 0, stdout: "", stderr: "" };
        if (request.args.includes("--version"))
          return { code: 0, stdout: "v24.18.0\n", stderr: "" };
        if (request.args.includes("process.arch")) return { code: 0, stdout: "x64\n", stderr: "" };
        if (request.args.includes("metadata"))
          return {
            code: 0,
            stdout: JSON.stringify({
              packages: [
                {
                  id: "path+zap-cli",
                  name: "zap-cli",
                  version: "1.0.0",
                  license: "UPL-1.0",
                  license_file: join(engineRoot, "LICENSE.md"),
                  manifest_path: join(engineRoot, "Cargo.toml"),
                  source: null,
                },
                {
                  id: "registry+serde",
                  name: "serde",
                  version: "1.0.228",
                  license: "MIT OR Apache-2.0",
                  license_file: null,
                  manifest_path: join(engineRoot, "fixture-deps", "serde", "Cargo.toml"),
                  source: "registry",
                },
              ],
            }),
            stderr: "",
          };
        if (request.args.includes("build") && request.executable === "cargo") {
          const binary = join(
            request.environment.CARGO_TARGET_DIR,
            "x86_64-pc-windows-msvc",
            "release",
            "zap.exe",
          );
          await mkdir(dirname(binary), { recursive: true });
          await writeFile(binary, "fixture zap binary\n");
          return { code: 0, stdout: "", stderr: "" };
        }
        if (request.args.includes("run") && request.args.includes("build")) {
          await writeRuntimeEntry(request.cwd, "dist/zap-quick-lens.js");
          await writeRuntimeEntry(request.cwd, "dist/zap-server.js");
          return { code: 0, stdout: "", stderr: "" };
        }
        if (request.args.includes("ci") && request.args.includes("--omit=dev")) {
          await writeRuntimeEntry(request.cwd, "node_modules/electron/dist/electron.exe");
          await writeRuntimeEntry(request.cwd, "node_modules/electron/package.json");
          await writeRuntimeEntry(request.cwd, "node_modules/node-pty/package.json");
          await writeRuntimeEntry(request.cwd, "node_modules/node-pty/build/Release/pty.node");
          return { code: 0, stdout: "", stderr: "" };
        }
        return { code: 0, stdout: "", stderr: "" };
      },
    };
    const result = await buildWindowsDistribution(
      {
        sourceRoot,
        lensRoot,
        engineRoot,
        nodeRoot,
        outputDirectory,
        sourceCommit,
        sourceTree,
        cargoExecutable: "cargo",
        offline: true,
      },
      {
        runner,
        async sourceSnapshot() {
          return { root: sourceRoot, cleanup: async () => {} };
        },
        async zip(_source, destination) {
          await writeFile(destination, "fixture zip bytes\n");
        },
      },
    );
    assert.equal(result.descriptor.management.entry, "management/launch.cmd");
    assert.equal(
      result.descriptor.files.some((file) => file.path.endsWith("pty.node")),
      true,
    );
    assert.equal(
      result.descriptor.files.some((file) => /licenses\/rust\/.+\/LICENSE/u.test(file.path)),
      true,
    );
    const notices = JSON.parse(
      await readFile(join(result.directory, "licenses", "THIRD-PARTY-NOTICES.json"), "utf8"),
    );
    assert.equal(
      notices.rust.every((entry) => entry.licenseFiles.length > 0),
      true,
    );
    const publicLauncher = await readFile(
      join(result.directory, "launchers", "zap-quicklens.cmd"),
      "utf8",
    );
    assert.match(publicLauncher, /%ZAP_OPT_ROOT%\\apps\\zap\\management\\launch\.cmd/u);
    assert.equal(publicLauncher.includes("payload\\node"), false);
    const management = await readFile(join(result.directory, "management", "launch.cmd"), "utf8");
    assert.match(management, /%ZAP_GENERATION_ROOT%\\payload\\node\\node\.exe/u);
    assert.equal(
      commands.some((request) => request.args.includes("--omit=dev")),
      true,
    );
    assert.equal(
      commands
        .filter((request) => request.args.includes("ci") || request.args.includes("run"))
        .every((request) => request.environment.PATH.startsWith(`${nodeRoot};`)),
      true,
    );
    assert.equal(
      commands.some((request) => request.args.includes("--target")),
      true,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

async function fixtureSources(lensRoot, engineRoot, nodeRoot) {
  for (const directory of [lensRoot, engineRoot, nodeRoot])
    await mkdir(directory, { recursive: true });
  await writeFile(
    join(lensRoot, "package.json"),
    JSON.stringify({ name: "@org.vibevm.zap/lens", version: "1.0.0" }),
  );
  await writeFile(
    join(lensRoot, "package-lock.json"),
    JSON.stringify({
      name: "@org.vibevm.zap/lens",
      version: "1.0.0",
      packages: {
        "": { name: "@org.vibevm.zap/lens", version: "1.0.0" },
        "node_modules/electron": { version: "44.3.0", license: "MIT" },
        "node_modules/node-pty": { version: "1.1.0", license: "MIT" },
      },
    }),
  );
  for (const [root, name] of [
    [lensRoot, "LICENSE.md"],
    [lensRoot, "README.md"],
    [engineRoot, "LICENSE.md"],
    [nodeRoot, "LICENSE"],
    [nodeRoot, "node.exe"],
    [nodeRoot, "node_modules/npm/bin/npm-cli.js"],
    [engineRoot, "Cargo.toml"],
    [engineRoot, "fixture-deps/serde/Cargo.toml"],
    [engineRoot, "fixture-deps/serde/LICENSE-MIT"],
  ])
    await writeRuntimeEntry(root, name);
}

async function writeRuntimeEntry(root, relativePath) {
  const path = join(root, ...relativePath.split("/"));
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${relativePath}\n`);
}
