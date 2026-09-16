import { expect, test } from "vitest";

import { createQuicklensDemoDataSource } from "../../cells/quicklens-demo/index.ts";
import { createHostBridgeDataSource, type QuicklensHostBridge } from "./host-bridge.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-002#acceptance */
test("browser bridge validates normalized snapshots and rejects malformed platform data", async () => {
  const demo = await createQuicklensDemoDataSource().read({
    signal: new AbortController().signal,
  });
  if (!demo.ok) throw new Error(demo.error.message);
  const validBridge = bridgeReturning({ ok: true, value: demo.value });
  const valid = await createHostBridgeDataSource(validBridge).read({
    signal: new AbortController().signal,
  });
  expect(valid.ok).toBe(true);

  const invalid = await createHostBridgeDataSource(bridgeReturning({ ok: true, value: {} })).read({
    signal: new AbortController().signal,
  });
  expect(invalid).toEqual({
    ok: false,
    error: {
      code: "invalid_data",
      message: "The platform bridge response did not match the Quicklens view model.",
      recovery: "Refresh after updating the backend adapter to the current client contract.",
    },
  });
});

function bridgeReturning(value: unknown): QuicklensHostBridge {
  return {
    read: async () => value,
    answerQuestion: async () => value,
    proposePlanIntent: async () => value,
    previewPlan: async () => value,
    applyPlan: async () => value,
    reconcilePlan: async () => value,
    decidePlan: async () => value,
  };
}
