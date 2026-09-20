/** Binary distribution manifest proofs. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  DISTRIBUTION_DESCRIPTOR,
  distributionDescriptor,
  parseDescriptor,
  sealDistributionDirectory,
  verifyDistributionDirectory,
} from "./manifest.mjs";

const SOURCE_COMMIT = "a".repeat(40);
const SOURCE_TREE = `sha256-tree/1:${"b".repeat(64)}`;
const COMMANDS = [
  "codlens",
  "codlens-mcp",
  "quicklens-service",
  "quicklens-web-auth",
  "quicklens-web",
  "zap-wayfinder",
  "zap-server",
  "zap-quicklens",
  "zap-quick-lens",
  "zap-mock-agent",
  "zap",
];

test("distribution descriptor exhaustively binds relative payload and standalone launchers", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap distribution manifest "));
  try {
    await fixturePayload(root);
    const descriptor = await sealDistributionDirectory(root, {
      sourceCommit: SOURCE_COMMIT,
      sourceTree: SOURCE_TREE,
      launchers: fixtureLaunchers(),
    });
    assert.deepEqual(descriptor.management, {
      runtime: "builtin",
      entry: "management/launch.cmd",
    });
    assert.equal(
      descriptor.files.some((file) => file.path === DISTRIBUTION_DESCRIPTOR),
      false,
    );
    assert.deepEqual(
      descriptor.files.map((file) => file.path),
      [
        ...COMMANDS.map((command) => `launchers/${command}.cmd`),
        "management/launch.cmd",
        "payload/app/dist/zap-quick-lens.js",
        "payload/app/dist/zap-server.js",
        "payload/bin/zap.exe",
        "payload/node/node.exe",
      ].sort((left, right) => left.localeCompare(right, "en")),
    );
    assert.deepEqual(await verifyDistributionDirectory(root), descriptor);
    const parsed = parseDescriptor(
      JSON.parse(await readFile(join(root, DISTRIBUTION_DESCRIPTOR), "utf8")),
    );
    assert.deepEqual(parsed.application.commands, COMMANDS);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("distribution descriptor refuses traversal, case collisions, links and post-seal drift", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap distribution refusal "));
  try {
    await fixturePayload(root);
    await assert.rejects(
      sealDistributionDirectory(root, {
        sourceCommit: SOURCE_COMMIT,
        sourceTree: SOURCE_TREE,
        launchers: fixtureLaunchers().map((launcher) =>
          launcher.command === "zap-quicklens" ? { ...launcher, path: "../zap.cmd" } : launcher,
        ),
      }),
      /not portable|not relative/u,
    );
    const descriptor = await sealDistributionDirectory(root, {
      sourceCommit: SOURCE_COMMIT,
      sourceTree: SOURCE_TREE,
      launchers: fixtureLaunchers(),
    });
    assert.equal(descriptor.files.length, 16);
    await writeFile(join(root, "payload", "bin", "zap.exe"), "changed\n");
    await assert.rejects(
      verifyDistributionDirectory(root),
      /differ from the exhaustive descriptor/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Linux descriptors require and preserve an explicit libc ABI", () => {
  const files = [
    { path: "management/launch.sh", sha256: "c".repeat(64), size: 1 },
    ...COMMANDS.map((command) => ({
      path: `launchers/${command}`,
      sha256: "d".repeat(64),
      size: 1,
    })),
  ];
  const input = {
    sourceCommit: SOURCE_COMMIT,
    sourceTree: SOURCE_TREE,
    os: "linux",
    arch: "x86_64",
    libc: "gnu",
    launchers: COMMANDS.map((command) => ({
      command,
      path: `launchers/${command}`,
      destination: command,
    })),
    files,
  };
  assert.equal(distributionDescriptor(input).libc, "gnu");
  assert.throws(() => distributionDescriptor({ ...input, libc: undefined }), /unsupported/u);
  assert.equal(parseDescriptor(distributionDescriptor({ ...input, libc: "musl" })).libc, "musl");
});

async function fixturePayload(root) {
  for (const directory of [
    "launchers",
    "management",
    "payload/app/dist",
    "payload/bin",
    "payload/node",
  ])
    await mkdir(join(root, ...directory.split("/")), { recursive: true });
  for (const [path, content] of [
    ...COMMANDS.map((command) => [`launchers/${command}.cmd`, "@echo off\r\n"]),
    ["management/launch.cmd", "@echo off\r\n"],
    ["payload/app/dist/zap-quick-lens.js", "// quick lens\n"],
    ["payload/app/dist/zap-server.js", "// server\n"],
    ["payload/bin/zap.exe", "fixture zap\n"],
    ["payload/node/node.exe", "fixture node\n"],
  ])
    await writeFile(join(root, ...path.split("/")), content);
}

function fixtureLaunchers() {
  return COMMANDS.map((command) => ({
    command,
    path: `launchers/${command}.cmd`,
    destination: `${command}.cmd`,
  }));
}
