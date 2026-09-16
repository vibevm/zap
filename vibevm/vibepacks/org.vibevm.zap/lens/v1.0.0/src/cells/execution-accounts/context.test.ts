import assert from "node:assert/strict";
import test from "node:test";
import { createContextApplicationPort } from "./index.ts";

test("context application refuses unsupported expansion and preserves provider-controlled truth", () => {
  const port = createContextApplicationPort();
  const configured = port.apply({
    request: { mode: "explicit", tokens: 200_000 },
    capability: {
      mode: "configurable",
      allowedTokens: [100_000, 200_000],
      defaultTokens: 100_000,
      documentedMaximumTokens: 200_000,
    },
  });
  assert.deepEqual(configured, { ok: true, value: { state: "configured", tokens: 200_000 } });
  const expansion = port.apply({
    request: { mode: "explicit", tokens: 400_000 },
    capability: {
      mode: "configurable",
      allowedTokens: [100_000, 200_000, 400_000],
      defaultTokens: 100_000,
      documentedMaximumTokens: 200_000,
    },
  });
  assert.equal(expansion.ok, false);
  const unknown = port.apply({
    request: { mode: "default" },
    capability: { mode: "unknown", reason: "installed adapter has no observation" },
  });
  assert.deepEqual(unknown, {
    ok: true,
    value: { state: "unknown", reason: "installed adapter has no observation" },
  });
  const inherited = port.apply({
    request: { mode: "inherit" },
    capability: { mode: "inherited", source: "parent coordinator" },
    inheritedTokens: 128_000,
  });
  assert.deepEqual(inherited, { ok: true, value: { state: "inherited", tokens: 128_000 } });
});
