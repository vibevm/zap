/** Public deterministic Zap mock-model seam. @scope spec://org.vibevm.zap/lens/PROP-013#root */
export { createZapMockModel } from "./model.ts";
export type {
  ZapMockModel,
  ZapMockModel as ZapMockModelPort,
  ZapMockModelErrorCode,
  ZapMockModelResult,
  ZapMockTransition,
} from "./model.ts";
export {
  ZAP_MOCK_MODEL_ID,
  ZapMockEffectSchema,
  ZapMockInputSchema,
  ZapMockScenarioSchema,
  ZapMockScenarioStepSchema,
  ZapMockSnapshotSchema,
  ZapMockStateSchema,
  ZapMockTraceEntrySchema,
} from "./schemas.ts";
export type {
  ZapMockEffect,
  ZapMockInput,
  ZapMockScenario,
  ZapMockScenarioStep,
  ZapMockSnapshot,
  ZapMockState,
  ZapMockTraceEntry,
} from "./schemas.ts";
