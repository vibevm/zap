/** @scope spec://org.vibevm.zap/lens/PROP-001#adapters */
import { z } from "zod";
import {
  ActorDescriptorSchema,
  ActorHandleSchema,
  ActorIdSchema,
  DecimalSchema,
  DeliveryIdSchema,
  HostBindingSchema,
  MessageIdSchema,
} from "../protocol/index.ts";
import type {
  ActorId,
  BindingId,
  DeliveryId,
  HostKind,
  LosslessDecimal,
  MessageId,
  Result,
} from "../protocol/index.ts";

export const REQ_ADAPTERS = "spec://org.vibevm.zap/lens/PROP-001#adapters";
export const REQ_DELIVERY = "spec://org.vibevm.zap/lens/PROP-001#delivery";
export const REQ_IDENTITY = "spec://org.vibevm.zap/lens/PROP-001#identity";

export const SafePointHostSchema = z.enum(["codex", "claude_code", "qwen_code"]);
export type SafePointHost = z.infer<typeof SafePointHostSchema>;

export const SafePointEventSchema = z.enum([
  "SessionStart",
  "SubagentStart",
  "PreToolUse",
  "PostToolUse",
  "PostToolUseFailure",
  "UserPromptSubmit",
  "Stop",
  "SubagentStop",
  "PreCompact",
]);
export type SafePointEvent = z.infer<typeof SafePointEventSchema>;

export const SafePointHookInputSchema = z
  .object({
    session_id: z.string().min(1).max(512),
    hook_event_name: SafePointEventSchema,
    agent_id: z.string().min(1).max(512).optional(),
    agent_type: z.string().min(1).max(256).optional(),
    turn_id: z.string().min(1).max(512).optional(),
    prompt_id: z.string().min(1).max(512).optional(),
  })
  .loose();
export type SafePointHookInput = z.infer<typeof SafePointHookInputSchema>;

export const ActorRouteBindingSchema = z
  .object({
    actor: ActorDescriptorSchema,
    handle: ActorHandleSchema,
    host: HostBindingSchema,
  })
  .strict();
export type ActorRouteBinding = z.infer<typeof ActorRouteBindingSchema>;

export const AuthenticatedActorHandleSchema = ActorHandleSchema.extend({
  nativeAttested: z.boolean().default(false),
}).strict();
export type AuthenticatedActorHandle = z.infer<typeof AuthenticatedActorHandleSchema>;

export const PersistedHostMessageSchema = z
  .object({
    deliveryId: DeliveryIdSchema,
    messageId: MessageIdSchema,
    recipientActorId: ActorIdSchema,
    bindingGeneration: DecimalSchema,
    content: z.string().min(1).max(16_384),
    persisted: z.literal(true),
  })
  .strict();
export type PersistedHostMessage = z.infer<typeof PersistedHostMessageSchema>;

export const AdapterDiagnosticCodeSchema = z.enum([
  "invalid_hook_input",
  "invalid_offer",
  "binding_not_found",
  "ambiguous_binding",
  "binding_mismatch",
  "stale_binding",
  "unsupported_host",
  "transport_failure",
]);
export type AdapterDiagnosticCode = z.infer<typeof AdapterDiagnosticCodeSchema>;

export interface AdapterDiagnostic {
  readonly code: AdapterDiagnosticCode;
  readonly requirement: string;
  readonly message: string;
  readonly retryable: boolean;
  readonly details?: Readonly<Record<string, string>>;
}

export interface HookRoute {
  readonly host: SafePointHost;
  readonly actorId: ActorId;
  readonly bindingId: BindingId;
  readonly bindingGeneration: LosslessDecimal;
  readonly eventName: SafePointEvent;
  readonly hostSessionId: string;
  readonly hostSubagentId?: string;
}

export type DeliveryObservation =
  | {
      readonly kind: "observed";
      readonly value: boolean;
      readonly evidence: string;
    }
  | { readonly kind: "not_observed" }
  | { readonly kind: "uncertain"; readonly reason: string };

