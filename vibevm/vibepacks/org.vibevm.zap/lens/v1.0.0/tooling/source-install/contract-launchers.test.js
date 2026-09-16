/** Source-install contract and launcher proofs. @scope spec://org.vibevm.zap/lens/PROP-016#verification */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { isAbsolute } from "node:path";
import test from "node:test";
import { sourceInstallReply } from "./build.mjs";
import {
  BUILD_ROOT_RELATIVE,
  HOST_MARKER_RELATIVE,
  SOURCE_INSTALL_ID,
  SOURCE_INSTALL_PROTOCOL,
  parseContext,
  parseMarker,
} from "./contract.mjs";
import {
  PACKAGE_COMMANDS,
  launcherFileNames,
  renderLaunchers,
  validatePackageCommands,
} from "./launchers.mjs";

const packageDocument = JSON.parse(
  readFileSync(new URL("../../package.json", import.meta.url), "utf8"),
);

test("source-install marker and lifecycle context retain exact package identity", () => {
  const marker = parseMarker({
    protocol: SOURCE_INSTALL_PROTOCOL,
    installationId: SOURCE_INSTALL_ID,
    settingsDir: absolute("Settings Root"),
    registryDir: absolute("Source Registry"),
    npmRegistry: "https://registry.example.test/npm",
    offline: true,
    lens: { group: "org.vibevm.zap", name: "lens", version: "1.0.0" },
    engine: {
      enabled: true,
      binary: "zap",
      vibeExecutable: absolute("Tools/Vibe VM/vibe.exe"),
    },
  });
  assert.equal(marker.protocol, "zap-source-install/1");
  assert.equal(marker.installationId, "org.vibevm.zap.source-install");
  assert.equal(marker.offline, true);
  assert.equal(marker.npmRegistry, "https://registry.example.test/npm");
  assert.equal(parseMarker({ ...marker, npmRegistry: null }).npmRegistry, null);
  assert.equal(
    parseMarker({ ...marker, npmRegistry: "HTTPS://Registry.Example.Test/npm" }).npmRegistry,
    "https://registry.example.test/npm",
  );
  assert.equal(HOST_MARKER_RELATIVE, ".vibe/zap-source-install.json");
  assert.equal(BUILD_ROOT_RELATIVE, "target/zap-source-install");
  const context = parseContext({
    envelope: 1,
    project: { root: absolute("Settings Root/opt/apps/zap") },
    world: {
      packages: [
        {
          group: "org.vibevm.zap",
          name: "lens",
          version: "1.0.0",
          slot: absolute("Settings Root/opt/apps/zap/vibevm/vibedeps/lens/1.0.0"),
        },
      ],
    },
  });
  assert.equal(context.world.packages[0]?.name, "lens");
  assert.throws(
    () => parseMarker({ ...marker, installationId: "unrelated.application" }),
    /marker identity is invalid/,
  );
  assert.throws(
    () => parseMarker({ ...marker, npmRegistry: "https://token@example.test/npm" }),
    /credential-free HTTP\(S\)/,
  );
  assert.throws(
    () => parseMarker({ ...marker, npmRegistry: "https://example.test/npm?token=private" }),
    /without a query or fragment/,
  );
  assert.throws(
    () => parseMarker({ ...marker, npmRegistry: "https://example.test/npm#private" }),
    /without a query or fragment/,
  );
});

test("launchers cover every package bin and preserve paths and arguments", () => {
  validatePackageCommands(packageDocument);
  assert.equal(PACKAGE_COMMANDS.length, 10);
  assert.equal(launcherFileNames(false).length, 30);
  assert.equal(launcherFileNames(true).length, 33);
  assert.equal(packageDocument.bin["zap-quicklens"], packageDocument.bin["zap-quick-lens"]);
  assert.equal(packageDocument.bin["zap-server"], "./dist/zap-server.js");
  const launchers = renderLaunchers({
    runtimeRoot: absolute(
      "Settings Root/opt/apps/zap/target/zap-source-install/generations/a b/runtime",
    ),
    nativeNode: absolute("Node Runtime/node.exe"),
    enginePath: absolute("Settings Root/opt/apps/zap/generated engine/zap.exe"),
  });
  assert.equal(launchers.length, 33);
  const powershell = launchers.find(
    (launcher) =>
      launcher.command === "zap-quick-lens" && launcher.platform === "windows-powershell",
  );
  const posix = launchers.find(
    (launcher) => launcher.command === "zap-quick-lens" && launcher.platform === "posix",
  );
  const cmd = launchers.find(
    (launcher) => launcher.command === "zap-quick-lens" && launcher.platform === "windows-cmd",
  );
  assert.match(powershell?.body ?? "", /'[^']*Node Runtime[^']*node\.exe'/);
  assert.match(powershell?.body ?? "", /@args/);
  assert.match(posix?.body ?? "", /exec '[^']*Node Runtime[^']*node\.exe'.*"\$@"/);
  assert.match(cmd?.body ?? "", /%~dp0zap-quick-lens\.ps1" %\*/);
  assert.equal(launchers.find((launcher) => launcher.command === "zap")?.mode, 0o644);
});

test("lifecycle reply exposes exactly two absolute forward-slashed artifacts", () => {
  const reply = sourceInstallReply(absolute("Settings Root/opt/apps/zap-ёж"), {
    reused: false,
    generation: { generationId: "a".repeat(64) },
  });
  assert.equal(reply.artifacts.length, 2);
  assert.deepEqual(
    reply.artifacts.map((artifact) => artifact.id),
    ["zap-source-install-prepared", "zap-source-install-launchers"],
  );
  for (const artifact of reply.artifacts) {
    assert.equal(isAbsolute(artifact.path), true);
    assert.doesNotMatch(artifact.path, /\\/u);
    assert.match(artifact.path, /target\/zap-source-install/u);
  }
});

function absolute(suffix) {
  return process.platform === "win32" ? `C:/${suffix}` : `/tmp/${suffix}`;
}
