/** Safe provider event normalization. @scope spec://org.vibevm.zap/lens/PROP-012#events */
import { z } from "zod";
import type { CoordinatorEvent, CoordinatorScope } from "../agent-runtime/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import type { ProviderCoordinatorId, ProviderCoordinatorSession } from "./index.ts";

export function normalizeProviderEvent(
  provider: ProviderCoordinatorId,
  sessionId: CoordinatorScope["coordinatorSessionId"],
  session: ProviderCoordinatorSession,
  raw: unknown,
  correlation: Exclude<CoordinatorEvent["transportCorrelation"], null | undefined> | null,
  observationSequence: string,
): CoordinatorEvent | null {
  const parsed = z
    .object({
      kind: z.string().optional(),
      nativeTurnId: z.string().nullable().optional(),
      nativeItemId: z.string().nullable().optional(),
      data: z.unknown().optional(),
    })
    .catchall(z.unknown())
    .safeParse(raw);
  if (!parsed.success) return null;
  const providerEvent = providerEventKind(provider, parsed.data);
  const mapped = z
    .enum([
      "message_delta",
      "item_completed",
      "native_child_observed",
      "native_message_observed",
      "turn_started",
      "turn_completed",
      "session_started",
      "session_resumed",
      "session_status",
      "session_paused",
      "session_continued",
      "lifecycle_uncertain",
      "host_request_pending",
      "host_request_resolved",
      "process_exited",
      "host_event_unmapped",
    ])
    .catch("host_event_unmapped")
    .parse(providerEvent.kind);
  const data =
    mapped === "host_request_pending"
      ? pendingHostData(sessionId, session, providerEvent)
      : providerEvent.data;
  return {
    coordinatorSessionId: sessionId,
    processEpoch: session.processEpoch,
    nativeThreadId: session.nativeThreadId,
    nativeTurnId: providerEvent.nativeTurnId,
    nativeItemId: providerEvent.nativeItemId,
    kind: mapped,
    sourceEventId: `${provider}:${session.processEpoch}:observation:${observationSequence}`,
    ...(correlation !== null &&
    (mapped === "message_delta" || mapped === "item_completed" || mapped === "turn_completed")
      ? { transportCorrelation: correlation }
      : {}),
    data: JsonValueSchema.safeParse(data).success
      ? JsonValueSchema.parse(data)
      : { provider, eventType: "invalid_data" },
  };
}

function pendingHostData(
  sessionId: CoordinatorScope["coordinatorSessionId"],
  session: ProviderCoordinatorSession,
  event: ReturnType<typeof providerEventKind>,
) {
  const parsed = z
    .looseObject({
      requestId: z.union([z.string(), z.number().int()]),
      kind: z.literal("permission_approval"),
      body: JsonValueSchema,
    })
    .safeParse(event.data);
  if (!parsed.success || event.nativeItemId === null) return event.data;
  return {
    coordinatorSessionId: sessionId,
    nativeThreadId: session.nativeThreadId,
    nativeTurnId: event.nativeTurnId,
    nativeItemId: event.nativeItemId,
    requestId: parsed.data.requestId,
    processEpoch: session.processEpoch,
    kind: parsed.data.kind,
    body: parsed.data.body,
  };
}

function providerEventKind(
  provider: ProviderCoordinatorId,
  raw: Record<string, unknown>,
): {
  readonly kind: string | undefined;
  readonly nativeTurnId: string | null;
  readonly nativeItemId: string | null;
  readonly data: unknown;
} {
  const explicit = stringField(raw, "kind");
  const type = stringField(raw, "type");
  const nativeTurnId = stringField(raw, "nativeTurnId") ?? stringField(raw, "turn_id") ?? null;
  const nativeItemId = stringField(raw, "nativeItemId") ?? stringField(raw, "item_id") ?? null;
  if (explicit !== undefined)
    return { kind: explicit, nativeTurnId, nativeItemId, data: raw["data"] ?? raw };
  if (type === "process_diagnostic") {
    const diagnostic = z
      .looseObject({
        channel: z.literal("stderr"),
        category: z.enum(["auth", "network", "rate_limit", "api", "configuration", "other"]),
        digest: z.string().regex(/^[0-9a-f]{64}$/),
        bytes: z.number().int().min(0),
        truncated: z.boolean(),
      })
      .safeParse(raw);
    return diagnostic.success
      ? {
          kind: "host_event_unmapped",
          nativeTurnId,
          nativeItemId,
          data: {
            provider,
            eventType: "process_diagnostic",
            channel: diagnostic.data.channel,
            category: diagnostic.data.category,
            digest: diagnostic.data.digest,
            bytes: diagnostic.data.bytes,
            truncated: diagnostic.data.truncated,
          },
        }
      : {
          kind: "host_event_unmapped",
          nativeTurnId,
          nativeItemId,
          data: { provider, eventType: "invalid_process_diagnostic" },
        };
  }
  if (type === "process_exited") {
    const exit = z.looseObject({ exitCode: z.number().int().nullable() }).safeParse(raw);
    return exit.success
      ? {
          kind: "process_exited",
          nativeTurnId,
          nativeItemId,
          data: { exitCode: exit.data.exitCode },
        }
      : {
          kind: "host_event_unmapped",
          nativeTurnId,
          nativeItemId,
          data: { provider, eventType: "invalid_process_exit" },
        };
  }
  if (provider === "claude_code" && type === "stream_event") {
    const event = objectField(raw, "event");
    const eventType = stringField(event, "type");
    return {
      kind: eventType === "content_block_delta" ? "message_delta" : "host_event_unmapped",
      nativeTurnId: stringField(raw, "message_id") ?? nativeTurnId,
      nativeItemId: stringField(event, "index") ?? nativeItemId,
      data: event ?? raw,
    };
  }
  if ((provider === "claude_code" || provider === "qwen_code") && type === "assistant")
    return {
      kind: "message_delta",
      nativeTurnId,
      nativeItemId,
      data: raw["message"] ?? { role: "assistant", content: [] },
    };
  if ((provider === "claude_code" || provider === "qwen_code") && type === "result")
    return { kind: "turn_completed", nativeTurnId, nativeItemId, data: raw };
  if (provider === "opencode" && type === "text")
    return { kind: "message_delta", nativeTurnId, nativeItemId, data: raw["text"] ?? raw };
  if (provider === "opencode" && type === "turn.started")
    return { kind: "turn_started", nativeTurnId, nativeItemId, data: raw };
  if (provider === "opencode" && type === "turn.completed")
    return { kind: "turn_completed", nativeTurnId, nativeItemId, data: raw };
  if (type === "session.status")
    return { kind: "session_status", nativeTurnId, nativeItemId, data: raw["status"] ?? raw };
  return {
    kind: undefined,
    nativeTurnId,
    nativeItemId,
    data: { provider, eventType: type ?? "unknown" },
  };
}

function stringField(value: Record<string, unknown> | undefined, key: string): string | undefined {
  const field = value?.[key];
  return typeof field === "string" ? field : undefined;
}

function objectField(
  value: Record<string, unknown> | undefined,
  key: string,
): Record<string, unknown> | undefined {
  const field = value?.[key];
  return typeof field === "object" && field !== null && !Array.isArray(field)
    ? z.record(z.string(), z.unknown()).parse(field)
    : undefined;
}
