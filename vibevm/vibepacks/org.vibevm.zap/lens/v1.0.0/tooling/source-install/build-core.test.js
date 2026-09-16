/** Immutable source-install generation proofs. @scope spec://org.vibevm.zap/lens/PROP-016#verification */
import assert from "node:assert/strict";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import test from "node:test";
import { GENERATION_PROTOCOL, PREPARED_PROTOCOL, buildSourceInstallation } from "./build-core.mjs";
import { PACKAGE_COMMANDS } from "./launchers.mjs";
import { MANAGEMENT_TOOL_FILES } from "./management-tools.mjs";

test("build stages an immutable Lens generation and reuses exact inputs", async () => {
  const fixture = createFixture();
  try {
    const runner = fakeRunner(fixture.lensRoot);
    const first = await buildSourceInstallation(fixture.input, {
      nativeNode: process.execPath,
      npmCli: fixture.npmCli,
      runner,
    });
    assert.equal(first.reused, false);
    assert.equal(first.generation.protocol, GENERATION_PROTOCOL);
    assert.equal(first.prepared.protocol, PREPARED_PROTOCOL);
    assert.equal(first.prepared.engineEnabled, false);
    assert.equal(first.prepared.launchers.length, 30);
    assert.equal(runner.commands.length, 3);
    assert.equal(
      runner.commands
        .filter((command) => command.executable === process.execPath)
        .every((command) => {
          const registry = command.args.indexOf("--registry");
          return (
            registry > 0 &&
            command.args[registry + 1] === "https://registry.example.test/npm" &&
            command.environment.NPM_CONFIG_REGISTRY === "https://registry.example.test/npm" &&
            Object.keys(command.environment)
              .filter((key) => key.toLowerCase() === "npm_config_registry")
              .join(",") === "NPM_CONFIG_REGISTRY"
          );
        }),
      true,
    );
    if (process.platform === "win32")
      assert.match(runner.commands[0]?.environment.PATHEXT ?? "", /\.EXE.*\.CMD/u);
    assert.ok(
      runner.commands.every(
        (command) => command.executable === process.execPath && Array.isArray(command.args),
      ),
    );
    assert.equal(
      runner.commands.some((command) => command.cwd.includes("Source Lens")),
      true,
    );
    const secondRunner = fakeRunner(fixture.lensRoot);
    const second = await buildSourceInstallation(fixture.input, {
      nativeNode: process.execPath,
      npmCli: fixture.npmCli,
      runner: secondRunner,
    });
    assert.equal(second.reused, true);
    assert.equal(second.generation.generationId, first.generation.generationId);
    assert.equal(secondRunner.commands.length, 0);
    assert.ok(existsSync(join(fixture.hostRoot, first.generation.generationRoot)));
    assert.equal(
      readFileSync(
        join(fixture.hostRoot, first.generation.runtimeRoot, "vibevm", "vibespecs", "PROP-016.xml"),
        "utf8",
      ),
      "fixture source-install guide\n",
    );
    assert.equal(
      readFileSync(
        join(
          fixture.hostRoot,
          first.generation.runtimeRoot,
          "node_modules",
          "electron",
          "index.js",
        ),
        "utf8",
      ),
      "// production electron package\n",
    );
    assert.equal(
      readFileSync(
        join(
          fixture.hostRoot,
          first.generation.runtimeRoot,
          "tooling",
          "source-install",
          "application.mjs",
        ),
        "utf8",
      ),
      "// application.mjs management fixture\n",
    );
  } finally {
    fixture.close();
  }
});

test("failed changed build preserves the prior prepared generation", async () => {
  const fixture = createFixture();
  try {
    const first = await buildSourceInstallation(fixture.input, {
      nativeNode: process.execPath,
      npmCli: fixture.npmCli,
      runner: fakeRunner(fixture.lensRoot),
    });
    const preparedPath = join(fixture.hostRoot, "target", "zap-source-install", "prepared.json");
    const before = readFileSync(preparedPath, "utf8");
    writeFileSync(join(fixture.lensRoot, "src", "changed.ts"), "export const changed = true;\n");
    const failing = {
      commands: [],
      run(command) {
        this.commands.push(command);
        return Promise.resolve({
          code: 19,
          signal: null,
          stdout: "",
          stderr: "fixture build failed",
        });
      },
    };
    await assert.rejects(
      () =>
        buildSourceInstallation(fixture.input, {
          nativeNode: process.execPath,
          npmCli: fixture.npmCli,
          runner: failing,
        }),
      /locked Lens dependency installation failed/,
    );
    assert.equal(readFileSync(preparedPath, "utf8"), before);
    assert.ok(existsSync(join(fixture.hostRoot, first.generation.generationRoot)));
    assert.equal(failing.commands.length, 1);
  } finally {
    fixture.close();
  }
});

