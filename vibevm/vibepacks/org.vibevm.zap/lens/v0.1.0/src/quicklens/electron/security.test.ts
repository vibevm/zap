import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { resolveRendererAsset } from "./security.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-002#acceptance */
void test("custom protocol serves only existing assets inside its renderer root", () => {
  const root = mkdtempSync(join(tmpdir(), "quicklens-assets-"));
  writeFileSync(join(root, "index.html"), "<!doctype html>");
  assert.equal(resolveRendererAsset(root, "quicklens://app/").ok, true);
  assert.equal(resolveRendererAsset(root, "quicklens://other/").ok, false);
  assert.equal(resolveRendererAsset(root, "quicklens://app/%2e%2e%2fsecret.txt").ok, false);
  assert.equal(resolveRendererAsset(root, "https://app/index.html").ok, false);
});
