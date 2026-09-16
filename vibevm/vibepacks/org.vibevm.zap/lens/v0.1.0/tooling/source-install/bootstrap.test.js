/** Source-install bootstrap lifecycle proofs. @scope spec://org.vibevm.zap/lens/PROP-016#verification */
import assert from "node:assert/strict";
import {
  access,
  copyFile,
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  executeBootstrap,
  inspectBootstrap,
  parseBootstrapArgs,
  renderHostFiles,
  resolveBootstrapPlan,
} from "./bootstrap.mjs";

const COMMANDS = [
  "codlens",
  "codlens-mcp",
  "quicklens-service",
  "quicklens-web-auth",
  "quicklens-web",
  "zap-wayfinder",
  "zap-quick-lens",
  "zap-mock-agent",
  "zap",
];

test("bootstrap preserves structured paths, becomes ready and keeps status read-only", async () => {
  const fixture = createFixture();
  try {
    const parsed = parseBootstrapArgs(
      [
        "install",
        "--registry",
        fixture.registryDir,
        "--npm-registry",
        "HTTPS://Registry.Example.Test/npm",
        "--settings-dir",
        fixture.settingsDir,
        "--vibe",
        fixture.vibeExecutable,
        "--offline",
      ],
      {},
    );
    assert.equal(parsed.ok, true);
    if (!parsed.ok) return;
    const planned = await resolveBootstrapPlan(parsed.value, fixture.ports);
    assert.equal(planned.ok, true, planned.ok ? undefined : planned.error.message);
    if (!planned.ok) return;
    assert.equal(planned.value.settingsDir, fixture.settingsDir);
    assert.equal(planned.value.registryDir, fixture.registryDir);
    assert.equal(planned.value.npmRegistry, "https://registry.example.test/npm");
    assert.equal(planned.value.vibeExecutable, fixture.vibeExecutable);
    const hostFiles = renderHostFiles(planned.value);
    assert.equal(hostFiles.length, 4);
    const manifest = hostFiles.find((file) => file.relative === "vibe.toml")?.content ?? "";
    for (const input of [
      ".vibe/zap-source-install.json",
      "hooks/**",
      "/LICENSE.md",
      "/README.md",
      "/vibe.toml",
      "/tooling/**",
      "/docs/**",
      "/vibevm/vibespecs/**",
      "/Cargo.lock",
    ])
      assert.equal(manifest.includes(input), true, `missing lifecycle input ${input}`);
    const installed = await executeBootstrap(parsed.value, fixture.ports);
    assert.equal(installed.ok, true, installed.message);
    assert.equal(installed.state, "ready");
    assert.equal(installed.commands.length, 3);
    assert.deepEqual(
      fixture.processCalls.map((call) => call.file),
      [fixture.vibeExecutable, fixture.vibeExecutable, fixture.vibeExecutable],
    );
    assert.equal(
      fixture.processCalls.every(
        (call) =>
          Array.isArray(call.args) &&
          call.options.cwd === planned.value.installerRoot &&
          call.options.env.VIBE_SETTINGS === fixture.settingsDir,
      ),
      true,
      JSON.stringify(fixture.processCalls),
    );
    const writes = fixture.mutations();
    const calls = fixture.processCalls.length;
    const status = await executeBootstrap({ ...parsed.value, command: "status" }, fixture.ports);
    assert.equal(status.ok, true);
    assert.equal(status.state, "ready");
    assert.equal(fixture.processCalls.length, calls);
    assert.equal(fixture.mutations(), writes);
  } finally {
    fixture.close();
  }
});

test("ownership refusal and build failure cannot replace a ready installation", async () => {
  const unrelated = createFixture();
  try {
    const root = join(unrelated.settingsDir, "opt", "apps", "zap");
    mkdirSync(root, { recursive: true });
    writeFileSync(join(root, "do-not-adopt.txt"), "unrelated\n");
    const refused = await executeBootstrap(unrelated.options("install"), unrelated.ports);
    assert.equal(refused.ok, false);
    assert.match(refused.message, /nonempty|ownership marker/);
    assert.equal(unrelated.processCalls.length, 0);
  } finally {
    unrelated.close();
  }

  const mismatched = createFixture();
  try {
    const root = join(mismatched.settingsDir, "opt", "apps", "zap");
    mkdirSync(join(root, ".vibe"), { recursive: true });
    writeFileSync(
      join(root, ".vibe", "zap-source-install.json"),
      JSON.stringify({ protocol: "another-app/1", installationId: "unrelated.application" }),
    );
    const refused = await executeBootstrap(mismatched.options("install"), mismatched.ports);
    assert.equal(refused.ok, false);
    assert.match(refused.message, /marker is invalid|missing or unknown|unsupported shape/);
    assert.equal(mismatched.processCalls.length, 0);
  } finally {
    mismatched.close();
  }

  const fixture = createFixture();
  try {
    const installed = await executeBootstrap(
      {
        ...fixture.options("install"),
        npmRegistry: "HTTPS://Registry.Example.Test/npm",
        npmRegistrySpecified: true,
      },
      fixture.ports,
    );
    assert.equal(installed.ok, true);
    const preparedPath = join(
      fixture.settingsDir,
      "opt",
      "apps",
      "zap",
      "target",
      "zap-source-install",
      "prepared.json",
    );
    const before = await readFile(preparedPath, "utf8");
    fixture.failOperation.value = "build";
    const failed = await executeBootstrap(fixture.options("update"), fixture.ports);
    assert.equal(failed.ok, false);
    assert.equal(failed.state, "failed");
    assert.notEqual(failed.message, "Zap source installation is ready");
    assert.equal(await readFile(preparedPath, "utf8"), before);
    fixture.failOperation.value = null;
    const updated = await executeBootstrap(fixture.options("update"), fixture.ports);
    assert.equal(updated.ok, true, updated.message);
    assert.equal(updated.state, "ready");
    const marker = JSON.parse(
      await readFile(
        join(fixture.settingsDir, "opt", "apps", "zap", ".vibe", "zap-source-install.json"),
        "utf8",
      ),
    );
    assert.equal(marker.npmRegistry, "https://registry.example.test/npm");
  } finally {
    fixture.close();
  }
});

