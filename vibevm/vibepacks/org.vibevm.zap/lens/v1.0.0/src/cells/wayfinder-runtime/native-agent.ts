/** Authenticated native work attachment tools. @scope spec://org.vibevm.zap/lens/PROP-011#deferred-instructions */
import { createHash } from "node:crypto";
import {
  NativeWorkAttachmentAckInputSchema,
  NativeWorkBeforeInputSchema,
  NativeWorkReadInputSchema,
  type NativeWorkAgentPort,
} from "../managed-work/index.ts";
import { JsonValueSchema, type JsonValue, type Result } from "../protocol/index.ts";
import { failure, type AdapterSessionId, type AgentTransportPort } from "../transport/index.ts";
import { ClientIdSchema, WorkspaceAccessContextSchema } from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { DeclaredNativeWorkTargetBridge } from "./native-targets.ts";

interface NativeReceipt {
  readonly attemptId: string;
  readonly state: "ready" | "waiting_for_target";
  readonly instructions: readonly {
    readonly attachmentId: string;
    readonly version: string;
    readonly bodyMarkdown: string;
  }[];
}

export function createNativeWorkAgentPort(options: {
  readonly agent: AgentTransportPort;
  readonly bridge: () => DeclaredNativeWorkTargetBridge | undefined;
  readonly store: WorkspaceStore;
}): NativeWorkAgentPort {
  const receipts = new Map<string, NativeReceipt>();
  const bound = async (session: AdapterSessionId) => {
    const actor = await options.agent.context(session);
    if (!actor.ok) return actor;
    const scope = options.store.resolveAgentScope(
      actor.value.actor.workspaceId,
      actor.value.actor.conversationId,
    );
    if (!scope.ok) return failure("forbidden", "native work has no exact project scope");
    const bridge = options.bridge();
    if (bridge === undefined)
      return failure("unsupported_operation", "native work attachments are not configured");
    const access = WorkspaceAccessContextSchema.parse({
      principalId: actor.value.actor.principalId,
      actorId: actor.value.actor.actorId,
      clientId: ClientIdSchema.parse("client.native-work." + digest(actor.value.actor.actorId)),
      authorizedProjectIds: [scope.value.projectId],
    });
    return { ok: true as const, value: { actor: actor.value, scope: scope.value, bridge, access } };
  };
  return {
    async beforeWork(session, raw) {
      const input = NativeWorkBeforeInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "native before-work input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      if (
        input.data.targetRefs.some(
          (target) =>
            target.projectId !== context.value.scope.projectId ||
            target.contextId !== context.value.scope.contextId,
        )
      )
        return failure("forbidden", "native work targets are outside caller scope");
      const prepared = await context.value.bridge.prepare({
        access: context.value.access,
        attemptId: input.data.attemptId,
        recipientActorId: context.value.actor.actor.actorId,
        packet: input.data,
      });
      if (!prepared.ok) return nativeFailure(prepared);
      const receipt = {
        attemptId: input.data.attemptId,
        state: prepared.value.state,
        instructions: prepared.value.instructions,
      } satisfies NativeReceipt;
      retain(receipts, key(session, input.data.attemptId), receipt);
      return json(receipt);
    },
    async read(session, raw) {
      const input = NativeWorkReadInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "native work read input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const receipt = receipts.get(key(session, input.data.attemptId));
      return receipt === undefined
        ? failure("not_found", "native before-work receipt is unavailable")
        : json(receipt);
    },
    async acknowledgeAttachment(session, raw) {
      const input = NativeWorkAttachmentAckInputSchema.safeParse(raw);
      if (!input.success)
        return failure("invalid_input", "native attachment acknowledgement is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const receipt = receipts.get(key(session, input.data.attemptId));
      if (
        receipt === undefined ||
        !receipt.instructions.some(
          (instruction) =>
            instruction.attachmentId === input.data.attachmentId &&
            instruction.version === input.data.version,
        )
      )
        return failure("forbidden", "native attachment was not offered to this attempt");
      const acknowledged = await context.value.bridge.acknowledge({
        access: context.value.access,
        attemptId: input.data.attemptId,
        attachmentId: input.data.attachmentId,
        version: input.data.version,
      });
      return acknowledged.ok ? json(null) : nativeFailure(acknowledged);
    },
  };
}

function retain(
  receipts: Map<string, NativeReceipt>,
  keyValue: string,
  receipt: NativeReceipt,
): void {
  receipts.delete(keyValue);
  receipts.set(keyValue, receipt);
  const oldest = receipts.size > 1_024 ? receipts.keys().next().value : undefined;
  if (oldest !== undefined) receipts.delete(oldest);
}
function key(session: AdapterSessionId, attemptId: string): string {
  return `${session}\u0000${attemptId}`;
}
function json(value: unknown): Result<JsonValue> {
  const parsed = JsonValueSchema.safeParse(value);
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("storage_failure", "native work result is not JSON-safe");
}
function nativeFailure(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}): Result<never> {
  return failure(
    result.error.code === "forbidden" ? "forbidden" : "storage_failure",
    result.error.message,
  );
}
function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 32);
}
