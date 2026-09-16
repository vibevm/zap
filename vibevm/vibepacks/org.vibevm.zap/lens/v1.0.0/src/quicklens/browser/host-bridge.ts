/**
 * Browser-side adapter for the narrow Electron preload bridge.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shells
 */
import { z, type ZodType } from "zod";

import {
  PlanOperationResultSchema,
  QuestionViewSchema,
  QuicklensErrorSchema,
  QuicklensSnapshotSchema,
  type InvalidationReason,
  type QuicklensDataSource,
  type QuicklensResult,
} from "../../cells/quicklens-model/index.ts";

const ResultEnvelopeSchema = z.discriminatedUnion("ok", [
  z.object({ ok: z.literal(true), value: z.unknown() }).strict(),
  z.object({ ok: z.literal(false), error: QuicklensErrorSchema }).strict(),
]);

export interface QuicklensHostBridge {
  read(): Promise<unknown>;
  answerQuestion(input: unknown): Promise<unknown>;
  proposePlanIntent(input: unknown): Promise<unknown>;
  previewPlan(input: unknown): Promise<unknown>;
  applyPlan(input: unknown): Promise<unknown>;
  reconcilePlan(input: unknown): Promise<unknown>;
  decidePlan(input: unknown): Promise<unknown>;
  subscribe?(listener: (reason: InvalidationReason) => void): () => void;
}

export function createHostBridgeDataSource(bridge: QuicklensHostBridge): QuicklensDataSource {
  const source: QuicklensDataSource = {
    read: async ({ signal }) =>
      signal.aborted ? cancelled() : parseResult(QuicklensSnapshotSchema, await bridge.read()),
    answerQuestion: async (input) =>
      parseResult(QuestionViewSchema, await bridge.answerQuestion(input)),
    proposePlanIntent: async (input) =>
      parseResult(PlanOperationResultSchema, await bridge.proposePlanIntent(input)),
    previewPlan: async (input) =>
      parseResult(PlanOperationResultSchema, await bridge.previewPlan(input)),
    applyPlan: async (input) =>
      parseResult(PlanOperationResultSchema, await bridge.applyPlan(input)),
    reconcilePlan: async (input) =>
      parseResult(PlanOperationResultSchema, await bridge.reconcilePlan(input)),
    decidePlan: async (input) =>
      parseResult(PlanOperationResultSchema, await bridge.decidePlan(input)),
  };
  return bridge.subscribe === undefined
    ? source
    : { ...source, subscribe: (listener) => bridge.subscribe?.(listener) ?? (() => undefined) };
}

function parseResult<T>(schema: ZodType<T>, candidate: unknown): QuicklensResult<T> {
  const envelope = ResultEnvelopeSchema.safeParse(candidate);
  if (!envelope.success) {
    return {
      ok: false,
      error: {
        code: "invalid_data",
        message: "The platform bridge returned an invalid response.",
        recovery: "Refresh after checking the trusted Quicklens host adapter.",
      },
    };
  }
  if (!envelope.data.ok) return envelope.data;
  const value = schema.safeParse(envelope.data.value);
  return value.success
    ? { ok: true, value: value.data }
    : {
        ok: false,
        error: {
          code: "invalid_data",
          message: "The platform bridge response did not match the Quicklens view model.",
          recovery: "Refresh after updating the backend adapter to the current client contract.",
        },
      };
}

function cancelled<T>(): QuicklensResult<T> {
  return {
    ok: false,
    error: {
      code: "cancelled",
      message: "The Quicklens read was cancelled.",
      recovery: "Refresh when ready.",
    },
  };
}
