/**
 * Runtime-validated Codex app-server 0.152.1 protocol boundary.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle
 */
import { z } from "zod";
import { JsonValueSchema, type JsonValue } from "../protocol/index.ts";

export const CodexRpcIdSchema = z.union([z.string(), z.number().int()]);
export type CodexRpcId = z.infer<typeof CodexRpcIdSchema>;

export const CodexThreadIdSchema = z.string().min(1).max(512);
export const CodexTurnIdSchema = z.string().min(1).max(512);
export const CodexItemIdSchema = z.string().min(1).max(512);

const JsonObjectSchema = z.record(z.string(), JsonValueSchema);
const EmittedAtMsSchema = z.number().int();

export const CodexThreadStatusSchema = z.union([
  z.object({ type: z.enum(["notLoaded", "idle", "systemError"]) }).loose(),
  z
    .object({
      type: z.literal("active"),
      activeFlags: z.array(z.string().min(1).max(128)).max(64),
    })
    .loose(),
]);

const CodexItemBaseSchema = z.object({
  id: CodexItemIdSchema,
  type: z.string().min(1).max(128),
  status: z.string().min(1).max(128).optional(),
});
export const CodexCollabItemSchema = CodexItemBaseSchema.extend({
  type: z.literal("collabAgentToolCall"),
  tool: z.enum([
    "spawnAgent",
    "sendInput",
    "resumeAgent",
    "wait",
    "closeAgent",
    "sendMessage",
    "followupTask",
    "interruptAgent",
    "listAgents",
  ]),
  senderThreadId: CodexThreadIdSchema,
  receiverThreadIds: z.array(CodexThreadIdSchema).max(1_000),
  prompt: z.string().max(1_000_000).nullable().optional(),
  agentsStates: z.record(z.string(), JsonValueSchema),
}).loose();
export const CodexPublicItemSchema = z.union([CodexCollabItemSchema, CodexItemBaseSchema.loose()]);
export type CodexPublicItem = z.infer<typeof CodexPublicItemSchema>;

export const CodexTurnSchema = z
  .object({
    id: CodexTurnIdSchema,
    status: z.enum(["inProgress", "completed", "interrupted", "failed"]),
    items: z.array(CodexPublicItemSchema).max(100_000),
    error: JsonValueSchema.nullable().optional(),
    startedAt: z.number().int().nullable().optional(),
    completedAt: z.number().int().nullable().optional(),
  })
  .loose();
export type CodexTurn = z.infer<typeof CodexTurnSchema>;

export const CodexThreadSchema = z
  .object({
    id: CodexThreadIdSchema,
    sessionId: z.string().min(1).max(512),
    cwd: z.string().min(1).max(32_768),
    status: CodexThreadStatusSchema,
    turns: z.array(CodexTurnSchema).max(100_000),
    parentThreadId: CodexThreadIdSchema.nullable().optional(),
    canAcceptDirectInput: z.boolean().nullable().optional(),
    cliVersion: z.string().min(1).max(128),
    ephemeral: z.boolean(),
    modelProvider: z.string().min(1).max(256),
    preview: z.string().max(1_000_000),
    createdAt: z.number().int(),
    updatedAt: z.number().int(),
  })
  .loose();
export type CodexThread = z.infer<typeof CodexThreadSchema>;

export const CodexThreadResponseSchema = z
  .object({
    thread: CodexThreadSchema,
    cwd: z.string().min(1).max(32_768).optional(),
    instructionSources: z.array(z.string().max(32_768)).max(1_000).optional(),
  })
  .loose();

export const CodexTurnStartResponseSchema = z.object({ turn: CodexTurnSchema }).loose();
export const CodexTurnSteerResponseSchema = z.object({ turnId: CodexTurnIdSchema }).strict();

export const CodexRpcResponseSchema = z.union([
  z.object({ id: CodexRpcIdSchema, result: JsonValueSchema }).strict(),
  z
    .object({
      id: CodexRpcIdSchema,
      error: z
        .object({ code: z.number().int(), message: z.string(), data: JsonValueSchema.optional() })
        .loose(),
    })
    .strict(),
]);

const UserInputQuestionSchema = z
  .object({
    id: z.string().min(1).max(256),
    header: z.string().min(1).max(256),
    question: z.string().min(1).max(100_000),
    options: z
      .array(
        z.object({ label: z.string().max(10_000), description: z.string().max(100_000) }).strict(),
      )
      .max(1_000)
      .nullable()
      .optional(),
    isOther: z.boolean().optional(),
    isSecret: z.boolean().optional(),
  })
  .strict();

export const UserInputRequestParamsSchema = z
  .object({
    threadId: CodexThreadIdSchema,
    turnId: CodexTurnIdSchema,
    itemId: CodexItemIdSchema,
    questions: z.array(UserInputQuestionSchema).min(1).max(100),
    isBlocking: z.boolean(),
    autoResolutionMs: z.number().int().nonnegative().nullable().optional(),
  })
  .strict();

const ApprovalParamsSchema = z
  .object({
    threadId: CodexThreadIdSchema,
    turnId: CodexTurnIdSchema,
    itemId: CodexItemIdSchema,
  })
  .loose();