export interface DeliveryObservations {
  readonly persisted: DeliveryObservation;
  readonly offered: DeliveryObservation;
  readonly hostAccepted: DeliveryObservation;
  readonly actorAcknowledged: DeliveryObservation;
}

export interface SafePointDelivery {
  readonly deliveryId: DeliveryId;
  readonly messageId: MessageId;
  readonly observations: DeliveryObservations;
}

export interface SafePointOffer {
  readonly route: HookRoute;
  readonly hostOutput: {
    readonly hookSpecificOutput: {
      readonly hookEventName: SafePointEvent;
      readonly additionalContext: string;
    };
  };
  readonly deliveries: readonly SafePointDelivery[];
}

export interface HostCapabilities {
  readonly host: Exclude<HostKind, "lens" | "test">;
  readonly safePointContext: "supported" | "unsupported";
  readonly idleWake: "unsupported" | "conditional" | "not_verified";
  readonly activeTurnSteering: "unsupported" | "conditional" | "not_verified";
  readonly existingSessionAttach: "unsupported" | "conditional" | "not_verified";
  readonly inputPaths: readonly HostInputPath[];
  readonly limitations: readonly string[];
}

export interface HostInputPath {
  readonly kind:
    | "safe_point_hook"
    | "native_channel"
    | "shared_server"
    | "managed_server"
    | "regular_file_queue"
    | "native_peer";
  readonly availability: "supported" | "conditional" | "unsupported";
  readonly idleWake: boolean;
  readonly activeTurnSteering: boolean;
}

export interface OpenCodeTransport {
  readonly observeStatus: (sessionId: string) => Promise<unknown>;
  readonly sendWhenIdle: (request: OpenCodeSendRequest) => Promise<unknown>;
}

export interface OpenCodeSendRequest {
  readonly sessionId: string;
  readonly expectedStatusObservationId: string;
  readonly deliveryId: DeliveryId;
  readonly messageId: MessageId;
  readonly content: string;
}

export interface OpenCodeDispatch {
  readonly outcome: "held" | "host_accepted" | "rejected" | "uncertain";
  readonly deliveryId: DeliveryId;
  readonly messageId: MessageId;
  readonly reconcileRequired: boolean;
  readonly observations: DeliveryObservations;
}

export const OpenCodeStatusSchema = z
  .object({
    state: z.enum(["idle", "busy", "unknown"]),
    observationId: z.string().min(1).max(256),
  })
  .strict();

export const OpenCodeSendResultSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("completed"),
      statusObservationId: z.string().min(1).max(256),
      hostReceiptId: z.string().min(1).max(512),
    })
    .strict(),
  z
    .object({
      kind: z.literal("accepted_without_result"),
      statusObservationId: z.string().min(1).max(256),
    })
    .strict(),
  z
    .object({
      kind: z.literal("busy"),
      statusObservationId: z.string().min(1).max(256),
    })
    .strict(),
  z.object({ kind: z.literal("rejected"), reason: z.string().min(1) }).strict(),
]);

export function success<T>(value: T): Result<T, AdapterDiagnostic> {
  return { ok: true, value };
}

export function failure<T>(error: AdapterDiagnostic): Result<T, AdapterDiagnostic> {
  return { ok: false, error };
}

export function diagnostic(
  code: AdapterDiagnosticCode,
  requirement: string,
  why: string,
  fixSurface: string,
  retryable = false,
  details?: Readonly<Record<string, string>>,
): AdapterDiagnostic {
  return {
    code,
    requirement,
    message: `violates REQ ${requirement}: ${why}; fix surface: ${fixSurface}`,
    retryable,
    ...(details === undefined ? {} : { details }),
  };
}

export function observed(value: boolean, evidence: string): DeliveryObservation {
  return { kind: "observed", value, evidence };
}

export function notObserved(): DeliveryObservation {
  return { kind: "not_observed" };
}

export function uncertain(reason: string): DeliveryObservation {
  return { kind: "uncertain", reason };
}

export function zodDetails(error: z.ZodError): Readonly<Record<string, string>> {
  return {
    issueCount: String(error.issues.length),
    paths: error.issues.map((issue) => issue.path.join(".") || "$root").join(","),
  };
}
