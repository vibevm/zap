/** Shared application-service assembly seam. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type {
  AgentHost,
  AgentRuntimeError,
  CoordinatorAdapter,
  CoordinatorCapabilities,
  CoordinatorEvent,
} from "../agent-runtime/index.ts";
import { z } from "zod";
import type { ProjectIdSchema, WorkContextIdSchema } from "../workspace-model/index.ts";
import type {
  ExecutionHostId,
  WorkspaceAccessContext,
  WorkspaceClientPort,
  AgentSessionId,
  ProjectId,
  WorkContextId,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import {
  ActorIdSchema,
  type ActorId,
  type ConversationId,
  type WorkspaceId,
} from "../protocol/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { ModelPolicyService } from "../model-policy-service/index.ts";
import type {
  CoordinatorRoutingResult,
  CoordinatorRoutingResultValue,
} from "../coordinator-routing/index.ts";
import type { WorkspaceInteractionFeature } from "../workspace-interaction/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import type { ManagedTerminalResult, ManagedTerminalSnapshot } from "../managed-terminal/index.ts";

export const WORKSPACE_SERVICE_ACTIONS = [
  "read",
  "events",
  "subscribe",
  "chat.post.v1",
  "question.create.v1",
  "question.answer.v1",
  "question.amend.v1",
  "question.cancel.v1",
  "native-approval.respond.v1",
  "terminal.start.v1",
  "terminal.acquire.v1",
  "terminal.release.v1",
  "terminal.input.v1",
  "terminal.resize.v1",
  "terminal.interrupt.v1",
  "terminal.stop.v1",
  "plan.intent.v1",
  "plan.preview.v1",
  "plan.apply.v1",
  "plan.reconcile.v1",
  "plan.decide.v1",
  "session.start.v1",
  "project.pause.v1",
  "project.stop.v1",
  "project.continue.v1",
  "model-policy.update.v1",
] as const;
export type WorkspaceServiceAction = (typeof WORKSPACE_SERVICE_ACTIONS)[number];

export interface WorkspaceServiceAuthorization {
  readonly access: WorkspaceAccessContext;
  readonly allowedActions: readonly WorkspaceServiceAction[];
}

export interface CoordinatorAdapterRegistration {
  readonly profileRef: string;
  readonly host: AgentHost;
}

export interface CoordinatorAdapterRegistry {
  resolve(profileRef: string): Promise<
    | {
        readonly ok: true;
        readonly value: { readonly hostId: ExecutionHostId; readonly adapter: CoordinatorAdapter };
      }
    | { readonly ok: false; readonly error: AgentRuntimeError }
  >;
}

export interface WorkspaceServiceOptions {
  readonly store: WorkspaceStore;
  readonly adapters: CoordinatorAdapterRegistry;
  readonly modelPolicy?: ModelPolicyService;
  readonly coordinatorRouting?: CoordinatorRoutingBridge | undefined;
  readonly interactions?: WorkspaceInteractionFeature | undefined;
  readonly terminals?: WorkspaceManagedTerminalPort | undefined;
  readonly ownedCoordinatorAgents?: OwnedCoordinatorAgentPort | undefined;
  readonly planning?: WorkspacePlanningFeature | undefined;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}

export const OwnedCoordinatorAgentBindingSchema = z
  .object({ actorId: ActorIdSchema, adapterSessionId: z.string().min(24).max(160) })
  .strict();
export type OwnedCoordinatorAgentBinding = z.infer<typeof OwnedCoordinatorAgentBindingSchema>;

export interface OwnedCoordinatorAgentPort {
  bind(input: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly coordinatorSessionId: AgentSessionId;
    readonly workspaceId: WorkspaceId;
    readonly conversationId: ConversationId;
  }): Promise<WorkspaceResult<OwnedCoordinatorAgentBinding>>;
  route(actorId: ActorId): WorkspaceResult<{
    readonly coordinatorSessionId: AgentSessionId;
    readonly coordinatorActorId: ActorId;
    readonly adapterSessionId: string;
    readonly forwarding: boolean;
  } | null>;
}

export interface WorkspaceManagedTerminalPort extends ManagedTerminalServicePort {
  startRegistered(
    access: WorkspaceAccessContext,
    request: {
      readonly profileId: string;
      readonly projectId: string;
      readonly contextId: string;
      readonly terminalId: string;
      readonly sessionId: string;
      readonly runId: string;
    },
  ): Promise<ManagedTerminalResult<ManagedTerminalSnapshot>>;
}

export interface CoordinatorRoutingBridge {
  resolve(
    access: WorkspaceAccessContext,
    input: {
      readonly projectId: z.infer<typeof ProjectIdSchema>;
      readonly contextId: z.infer<typeof WorkContextIdSchema>;
      readonly sessionId: string;
      readonly runId: string;
      readonly attemptId: string;
      readonly clientRequestId: string;
      readonly sourceEventId: string;
      readonly explicitProfileId: string;
    },
  ): Promise<CoordinatorRoutingResult<CoordinatorRoutingResultValue>>;
  resume(
    access: WorkspaceAccessContext,
    input: {
      readonly projectId: z.infer<typeof ProjectIdSchema>;
      readonly contextId: z.infer<typeof WorkContextIdSchema>;
      readonly sessionId: string;
      readonly runId: string;
      readonly attemptId: string;
      readonly explicitProfileId: string;
    },
  ): Promise<CoordinatorRoutingResult<CoordinatorRoutingResultValue>>;
}

export interface WorkspaceService {
  bind(authorization: WorkspaceServiceAuthorization): WorkspaceClientPort;
  observe(event: CoordinatorEvent): void;
  close(): void;
}

export function createCoordinatorAdapterRegistry(
  registrations: readonly CoordinatorAdapterRegistration[],
): CoordinatorAdapterRegistry {
  const byRef = new Map(
    registrations.map((registration) => [registration.profileRef, registration]),
  );
  return {
    async resolve(profileRef) {
      const registration = byRef.get(profileRef);
      if (registration === undefined) {
        return {
          ok: false,
          error: {
            code: "not_found",
            message: "trusted coordinator profile is not registered",
            retry: "never",
          },
        };
      }
      const opened = await registration.host.openCoordinator(profileRef);
      return opened.ok
        ? { ok: true, value: { hostId: registration.host.hostId, adapter: opened.value } }
        : opened;
    },
  };
}

export type { CoordinatorAdapter, CoordinatorCapabilities, CoordinatorEvent };