export const CodexServerRequestSchema = z.discriminatedUnion("method", [
  z
    .object({
      id: CodexRpcIdSchema,
      method: z.literal("item/tool/requestUserInput"),
      params: UserInputRequestParamsSchema,
    })
    .strict(),
  z
    .object({
      id: CodexRpcIdSchema,
      method: z.literal("item/commandExecution/requestApproval"),
      params: ApprovalParamsSchema,
    })
    .strict(),
  z
    .object({
      id: CodexRpcIdSchema,
      method: z.literal("item/fileChange/requestApproval"),
      params: ApprovalParamsSchema,
    })
    .strict(),
  z
    .object({
      id: CodexRpcIdSchema,
      method: z.literal("item/permissions/requestApproval"),
      params: ApprovalParamsSchema,
    })
    .strict(),
]);
export type CodexServerRequest = z.infer<typeof CodexServerRequestSchema>;

const ThreadNotificationSchema = z.discriminatedUnion("method", [
  z
    .object({
      method: z.literal("thread/started"),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z.object({ thread: CodexThreadSchema }).strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal("thread/status/changed"),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z.object({ threadId: CodexThreadIdSchema, status: CodexThreadStatusSchema }).strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal("thread/closed"),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z.object({ threadId: CodexThreadIdSchema }).strict(),
    })
    .strict(),
]);

const TurnNotificationSchema = z.discriminatedUnion("method", [
  z
    .object({
      method: z.literal("turn/started"),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z.object({ threadId: CodexThreadIdSchema, turn: CodexTurnSchema }).strict(),
    })
    .strict(),
  z
    .object({
      method: z.literal("turn/completed"),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z.object({ threadId: CodexThreadIdSchema, turn: CodexTurnSchema }).strict(),
    })
    .strict(),
]);

const ItemNotificationSchema = z.discriminatedUnion("method", [
  z
    .object({
      method: z.enum(["item/started", "item/completed"]),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z
        .object({
          threadId: CodexThreadIdSchema,
          turnId: CodexTurnIdSchema,
          item: CodexPublicItemSchema,
        })
        .loose(),
    })
    .strict(),
  z
    .object({
      method: z.literal("item/agentMessage/delta"),
      emittedAtMs: EmittedAtMsSchema.optional(),
      params: z
        .object({
          threadId: CodexThreadIdSchema,
          turnId: CodexTurnIdSchema,
          itemId: CodexItemIdSchema,
          delta: z.string().max(10_000_000),
        })
        .strict(),
    })
    .strict(),
]);

const RequestResolvedSchema = z
  .object({
    method: z.literal("serverRequest/resolved"),
    emittedAtMs: EmittedAtMsSchema.optional(),
    params: z.object({ threadId: CodexThreadIdSchema, requestId: CodexRpcIdSchema }).strict(),
  })
  .strict();

export const CodexNotificationSchema = z.union([
  ThreadNotificationSchema,
  TurnNotificationSchema,
  ItemNotificationSchema,
  RequestResolvedSchema,
]);
export type CodexNotification = z.infer<typeof CodexNotificationSchema>;

export const CodexInboundMessageSchema = z.union([
  CodexRpcResponseSchema,
  CodexServerRequestSchema,
  CodexNotificationSchema,
]);
export type CodexInboundMessage = z.infer<typeof CodexInboundMessageSchema>;

export const CodexWireMessageSchema = z.union([
  CodexRpcResponseSchema,
  z
    .object({
      id: CodexRpcIdSchema,
      method: z.string().min(1).max(512),
      params: JsonValueSchema,
    })
    .strict(),
  z
    .object({
      method: z.string().min(1).max(512),
      params: JsonValueSchema,
      emittedAtMs: EmittedAtMsSchema.optional(),
    })
    .strict(),
]);
export type CodexWireMessage = z.infer<typeof CodexWireMessageSchema>;

export const CodexUserInputResponseSchema = z
  .object({
    answers: z.record(
      z.string().min(1).max(256),
      z.object({ answers: z.array(z.string().max(100_000)).max(1_000) }).strict(),
    ),
  })
  .strict();
export const CodexCommandApprovalResponseSchema = z
  .object({
    decision: z.union([
      z.enum(["accept", "acceptForSession", "decline", "cancel"]),
      JsonObjectSchema,
    ]),
  })
  .strict();
export const CodexFileApprovalResponseSchema = z
  .object({ decision: z.enum(["accept", "acceptForSession", "decline", "cancel"]) })
  .strict();
export const CodexPermissionsResponseSchema = z
  .object({
    permissions: JsonObjectSchema,
    scope: z.enum(["turn", "session"]).optional(),
    strictAutoReview: z.boolean().nullable().optional(),
  })
  .strict();

export function publicItem(item: CodexPublicItem): JsonValue {
  if (item.type === "reasoning") {
    return {
      id: item.id,
      type: item.type,
      ...(item.status === undefined ? {} : { status: item.status }),
      redacted: true,
    };
  }
  return JsonValueSchema.parse(item);
}
