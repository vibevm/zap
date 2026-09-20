/** Release-index matrix proof. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createIndex } from "./create-index.mjs";

test("release index requires both Linux ABIs and both native macOS architectures", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap distribution index "));
  try {
    const targets = [
      ["windows", "x86_64", undefined, "windows-x64"],
      ["linux", "x86_64", "musl", "linux-x64-musl"],
      ["linux", "x86_64", "gnu", "linux-x64-gnu"],
      ["macos", "x86_64", undefined, "macos-x64"],
      ["macos", "aarch64", undefined, "macos-arm64"],
    ];
    const receipts = [];
    for (const [os, arch, libc, slug] of targets) {
      const path = join(root, `${slug}.build.json`);
      await writeFile(
        path,
        JSON.stringify({
          protocol: "zap-distribution-build/1",
          application: "org.vibevm.zap/zap@1.0.0",
          target: { os, arch, ...(libc === undefined ? {} : { libc }), format: "zip" },
          source: {
            repository: "https://github.com/vibevm/zap.git",
            commit: "a".repeat(40),
            tree: `sha256-tree/1:${"b".repeat(64)}`,
          },
          archive: { file: `zap-${slug}-1.0.0.zip`, sha256: "c".repeat(64), size: 42 },
        }),
      );
      receipts.push(path);
    }
    const output = join(root, "DISTRIBUTIONS.json");
    const index = await createIndex(receipts, output);
    assert.equal(index.distributions.length, 5);
    assert.deepEqual(
      index.distributions
        .filter((row) => row.os === "linux")
        .map((row) => row.libc)
        .sort(),
      ["gnu", "musl"],
    );
    assert.deepEqual(JSON.parse(await readFile(output, "utf8")), index);
    await assert.rejects(createIndex(receipts.slice(1), output), /exactly five/u);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
