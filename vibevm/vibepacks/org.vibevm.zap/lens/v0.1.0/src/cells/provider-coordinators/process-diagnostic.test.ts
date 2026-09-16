import assert from "node:assert/strict";
import test from "node:test";
import { providerStderrDiagnostic } from "./process-diagnostic.ts";

test("provider stderr diagnostics expose bounded metadata without raw stderr", () => {
  const raw = "proxy connection timed out token=synthetic-secret";
  const diagnostic = providerStderrDiagnostic(raw);

  assert.equal(diagnostic.category, "network");
  assert.equal(diagnostic.bytes, Buffer.byteLength(raw));
  assert.match(diagnostic.digest, /^[0-9a-f]{64}$/u);
  assert.doesNotMatch(JSON.stringify(diagnostic), /synthetic-secret/u);

  const bounded = providerStderrDiagnostic(`API failure ${"x".repeat(70_000)}`);
  assert.equal(bounded.category, "api");
  assert.equal(bounded.bytes, 65_536);
  assert.equal(bounded.truncated, true);
});
