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
  WorkspaceReadRequest,
  WorkspaceReadResponse,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
} from "../workspace-model/index.ts";
import {
  ActorIdSchema,
  type ActorId,
  type ConversationId,
  type WorkspaceId,
} from "../protocol/index.ts";
import type { ManagedWakeDelivery, WorkspaceStore } from "../workspace-store/index.ts";
import type { ManagedAgentBackend } from "../managed-work/index.ts";
import type { ModelPolicyService } from "../model-policy-service/index.ts";
import type { ExecutionCatalogService } from "../execution-catalog-service/index.ts";
import type {
  CoordinatorRoutingResult,
  CoordinatorRoutingResultValue,
} from "../coordinator-routing/index.ts";
import type { WorkspaceInteractionFeature } from "../workspace-interaction/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import type { AnnotationService } from "../workspace-annotations/index.ts";
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
  "execution-catalog.connection.upsert.v1",
  "execution-catalog.connection.create.v1",
  "execution-catalog.configuration.upsert.v1",
  "execution-catalog.configuration.create.v1",
  "execution-catalog.preferences.update.v1",
  "execution-catalog.usage.refresh.v1",
  "managed-work.create.v1",
  "managed-work.start.v1",
  "managed-work.stop.v1",
  "managed-work.continue.v1",
  "managed-work.interrupt.v1",
  "managed-work.report.v1",
  "managed-work.review.v1",
  "annotation.note.create.v1",
  "annotation.note.update.v1",
  "annotation.note.archive.v1",
  "annotation.note.restore.v1",
  "annotation.note.relink.v1",
  "annotation.note.send.v1",
  "annotation.object.restore.intent.v1",
  "plan.workspace.prepare.v1",
  "worktree.prepare.v1",
  "integration.prepare.v1",
  "integration.test.v1",
  "integration.review.v1",
  "integration.resolution.prepare.v1",
  "integration.promote.v1",
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
  register?(
    registration: CoordinatorAdapterRegistration,
  ):
    | { readonly ok: true; readonly value: null }
    | { readonly ok: false; readonly error: AgentRuntimeError };
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
  readonly executionCatalog?: ExecutionCatalogService;
  readonly coordinatorRouting?: CoordinatorRoutingBridge | undefined;
  readonly interactions?: WorkspaceInteractionFeature | undefined;
  readonly terminals?: WorkspaceManagedTerminalPort | undefined;
  readonly ownedCoordinatorAgents?: OwnedCoordinatorAgentPort | undefined;
  readonly planning?: WorkspacePlanningFeature | undefined;
  readonly annotations?: AnnotationService | undefined;
  readonly managedWork?: ManagedAgentBackend | undefined;
  readonly managedWake?: ManagedWakePort | undefined;
  readonly repositoryWorkspaces?: RepositoryWorkspaceFeature | undefined;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}

export type RepositoryWorkspaceReadRequest = Extract<
  WorkspaceReadRequest,
  {
    operation:
      | "repository.get.v1"
      | "plan.workspace.list.v1"
      | "plan.workspace.get.v1"
      | "worktree.list.v1"
      | "worktree.get.v1"
      | "integration.list.v1"
      | "integration.get.v1"
      | "integration.diff.v1";
  }
>;
export type RepositoryWorkspaceCommandRequest = Extract<
  WorkspaceCommandRequest,
  {
    operation:
      | "plan.workspace.prepare.v1"
      | "worktree.prepare.v1"
      | "integration.prepare.v1"
      | "integration.test.v1"
      | "integration.review.v1"
      | "integration.resolution.prepare.v1"
      | "integration.promote.v1";
  }
>;
export interface RepositoryWorkspaceFeature {
  read(
    access: WorkspaceAccessContext,
    request: RepositoryWorkspaceReadRequest,
  ): Promise<WorkspaceResult<WorkspaceReadResponse>>;
  command(
    access: WorkspaceAccessContext,
    request: RepositoryWorkspaceCommandRequest,
  ): Promise<WorkspaceResult<WorkspaceCommandResponse>>;
  canOpenWriter(
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): WorkspaceResult<null>;
}

export type ManagedWakeObservation = "settled" | "requested" | "unsupported" | "uncertain";

/**
 * Planning-owned managed work control. The implementation must report settled
 * only after its owned worker/lease state is observed idle or stopped. Its
 * events are wake hints; the durable chat queue remains the source of truth.
 */
export interface ManagedWakePort {
  pauseOwned(
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): Promise<ManagedWakeObservation>;
  stopOwned(
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): Promise<"settled" | "uncertain">;
  continueOwned(
    access: WorkspaceAccessContext,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): Promise<"settled" | "unsupported" | "uncertain">;
  resolveRecipient(input: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly actorId: ActorId;
  }): Promise<ManagedWakeTarget | null>;
  offerWake(input: {
    readonly notice: ManagedWakeDelivery;
    readonly leaseId: string;
  }): Promise<"host_accepted" | "uncertain" | "busy" | "refused">;
  subscribe(listener: (event: ManagedWakeEvent) => void): () => void;
}

export interface ManagedWakeTarget {
  readonly actorId: ActorId;
  readonly runId: string;
  readonly attemptId: string;
  readonly adapterSessionId: string;
  readonly processEpoch: string | null;
  readonly leaseId: string | null;
}

export interface ManagedWakeEvent {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly actorId: ActorId | null;
  readonly state: "busy" | "idle" | "pausing" | "paused" | "stopped";
  readonly wakeEligible: boolean;
  readonly action: "wake" | "pause" | "stop" | "continue";
  readonly processEpoch: string | null;
  readonly leaseId: string | null;
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
    readonly coordinatorActorId: ActorId;
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
    register(registration) {
      const current = byRef.get(registration.profileRef);
      if (current !== undefined)
        return current.host === registration.host
          ? { ok: true, value: null }
          : {
              ok: false,
              error: {
                code: "already_exists",
                message: "coordinator profile is already registered to another host",
                retry: "never",
              },
            };
      byRef.set(registration.profileRef, registration);
      return { ok: true, value: null };
    },
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
