/** Browser-safe managed-terminal wire views. @scope spec://org.vibevm.zap/lens/PROP-006#managed-terminal */
import { z } from "zod";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  TerminalIdSchema,
  TerminalLeaseIdSchema,
  WorkContextIdSchema,
  AgentSessionIdSchema,
  RunIdSchema,
} from "./ids.ts";

export const TerminalOutputChunkSchema = z
  .object({
    terminalId: TerminalIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    controlEpoch: DecimalSchema,
    sequence: DecimalSchema,
    data: z.string().max(64_000),
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type TerminalOutputChunk = z.infer<typeof TerminalOutputChunkSchema>;

export const TerminalOutputPageSchema = z
  .object({
    terminalId: TerminalIdSchema,
    events: z.array(TerminalOutputChunkSchema).max(256),
    afterSequence: DecimalSchema,
    nextSequence: DecimalSchema.nullable(),
    gap: z
      .object({ firstAvailableSequence: DecimalSchema, reason: z.string().min(1).max(2_000) })
      .strict()
      .nullable(),
  })
  .strict();
export type TerminalOutputPage = z.infer<typeof TerminalOutputPageSchema>;

export const TerminalLeaseViewSchema = z
  .object({
    terminalId: TerminalIdSchema,
    leaseId: TerminalLeaseIdSchema,
    clientId: ClientIdSchema,
    controlEpoch: DecimalSchema,
    acquiredAt: z.iso.datetime(),
  })
  .strict();
export type TerminalLeaseView = z.infer<typeof TerminalLeaseViewSchema>;

export const TerminalCommandReceiptSchema = z
  .object({ terminalId: TerminalIdSchema, observation: z.literal("accepted") })
  .strict();
export type TerminalCommandReceipt = z.infer<typeof TerminalCommandReceiptSchema>;

export const ManagedTerminalViewSchema = z
  .object({
    terminalId: TerminalIdSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    sessionId: AgentSessionIdSchema,
    runId: RunIdSchema,
    state: z.enum(["running", "stopping", "exited", "stopped", "unknown"]),
    controlEpoch: DecimalSchema,
  })
  .strict();
export type ManagedTerminalView = z.infer<typeof ManagedTerminalViewSchema>;

const TerminalScopedCommandSchema = z.object({
  clientRequestId: ClientRequestIdSchema,
  projectId: ProjectIdSchema,
  contextId: WorkContextIdSchema,
});

export const TerminalListReadRequestSchema = z
  .object({
    operation: z.literal("terminal.list.v1"),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
  })
  .strict();
export const TerminalOutputReadRequestSchema = z
  .object({
    operation: z.literal("terminal.output.page.v1"),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    terminalId: TerminalIdSchema,
    afterSequence: DecimalSchema,
    limit: z.number().int().min(1).max(256),
  })
  .strict();

export const TerminalStartCommandSchema = TerminalScopedCommandSchema.extend({
  operation: z.literal("terminal.start.v1"),
  profileId: z.string().min(3).max(160),
  terminalId: TerminalIdSchema,
  sessionId: AgentSessionIdSchema,
  runId: RunIdSchema,
}).strict();
export const TerminalAcquireCommandSchema = TerminalScopedCommandSchema.extend({
  operation: z.literal("terminal.acquire.v1"),
  terminalId: TerminalIdSchema,
  expectedControlEpoch: DecimalSchema,
  takeover: z.boolean().default(false),
}).strict();
export const TerminalReleaseCommandSchema = TerminalScopedCommandSchema.extend({
  operation: z.literal("terminal.release.v1"),
  terminalId: TerminalIdSchema,
  leaseId: TerminalLeaseIdSchema,
  expectedControlEpoch: DecimalSchema,
}).strict();
export const TerminalInputCommandSchema = TerminalReleaseCommandSchema.omit({ operation: true })
  .extend({ operation: z.literal("terminal.input.v1"), data: z.string().min(1).max(64_000) })
  .strict();
export const TerminalResizeCommandSchema = TerminalReleaseCommandSchema.omit({ operation: true })
  .extend({
    operation: z.literal("terminal.resize.v1"),
    columns: z.number().int().min(1).max(1_000),
    rows: z.number().int().min(1).max(1_000),
  })
  .strict();
export const TerminalInterruptCommandSchema = TerminalReleaseCommandSchema.omit({ operation: true })
  .extend({ operation: z.literal("terminal.interrupt.v1") })
  .strict();
export const TerminalStopCommandSchema = TerminalReleaseCommandSchema.omit({ operation: true })
  .extend({ operation: z.literal("terminal.stop.v1") })
  .strict();

export const TerminalListReadResponseSchema = z
  .object({
    operation: z.literal("terminal.list.v1"),
    terminals: z.array(ManagedTerminalViewSchema),
  })
  .strict();
export const TerminalOutputReadResponseSchema = z
  .object({ operation: z.literal("terminal.output.page.v1"), page: TerminalOutputPageSchema })
  .strict();
export const TerminalAcquireResponseSchema = z
  .object({ operation: z.literal("terminal.acquire.v1"), lease: TerminalLeaseViewSchema })
  .strict();
export const TerminalStartResponseSchema = z
  .object({ operation: z.literal("terminal.start.v1"), terminal: ManagedTerminalViewSchema })
  .strict();

function terminalReceiptResponseSchema<Operation extends z.ZodLiteral<string>>(
  operation: Operation,
) {
  return z.object({ operation, receipt: TerminalCommandReceiptSchema }).strict();
}
export const TerminalReleaseResponseSchema = terminalReceiptResponseSchema(
  z.literal("terminal.release.v1"),
);
export const TerminalInputResponseSchema = terminalReceiptResponseSchema(
  z.literal("terminal.input.v1"),
);
export const TerminalResizeResponseSchema = terminalReceiptResponseSchema(
  z.literal("terminal.resize.v1"),
);
export const TerminalInterruptResponseSchema = terminalReceiptResponseSchema(
  z.literal("terminal.interrupt.v1"),
);
export const TerminalStopResponseSchema = terminalReceiptResponseSchema(
  z.literal("terminal.stop.v1"),
);

export const TerminalReadRequestSchemas = [
  TerminalListReadRequestSchema,
  TerminalOutputReadRequestSchema,
] as const;
export const TerminalReadResponseSchemas = [
  TerminalListReadResponseSchema,
  TerminalOutputReadResponseSchema,
] as const;
export const TerminalCommandSchemas = [
  TerminalStartCommandSchema,
  TerminalAcquireCommandSchema,
  TerminalReleaseCommandSchema,
  TerminalInputCommandSchema,
  TerminalResizeCommandSchema,
  TerminalInterruptCommandSchema,
  TerminalStopCommandSchema,
] as const;
export const TerminalCommandResponseSchemas = [
  TerminalAcquireResponseSchema,
  TerminalStartResponseSchema,
  TerminalReleaseResponseSchema,
  TerminalInputResponseSchema,
  TerminalResizeResponseSchema,
  TerminalInterruptResponseSchema,
  TerminalStopResponseSchema,
] as const;
