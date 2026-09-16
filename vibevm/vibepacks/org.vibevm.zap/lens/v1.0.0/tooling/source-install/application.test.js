/** Generic Zap application adapter proofs. @scope spec://org.vibevm.zap/lens/PROP-017#verification */
import assert from "node:assert/strict";
import { lstat, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import test from "node:test";
import {
  APPLICATION_COMMANDS,
  APPLICATION_CONTEXT_PROTOCOL,
  APPLICATION_RESULT_PROTOCOL,
  parseApplicationContext,
  runApplicationAdapter,
} from "./application.mjs";

const scenario = JSON.parse(
  await readFile(new URL("./application.fixture.json", import.meta.url), "utf8"),
);

test("generic install returns the immutable management adapter and exact public commands", async () => {
  const fixture = await createFixture("install");
  try {
    let bootstrapOptions;
    const outcome = await runApplicationAdapter(fixture.environment, {
      bootstrapPorts: fixture.ports,
      executeBootstrap(options) {
        bootstrapOptions = options;
        return Promise.resolve({
          ok: true,
          code: 0,
          operation: "install",
          state: "ready",
          installerRoot: fixture.hostRoot,
          message: "fixture ready",
          commands: [],
        });
      },
      inspectBootstrap: () => Promise.resolve(fixture.inspection),
    });
    assert.equal(outcome.ok, true);
    const reply = JSON.parse(await readFile(fixture.replyPath, "utf8"));
    assert.equal(reply.protocol, APPLICATION_RESULT_PROTOCOL);
    assert.equal(reply.operation, "install");
    assert.equal(reply.applicationId, scenario.applicationId);
    assert.equal(reply.status, scenario.expected.installStatus);
    assert.deepEqual(reply.commands, scenario.commands);
    assert.equal(reply.launchers.length, 4);
    assert.equal(
      reply.launchers.every(
        (launcher) =>
          dirname(launcher.destination) === join(fixture.settingsRoot, "opt", "bin") &&
          /^[a-f0-9]{64}$/u.test(launcher.sha256),
      ),
      true,
    );
    assert.equal(reply.management.runtime, "node");
    assert.equal(reply.management.entry, await realpath(fixture.managementEntry));
    assert.equal(bootstrapOptions.registry, fixture.registryRoot);
    assert.equal(bootstrapOptions.npmRegistry, scenario.expected.npmRegistry);
    assert.equal(bootstrapOptions.offline, true);
    assert.equal(bootstrapOptions.vibe, fixture.vibeExecutable);
    assert.equal(bootstrapOptions.env.PATH, "fixture-native-path");
    assert.equal(bootstrapOptions.env.SystemRoot, "fixture-system-root");
    assert.equal(bootstrapOptions.env.VIBE_SETTINGS, fixture.settingsRoot);
  } finally {
    await fixture.close();
  }
});

test("retained uninstall needs no registry and returns no management entry", async () => {
  const fixture = await createFixture("uninstall");
  try {
    let bootstrapOptions;
    const outcome = await runApplicationAdapter(fixture.environment, {
      bootstrapPorts: fixture.ports,
      executeBootstrap(options) {
        bootstrapOptions = options;
        return Promise.resolve({
          ok: true,
          code: 0,
          operation: "uninstall",
          state: "undeployed",
          installerRoot: fixture.hostRoot,
          message: "fixture undeployed",
          commands: [],
        });
      },
    });
    assert.equal(outcome.ok, true);
    const reply = JSON.parse(await readFile(fixture.replyPath, "utf8"));
    assert.equal(reply.status, scenario.expected.uninstallStatus);
    assert.equal(reply.management, null);
    assert.deepEqual(reply.launchers, []);
    assert.equal(bootstrapOptions.registry, null);
    assert.equal(bootstrapOptions.npmRegistrySpecified, false);
  } finally {
    await fixture.close();
  }
});

test("identity drift and a mutable-slot management path refuse", async () => {
  const fixture = await createFixture("install");
  try {
    assert.throws(
      () =>
        parseApplicationContext({
          ...fixture.context,
          application: { ...fixture.context.application, id: "another-app" },
        }),
      /application id differs from Zap/,
    );
    const outcome = await runApplicationAdapter(fixture.environment, {
      bootstrapPorts: fixture.ports,
      executeBootstrap: () =>
        Promise.resolve({
          ok: true,
          code: 0,
          operation: "install",
          state: "ready",
          installerRoot: fixture.hostRoot,
          message: "fixture ready",
          commands: [],
        }),
      inspectBootstrap: () =>
        Promise.resolve({
          ...fixture.inspection,
          value: {
            ...fixture.inspection.value,
            generation: { runtimeRoot: "vibevm/vibedeps/org.vibevm.zap.lens/1.0.0" },
          },
        }),
    });
    assert.equal(outcome.ok, false);
    const reply = JSON.parse(await readFile(fixture.replyPath, "utf8"));
    assert.equal(reply.status, "failed");
    assert.equal(reply.management, null);
    assert.deepEqual(reply.launchers, []);
  } finally {
    await fixture.close();
  }
});

async function createFixture(operation) {
  const parent = resolve(tmpdir());
  const root = await mkdtemp(join(parent, "zap application адаптер "));
  assert.equal(root.startsWith(`${parent}${sep}`), true);
  const settingsRoot = join(root, "Settings Root");
  const hostRoot = join(settingsRoot, "opt", "apps", "zap");
  const registryRoot = operation === "uninstall" ? null : join(root, "Source Registry");
  const vibeExecutable = join(root, "Vibe Runtime", "vibe.exe");
  const generationRoot = "target/zap-source-install/generations/" + "a".repeat(64);
  const runtimeRoot = `${generationRoot}/runtime`;
  const managementEntry = join(
    hostRoot,
    ...runtimeRoot.split("/"),
    "tooling",
    "source-install",
    "application.mjs",
  );
  const launcherPaths = scenario.commands.flatMap((command) =>
    [".cmd", ".ps1"].map((suffix) => join(settingsRoot, "opt", "bin", `${command}${suffix}`)),
  );
  await Promise.all([
    mkdir(dirname(managementEntry), { recursive: true }),
    mkdir(dirname(vibeExecutable), { recursive: true }),
    mkdir(join(settingsRoot, "opt", "bin"), { recursive: true }),
    registryRoot === null ? Promise.resolve() : mkdir(registryRoot, { recursive: true }),
  ]);
  await Promise.all([
    writeFile(managementEntry, "// retained management fixture\n"),
    writeFile(vibeExecutable, "fixture vibe\n"),
    ...launcherPaths.map((path) => writeFile(path, `fixture launcher ${path}\n`)),
  ]);
  const context = {
    protocol: APPLICATION_CONTEXT_PROTOCOL,
    operation,
    application: {
      id: scenario.applicationId,
      package: scenario.applicationPackage,
      installerPackage: scenario.installerPackage,
      commands: scenario.commands,
    },
    settingsRoot,
    hostRoot,
    registryRoot,
    vibeExecutable,
    offline: true,
  };
  const contextPath = join(root, "context.json");
  const replyPath = join(root, "reply.json");
  await writeFile(contextPath, JSON.stringify(context));
  return {
    root,
    context,
    contextPath,
    replyPath,
    settingsRoot,
    hostRoot,
    registryRoot,
    vibeExecutable,
    managementEntry,
    environment: {
      VIBE_APPLICATION_CONTEXT: contextPath,
      VIBE_APPLICATION_REPLY: replyPath,
      PATH: "fixture-native-path",
      SystemRoot: "fixture-system-root",
    },
    ports: {
      fs: { lstat, readFile, realpath },
    },
    inspection: {
      ok: true,
      value: { state: "ready", generation: { runtimeRoot }, launcherPaths },
    },
    close: async () => {
      const resolved = resolve(root);
      assert.equal(resolved.startsWith(`${parent}${sep}`), true);
      await rm(resolved, { recursive: true, force: true });
    },
  };
}

assert.deepEqual(APPLICATION_COMMANDS, scenario.commands);
assert.equal(scenario.expected.zeroLlmInference, true);
