/** Read-only source installer CLI proof. @scope spec://org.vibevm.zap/lens/PROP-016#commands */
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("source installer help exits without creating its selected settings directory", () => {
  const root = mkdtempSync(join(tmpdir(), "zap source help "));
  const settingsDir = join(root, "Never Created", ".vibe");
  try {
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../install-source.mjs", import.meta.url)),
        "--help",
        "--settings-dir",
        settingsDir,
      ],
      { encoding: "utf8", windowsHide: true },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /install\|update\|status\|uninstall/);
    assert.match(result.stdout, /--lens-only/);
    assert.equal(existsSync(settingsDir), false);
  } finally {
    rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});