test("successful Vibe build without prepared evidence refuses before deploy", async () => {
  const fixture = createFixture();
  try {
    fixture.skipPrepared.value = true;
    const result = await executeBootstrap(fixture.options("install"), fixture.ports);
    assert.equal(result.ok, false);
    assert.match(result.message, /valid prepared Zap runtime generation/);
    assert.deepEqual(
      fixture.processCalls.map((call) => call.operation),
      ["install", "build"],
    );
  } finally {
    fixture.close();
  }
});

test("uninstall removes receipt launchers and retains cache and user state", async () => {
  const fixture = createFixture();
  try {
    const installed = await executeBootstrap(fixture.options("install"), fixture.ports);
    assert.equal(installed.ok, true);
    const host = join(fixture.settingsDir, "opt", "apps", "zap");
    const userState = join(fixture.settingsDir, "zap", "user-state.json");
    mkdirSync(join(fixture.settingsDir, "zap"), { recursive: true });
    writeFileSync(userState, "{}\n");
    const uninstalled = await executeBootstrap(fixture.options("uninstall"), fixture.ports);
    assert.equal(uninstalled.ok, true);
    assert.equal(uninstalled.state, "undeployed");
    assert.equal(existsSync(join(host, ".vibe", "zap-source-install.json")), true);
    assert.equal(existsSync(join(host, "target", "zap-source-install", "prepared.json")), true);
    assert.equal(existsSync(userState), true);
    assert.equal(existsSync(join(fixture.settingsDir, "opt", "bin", "zap.cmd")), false);
    const status = await inspectBootstrap(fixture.options("status"), fixture.ports);
    assert.equal(status.ok, true);
    if (status.ok) assert.equal(status.value.state, "undeployed");
  } finally {
    fixture.close();
  }
});

test("standalone bootstrap without a registry fails before writes or process calls", async () => {
  const fixture = createFixture();
  try {
    const standalone = join(fixture.root, "Standalone", "tooling", "bootstrap.mjs");
    mkdirSync(dirname(standalone), { recursive: true });
    writeFileSync(standalone, "// standalone\n");
    const result = await resolveBootstrapPlan(
      {
        command: "install",
        registry: null,
        settingsDir: fixture.settingsDir,
        vibe: fixture.vibeExecutable,
        offline: false,
        lensOnly: false,
        help: false,
        env: {},
      },
      { ...fixture.ports, modulePath: standalone },
    );
    assert.equal(result.ok, false);
    if (!result.ok) assert.match(result.error.message, /pass --registry/);
    assert.equal(fixture.processCalls.length, 0);
    assert.equal(fixture.mutations(), 0);
  } finally {
    fixture.close();
  }
});

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), "zap bootstrap "));
  const registryDir = join(root, "Source Registry");
  const settingsDir = join(root, "Settings Root", ".vibe");
  const vibeExecutable = join(root, "Tools", "Vibe VM", "vibe.exe");
  for (const directory of [registryDir, settingsDir, dirname(vibeExecutable)])
    mkdirSync(directory, { recursive: true });
  writeFileSync(vibeExecutable, "fixture executable\n");
  createRegistryLensSource(registryDir);
  const processCalls = [];
  const failOperation = { value: null };
  const skipPrepared = { value: false };
  let mutationCount = 0;
  const fs = {
    stat,
    lstat,
    readdir,
    readFile,
    realpath,
    access,
    copyFile: async (...args) => {
      mutationCount += 1;
      return copyFile(...args);
    },
    mkdir: async (...args) => {
      mutationCount += 1;
      return mkdir(...args);
    },
    writeFile: async (...args) => {
      mutationCount += 1;
      return writeFile(...args);
    },
    rename: async (...args) => {
      mutationCount += 1;
      return rename(...args);
    },
    rm: async (...args) => {
      mutationCount += 1;
      return rm(...args);
    },
  };
  const ports = {
    fs,
    process: {
      async run(file, args, options) {
        const operation = args.find((value) =>
          ["install", "build", "deploy", "undeploy"].includes(value),
        );
        processCalls.push({ file, args: [...args], options, operation });
        if (failOperation.value === operation)
          return { code: 17, stdout: "", stderr: "fixture phase failed" };
        if (skipPrepared.value && operation === "build") return { code: 0, stdout: "", stderr: "" };
        await simulateVibe(operation, options.cwd, settingsDir);
        return { code: 0, stdout: "", stderr: "" };
      },
    },
    now: () => new Date("2026-09-16T12:00:00.000Z"),
    platform: "win32",
    homeDir: join(root, "Home User"),
    modulePath: fileURLToPath(new URL("./bootstrap.mjs", import.meta.url)),
    nodeVersion: process.versions.node,
    nodeExecutable: process.execPath,
  };
  return {
    root,
    registryDir,
    settingsDir,
    vibeExecutable,
    processCalls,
    failOperation,
    skipPrepared,
    ports,
    mutations: () => mutationCount,
    options: (command) => ({
      command,
      registry: registryDir,
      npmRegistry: null,
      npmRegistrySpecified: false,
      settingsDir,
      vibe: vibeExecutable,
      offline: true,
      offlineSpecified: true,
      lensOnly: false,
      lensOnlySpecified: false,
      help: false,
      env: {},
    }),
    close: () => rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 }),
  };
}

