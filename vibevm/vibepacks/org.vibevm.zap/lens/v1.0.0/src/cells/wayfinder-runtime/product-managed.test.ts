/** Managed worker network inheritance. @scope spec://org.vibevm.zap/lens/PROP-010#agent-network */
import assert from "node:assert/strict";
import test from "node:test";
import { ProxyPolicySchema } from "../proxy-policy/index.ts";
import { effectiveManagedProxy } from "./product-managed.ts";

test("managed Codex workers inherit the Codex-specific proxy before global policy", () => {
  const global = ProxyPolicySchema.parse({
    mode: "explicit",
    httpsProxy: "http://global.example:8080",
  });
  assert.deepEqual(effectiveManagedProxy({ proxy: { mode: "direct" } }, undefined, global), {
    mode: "direct",
  });
  assert.deepEqual(effectiveManagedProxy({}, undefined, global), global);
  assert.deepEqual(effectiveManagedProxy(undefined, { proxy: { mode: "direct" } }, global), {
    mode: "direct",
  });
});
