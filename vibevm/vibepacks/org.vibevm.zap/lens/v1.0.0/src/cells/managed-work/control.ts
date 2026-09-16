/** Provider-side control for an exact owned interactive managed session.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import { z } from "zod";
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";
import { AgentSessionIdSchema, RunIdSchema, TerminalIdSchema } from "../workspace-model/index.ts";
import { ManagedProviderIdSchema } from "./provider-types.ts";
import type { ManagedWorkResult } from "./contracts.ts";
import type { ProviderLaunch } from "./providers.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";

export const ManagedSessionReadinessSchema = z.enum([
  "starting",
  "idle",
  "busy",
  "permission_required",
  "interrupting",
  "stopped",
  "unknown",
]);
export type ManagedSessionReadiness = z.infer<typeof ManagedSessionReadinessSchema>;

export const ManagedControlTargetSchema = z
  .object({
    runId: RunIdSchema,
    actorId: ActorIdSchema,
    sessionId: AgentSessionIdSchema,
    terminalId: TerminalIdSchema,
    expectedProcessEpoch: z.string().min(1).max(160),
  })
  .strict();
export type ManagedControlTarget = z.infer<typeof ManagedControlTargetSchema>;

export const ManagedSessionControlEventSchema = z
  .object({
    eventId: z.string().min(3).max(160),
    runId: RunIdSchema,
    actorId: ActorIdSchema,
    sessionId: AgentSessionIdSchema,
    terminalId: TerminalIdSchema,
    provider: ManagedProviderIdSchema,
    processEpoch: z.string().min(1).max(160),
    sourceSequence: DecimalSchema,
    kind: z.enum([
      "session_ready",
      "turn_started",
      "permission_required",
      "turn_settled",
      "session_exited",
      "delivery_rejected",
    ]),
    providerSessionId: z.string().min(1).max(512).nullable(),
    providerTurnId: z.string().min(1).max(512).nullable(),
    transportCorrelation: z.string().min(3).max(512).nullable(),
    occurredAt: z.iso.datetime(),
  })
  .strict();
export type ManagedSessionControlEvent = z.infer<typeof ManagedSessionControlEventSchema>;

export interface ManagedSessionControlPort {
  inspect(target: ManagedControlTarget): ManagedWorkResult<{
    readonly readiness: ManagedSessionReadiness;
    readonly providerSessionId: string | null;
    readonly providerTurnId: string | null;
    readonly observationId: string;
    readonly automationControlEpoch: string | null;
    readonly pauseRequested: boolean;
  }>;
  interrupt(
    target: ManagedControlTarget & { readonly reason: "project_pause" | "user_interrupt" },
  ): Promise<
    ManagedWorkResult<{
      readonly observation: "requested" | "already_idle" | "unsupported";
    }>
  >;
  continueSession(target: ManagedControlTarget): ManagedWorkResult<{
    readonly readiness: ManagedSessionReadiness;
  }>;
  offer(
    target: ManagedControlTarget & {
      readonly expectedAutomationControlEpoch: string;
      readonly deliveryId: string;
      readonly bodyMarkdown: string;
    },
  ): Promise<
    ManagedWorkResult<{
      readonly observation:
        | "host_accepted"
        | "provider_queued"
        | "busy"
        | "permission_required"
        | "uncertain";
      readonly transportCorrelation: string | null;
    }>
  >;
  subscribe(listener: (event: ManagedSessionControlEvent) => void): () => void;
  close(): void;
}

export interface ManagedControlProvisioner extends ManagedSessionControlPort {
  prepare(input: {
    readonly runId: z.infer<typeof RunIdSchema>;
    readonly actorId: z.infer<typeof ActorIdSchema>;
    readonly sessionId: z.infer<typeof AgentSessionIdSchema>;
    readonly terminalId: z.infer<typeof TerminalIdSchema>;
    readonly provider: z.infer<typeof ManagedProviderIdSchema>;
    readonly launch: ProviderLaunch;
  }): Promise<
    ManagedWorkResult<{ readonly target: ManagedControlTarget; readonly launch: ProviderLaunch }>
  >;
  activate(input: {
    readonly target: ManagedControlTarget;
    readonly access: WorkspaceAccessContext;
    readonly leaseId: string;
    readonly automationControlEpoch: string;
  }): Promise<ManagedWorkResult<null>>;
  discard(target: ManagedControlTarget): void;
  settleStopped(target: ManagedControlTarget): void;
}

export interface ManagedControlIo {
  input(data: string): ManagedWorkResult<void>;
  interrupt(): ManagedWorkResult<void>;
  stop(): ManagedWorkResult<void>;
}

export interface ManagedProviderControlAdapter {
  readonly provider: z.infer<typeof ManagedProviderIdSchema>;
  readonly canQueueWhileBusy?: boolean;
  prepare(input: {
    readonly target: ManagedControlTarget;
    readonly launch: ProviderLaunch;
    readonly publish: (event: Omit<ManagedSessionControlEvent, "sourceSequence">) => void;
  }): Promise<ManagedWorkResult<{ readonly launch: ProviderLaunch; readonly state: unknown }>>;
  activate?(state: unknown, io: ManagedControlIo): Promise<ManagedWorkResult<null>>;
  interrupt(
    state: unknown,
    reason: "project_pause" | "user_interrupt",
    io: ManagedControlIo,
  ): Promise<ManagedWorkResult<"requested" | "idle">>;
  offer(
    state: unknown,
    deliveryId: string,
    bodyMarkdown: string,
    io: ManagedControlIo,
  ): Promise<ManagedWorkResult<{ readonly queued: boolean; readonly correlation: string | null }>>;
  close(state: unknown): void;
}