test("default build uses Vibe binary dispatch and publishes the engine launcher", async () => {
  const fixture = createFixture();
  try {
    const engineRoot = join(fixture.root, "Engine Source");
    const enginePath = join(engineRoot, "target", "zap.exe");
    const vibeExecutable = join(fixture.root, "Vibe Runtime", "vibe.exe");
    mkdirSync(dirname(enginePath), { recursive: true });
    mkdirSync(dirname(vibeExecutable), { recursive: true });
    writeFileSync(join(engineRoot, "Cargo.toml"), "[package]\nname='zap'\nversion='1.0.0'\n");
    writeFileSync(enginePath, "fixture engine\n");
    writeFileSync(vibeExecutable, "fixture vibe\n");
    chmodSync(enginePath, 0o755);
    chmodSync(vibeExecutable, 0o755);
    const runner = fakeRunner(fixture.lensRoot, enginePath);
    const built = await buildSourceInstallation(
      {
        ...fixture.input,
        engine: { slot: engineRoot, version: "1.0.0" },
        config: {
          ...fixture.input.config,
          engine: { enabled: true, binary: "zap", vibeExecutable },
        },
      },
      { nativeNode: process.execPath, npmCli: fixture.npmCli, runner },
    );
    assert.equal(built.prepared.engineEnabled, true);
    assert.equal(built.prepared.engineVersion, "1.0.0");
    assert.equal(built.prepared.launchers.length, 33);
    assert.equal(
      runner.commands.some(
        (command) =>
          command.executable === vibeExecutable &&
          command.args.join(" ") === "--offline bin build zap --assume-yes",
      ),
      true,
    );
    assert.equal(
      runner.commands.some(
        (command) =>
          command.executable === vibeExecutable && command.args.join(" ") === "bin path zap",
      ),
      true,
    );
    if (process.platform === "win32") {
      const engineBuild = runner.commands.find(
        (command) => command.executable === vibeExecutable && command.args.includes("build"),
      );
      assert.match(engineBuild?.environment.ProgramFiles ?? "", /Program Files$/u);
      assert.match(engineBuild?.environment["ProgramFiles(x86)"] ?? "", /Program Files \(x86\)$/u);
    }
  } finally {
    fixture.close();
  }
});

function createFixture() {
  const root = mkdtempSync(join(tmpdir(), "zap source install "));
  const hostRoot = join(root, "Settings Root", "opt", "apps", "zap");
  const lensRoot = join(root, "Source Lens");
  const npmCli = join(root, "Node Runtime", "npm-cli.js");
  for (const directory of [
    hostRoot,
    join(lensRoot, "src"),
    join(lensRoot, "integrations"),
    join(lensRoot, "tooling", "source-install"),
    join(lensRoot, "vibevm", "vibespecs"),
    join(root, "Node Runtime"),
  ])
    mkdirSync(directory, { recursive: true });
  const packageDocument = JSON.parse(
    readFileSync(new URL("../../package.json", import.meta.url), "utf8"),
  );
  writeFileSync(join(lensRoot, "package.json"), JSON.stringify(packageDocument));
  writeFileSync(join(lensRoot, "package-lock.json"), "{}\n");
  writeFileSync(join(lensRoot, "LICENSE.md"), "fixture license\n");
  writeFileSync(join(lensRoot, "README.md"), "fixture readme\n");
  writeFileSync(join(lensRoot, "vibe.toml"), "[package]\nname='lens'\n");
  writeFileSync(join(lensRoot, "src", "index.ts"), "export const fixture = true;\n");
  writeFileSync(join(lensRoot, "integrations", "README.md"), "fixture integration\n");
  writeFileSync(
    join(lensRoot, "vibevm", "vibespecs", "PROP-016.xml"),
    "fixture source-install guide\n",
  );
  for (const file of MANAGEMENT_TOOL_FILES)
    writeFileSync(
      join(lensRoot, "tooling", "source-install", file),
      `// ${file} management fixture\n`,
    );
  writeFileSync(npmCli, "// fixture npm cli\n");
  return {
    root,
    hostRoot,
    lensRoot,
    npmCli,
    input: {
      projectRoot: hostRoot,
      lensRoot,
      engine: null,
      config: {
        offline: true,
        npmRegistry: "https://registry.example.test/npm",
        lens: { group: "org.vibevm.zap", name: "lens", version: "1.0.0" },
        engine: { enabled: false, binary: "zap", vibeExecutable: join(root, "vibe") },
      },
    },
    close: () => rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 }),
  };
}

function fakeRunner(lensRoot, enginePath = null) {
  return {
    commands: [],
    async run(command) {
      this.commands.push(command);
      if (command.cwd === lensRoot && command.args.includes("ci")) {
        const electronRoot = join(lensRoot, "node_modules", "electron");
        const electron = join(
          electronRoot,
          "dist",
          process.platform === "win32" ? "electron.exe" : "electron",
        );
        mkdirSync(dirname(electron), { recursive: true });
        writeFileSync(electron, "fixture electron\n");
        chmodSync(electron, 0o755);
        writeFileSync(join(electronRoot, "index.js"), "// source electron package\n");
        writeFileSync(join(electronRoot, "package.json"), '{"name":"electron"}\n');
        writeFileSync(
          join(electronRoot, "path.txt"),
          process.platform === "win32" ? "electron.exe\n" : "electron\n",
        );
      }
      if (command.cwd === lensRoot && command.args.includes("build")) {
        const dist = join(lensRoot, "dist");
        mkdirSync(dist, { recursive: true });
        for (const [, entry] of PACKAGE_COMMANDS) {
          const target = join(lensRoot, entry);
          mkdirSync(dirname(target), { recursive: true });
          writeFileSync(target, `// ${basename(entry)} fixture\n`);
        }
        for (const entry of [
          join("dist", "quicklens", "browser", "index.html"),
          join("dist", "quicklens", "electron", "main.js"),
        ]) {
          const target = join(lensRoot, entry);
          mkdirSync(dirname(target), { recursive: true });
          writeFileSync(target, "fixture runtime\n");
        }
      }
      if (command.cwd !== lensRoot && command.args.includes("--omit=dev")) {
        const electronRoot = join(command.cwd, "node_modules", "electron");
        mkdirSync(electronRoot, { recursive: true });
        writeFileSync(join(electronRoot, "index.js"), "// production electron package\n");
        writeFileSync(join(electronRoot, "package.json"), '{"name":"electron"}\n');
      }
      return {
        code: 0,
        signal: null,
        stdout: command.args[0] === "bin" && command.args[1] === "path" ? `${enginePath}\n` : "",
        stderr: "",
      };
    },
  };
}