function createRegistryLensSource(registryDir) {
  const root = join(registryDir, "org.vibevm.zap", "lens", "v0.1.0");
  const files = [
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
    "vitest.config.ts",
  ];
  for (const file of files) {
    const target = join(root, ...file.split("/"));
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, file.endsWith(".json") ? "{}\n" : `// ${file}\n`);
  }
  writeFileSync(
    join(root, "vibe.toml"),
    '[package]\nname = "lens"\ngroup = "org.vibevm.zap"\nversion = "0.1.0"\n',
  );
  for (const directory of ["integrations", "src", "tooling/source-install", "vibevm/vibespecs"]) {
    const target = join(root, ...directory.split("/"), "fixture.txt");
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, `${directory} fixture\n`);
  }
}

async function simulateVibe(operation, installerRoot, settingsDir) {
  if (operation === "build") {
    const prepared = join(installerRoot, "target", "zap-source-install", "prepared.json");
    const generationId = "a".repeat(64);
    const generationRoot = `target/zap-source-install/generations/${generationId}`;
    await mkdir(dirname(prepared), { recursive: true });
    await mkdir(join(installerRoot, ...generationRoot.split("/")), { recursive: true });
    const launchers = COMMANDS.flatMap((command) => [
      {
        command,
        platform: "windows-cmd",
        path: `target/zap-source-install/launchers/${command}.cmd`,
      },
      {
        command,
        platform: "windows-powershell",
        path: `target/zap-source-install/launchers/${command}.ps1`,
      },
      { command, platform: "posix", path: `target/zap-source-install/launchers/${command}` },
    ]);
    const generationLaunchers = launchers.map((launcher) => ({
      ...launcher,
      path: `${generationRoot}/launchers/${launcher.path.split("/").at(-1)}`,
    }));
    await writeFile(
      join(installerRoot, ...generationRoot.split("/"), "generation.json"),
      `${JSON.stringify({
        protocol: "zap-source-install-generation/1",
        generationId,
        generationRoot,
        runtimeRoot: `${generationRoot}/runtime`,
        sourceDigest: "b".repeat(64),
        payloadDigest: "c".repeat(64),
        platform: "win32",
        architecture: "x64",
        nodeVersion: process.versions.node,
        lensVersion: "0.1.0",
        engineVersion: "1.1.0",
        engineEnabled: true,
        launchers: generationLaunchers,
      })}\n`,
    );
    for (const launcher of launchers) {
      const stable = join(installerRoot, ...launcher.path.split("/"));
      const generated = join(
        installerRoot,
        ...generationLaunchers
          .find(
            (candidate) =>
              candidate.command === launcher.command && candidate.platform === launcher.platform,
          )
          .path.split("/"),
      );
      await mkdir(dirname(stable), { recursive: true });
      await mkdir(dirname(generated), { recursive: true });
      await writeFile(stable, "fixture launcher\n");
      await writeFile(generated, "fixture launcher\n");
    }
    await writeFile(
      prepared,
      `${JSON.stringify({
        protocol: "zap-source-install-prepared/1",
        generationId,
        generationRoot,
        launchers,
        engineEnabled: true,
        lensVersion: "0.1.0",
        engineVersion: "1.1.0",
      })}\n`,
    );
  }
  if (operation === "deploy") {
    const bin = join(settingsDir, "opt", "bin");
    await mkdir(bin, { recursive: true });
    for (const command of COMMANDS)
      for (const suffix of [".cmd", ".ps1"])
        await writeFile(join(bin, `${command}${suffix}`), "fixture launcher\n");
  }
  if (operation === "undeploy") {
    const bin = join(settingsDir, "opt", "bin");
    for (const command of COMMANDS)
      for (const suffix of [".cmd", ".ps1"])
        await rm(join(bin, `${command}${suffix}`), { force: true });
  }
}
