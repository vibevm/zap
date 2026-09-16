/** Wayfinder annotation runtime composition. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import { createHash } from "node:crypto";
import type { ManagedAgentBackend, WorkAttachmentPort } from "../managed-work/index.ts";
import type {
  WorkspacePlanningFeature,
  WorkspacePlanningSourceObserver,
} from "../workspace-planning/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import {
  type AnnotationCommandRequest,
  type AnnotationTargetSnapshot,
  type ProjectObjectReference,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import { ClientRequestIdSchema, JsonValueSchema, PrincipalIdSchema } from "../protocol/index.ts";
import { ClientIdSchema } from "../workspace-model/index.ts";
import { QuicklensRefSchema } from "../quicklens-model/index.ts";
import {
  createAnnotationService,
  createAnnotationWorkAttachmentPort,
  openAnnotationStore,
  type AnnotationNotificationPort,
  type AnnotationRestoreIntentPort,
  type AnnotationResult,
  type AnnotationService,
  type AnnotationStore,
  type AnnotationTargetResolver,
} from "../workspace-annotations/index.ts";
export type {
  AnnotationNotificationPort,
  AnnotationRestoreIntentPort,
} from "../workspace-annotations/index.ts";

export interface AnnotationHistoryRecorder {
  record(input: {
    readonly kind: "annotation_changed" | "annotation_source_archived";
    readonly projectId: string;
    readonly contextId: string;
    readonly sourceEventId: string;
  }): void;
}

export interface WayfinderAnnotationsRuntime {
  readonly store: AnnotationStore;
  readonly service: AnnotationService;
  readonly attachments: ReturnType<typeof createAnnotationWorkAttachmentPort>;
  close(): void;
}

export interface WayfinderAnnotationsRuntimeOptions {
  readonly databasePath: string;
  readonly workspaceStore: WorkspaceStore;
  readonly planning?: WorkspacePlanningFeature;
  readonly managedWork?: ManagedAgentBackend;
  readonly notifications: AnnotationNotificationPort;
  readonly restoreIntent?: AnnotationRestoreIntentPort;
  readonly history?: AnnotationHistoryRecorder;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}

export interface WayfinderAnnotationBindings {
  readonly attachments: WorkAttachmentPort;
  readonly sourceObserver: WorkspacePlanningSourceObserver;
  bind(runtime: WayfinderAnnotationsRuntime): void;
}

export function createWayfinderAnnotationBindings(
  workspaceStore: WorkspaceStore,
): WayfinderAnnotationBindings {
  let runtime: WayfinderAnnotationsRuntime | undefined;
  const pending = new Map<string, Parameters<WorkspacePlanningSourceObserver["observe"]>[0]>();
  const observe = (input: Parameters<WorkspacePlanningSourceObserver["observe"]>[0]): void => {
    if (runtime === undefined) {
      pending.set(`${input.projectId}\u0000${input.contextId}`, input);
      return;
    }
    observePlanningSnapshot(workspaceStore, runtime, input);
  };
  return {
    attachments: {
      async prepareBeforeWork(input) {
        if (runtime !== undefined) return runtime.attachments.prepareBeforeWork(input);
        return {
          ok: true,
          value: {
            state: input.targets.length === 0 ? "ready" : "waiting_for_target",
            instructions: [],
          },
        };
      },
      async acknowledge(input) {
        if (runtime !== undefined) return runtime.attachments.acknowledge(input);
        return {
          ok: false,
          error: { code: "unavailable", message: "annotation runtime is not bound" },
        };
      },
    },
    sourceObserver: { observe },
    bind(next) {
      runtime = next;
      for (const observation of pending.values())
        observePlanningSnapshot(workspaceStore, next, observation);
      pending.clear();
    },
  };
}

export function openWayfinderAnnotationsRuntime(
  options: WayfinderAnnotationsRuntimeOptions,
): AnnotationResult<WayfinderAnnotationsRuntime> {
  const opened = openAnnotationStore({
    databasePath: options.databasePath,
    ...(options.clock === undefined ? {} : { clock: options.clock }),
    ...(options.idFactory === undefined ? {} : { idFactory: options.idFactory }),
  });
  if (!opened.ok) return opened;
  const resolver = createTrustedTargetResolver(options);
  const restoreIntent = options.restoreIntent ?? createNormalRestoreIntent(options, opened.value);
  const service = createAnnotationService({
    store: opened.value,
    resolver,
    notifications: options.notifications,
    restoreIntent,
  });
  const attachments = createAnnotationWorkAttachmentPort({
    store: opened.value,
    resolver,
    ...(options.clock === undefined ? {} : { clock: options.clock }),
    ...(options.idFactory === undefined ? {} : { idFactory: options.idFactory }),
  });
  const serviceWithHistory: AnnotationService = {
    ...service,
    async command(access, request) {
      const result = await service.command(access, request);
      if (result.ok && options.history !== undefined)
        options.history.record({
          kind: "annotation_changed",
          projectId: request.projectId,
          contextId: request.contextId,
          sourceEventId: annotationEventIdentity(access, request),
        });
      return result;
    },
    observeSource(access, observation) {
      const result = service.observeSource(access, observation);
      if (result.ok && result.value.length > 0 && options.history !== undefined)
        options.history.record({
          kind: "annotation_source_archived",
          projectId: observation.projectId,
          contextId: observation.contextId,
          sourceEventId: scopedIdentity([
            observation.projectId,
            observation.contextId,
            observation.state,
            observation.basisRef,
          ]),
        });
      return result;
    },
  };
  return {
    ok: true,
    value: {
      store: opened.value,
      service: serviceWithHistory,
      attachments,
      close: () => {
        opened.value.close();
      },
    },
  };
}

function createNormalRestoreIntent(
  options: WayfinderAnnotationsRuntimeOptions,
  annotationStore: AnnotationStore,
): AnnotationRestoreIntentPort {
  return {
    async create(input) {
      if (options.planning === undefined)
        return {
          ok: false,
          error: { code: "unavailable", message: "planning restore-intent port is not configured" },
        };
      const trash = annotationStore.readTrash(input.access, input.trashId);
      if (!trash.ok) return trash;
      if (trash.value.revision !== input.expectedRevision)
        return {
          ok: false,
          error: { code: "stale_revision", message: "Trash entry revision changed" },
        };
      if (trash.value.entryKind !== "object")
        return {
          ok: false,
          error: { code: "conflict", message: "only removed objects create restore intents" },
        };
      if (
        trash.value.projectId !== input.projectId ||
        trash.value.contextId !== input.contextId ||
        trash.value.target.projectId !== input.projectId ||
        trash.value.target.contextId !== input.contextId
      )
        return {
          ok: false,
          error: { code: "conflict", message: "Trash entry does not belong to this plan scope" },
        };
      const snapshot = await options.planning.snapshot(
        input.access,
        input.projectId,
        input.contextId,
      );
      if (
        !snapshot.ok ||
        snapshot.value.operation !== "project.snapshot.v1" ||
        snapshot.value.snapshot.state !== "ready" ||
        snapshot.value.snapshot.snapshot.plan === null
      )
        return {
          ok: false,
          error: { code: "unavailable", message: "current plan basis is unavailable" },
        };
      const network = options.workspaceStore.read(input.access, {
        operation: "agent.network.v1",
        projectId: input.projectId,
        contextId: input.contextId,
      });
      if (!network.ok || network.value.operation !== "agent.network.v1")
        return {
          ok: false,
          error: { code: "unavailable", message: "eligible coordinator is unavailable" },
        };
      const coordinator = network.value.network.agents.find(
        (agent) =>
          agent.role === "coordinator" && agent.state !== "failed" && agent.state !== "stopped",
      );
      if (coordinator === undefined)
        return {
          ok: false,
          error: { code: "unavailable", message: "eligible coordinator is unavailable" },
        };
      const command = await options.planning.command(input.access, {
        operation: "plan.intent.v1",
        clientRequestId: ClientRequestIdSchema.parse(
          `request.annotation.restore.${input.trashId}.${input.expectedRevision}`,
        ),
        projectId: input.projectId,
        contextId: input.contextId,
        input: {
          text: `Restore removed object ${trash.value.target.domain}/${trash.value.target.ref} through the current plan authority. Archived source basis: ${trash.value.sourceBasisRef}. Archived snapshot: ${JSON.stringify(trash.value.snapshot)}.`,
          basis: snapshot.value.snapshot.snapshot.plan.basis,
          targetActorRef: QuicklensRefSchema.parse(coordinator.actorId),
        },
      });
      if (!command.ok || command.value.operation !== "plan.intent.v1")
        return {
          ok: false,
          error: { code: "unavailable", message: "restore intent was not accepted" },
        };
      return { ok: true, value: { intentRef: command.value.result.operationRef } };
    },
  };
}

function createTrustedTargetResolver(
  options: WayfinderAnnotationsRuntimeOptions,
): AnnotationTargetResolver {
  return {
    async resolve(target) {
      if (target.domain === "project" && target.ref !== target.projectId)
        return { state: "missing" };
      const access = accessFor(options.workspaceStore, target);
      if (access === null)
        return { state: "unavailable", reason: "trusted target access is unavailable" };
      if (target.domain === "semantic_object" || target.domain === "semantic_relationship") {
        if (options.planning === undefined)
          return { state: "unavailable", reason: "planning snapshot resolver is not configured" };
        const snapshot = await options.planning.snapshot(
          access,
          target.projectId,
          target.contextId,
        );
        if (
          !snapshot.ok ||
          snapshot.value.operation !== "project.snapshot.v1" ||
          snapshot.value.snapshot.state !== "ready"
        )
          return { state: "unavailable", reason: "complete planning snapshot is unavailable" };
        const value = snapshot.value.snapshot.snapshot;
        const found =
          target.domain === "semantic_object"
            ? value.objects.find((object) => object.ref === target.ref)
            : value.relationships.find((relationship) => relationship.ref === target.ref);
        return found === undefined
          ? { state: "missing" }
          : {
              state: "present",
              snapshot: snapshotOf(snapshot.value.snapshot.snapshot.revision, found),
            };
      }
      if (target.domain === "project") {
        const found = options.workspaceStore.read(access, {
          operation: "project.get.v1",
          projectId: target.projectId,
        });
        return !found.ok || found.value.operation !== "project.get.v1"
          ? { state: "missing" }
          : {
              state: "present",
              snapshot: snapshotOf(found.value.detail.project.revision, found.value.detail),
            };
      }
      if (target.domain === "agent") {
        const found = options.workspaceStore.read(access, {
          operation: "agent.network.v1",
          projectId: target.projectId,
          contextId: target.contextId,
        });
        if (!found.ok || found.value.operation !== "agent.network.v1")
          return { state: "unavailable", reason: "agent network is unavailable" };
        const agent = found.value.network.agents.find(
          (candidate) => candidate.actorId === target.ref,
        );
        return agent === undefined
          ? { state: "missing" }
          : { state: "present", snapshot: snapshotOf(agent.revision, agent) };
      }
      if (options.managedWork === undefined)
        return { state: "unavailable", reason: "managed work resolver is not configured" };
      const claims = options.managedWork.list(access, target.projectId, target.contextId);
      if (!claims.ok) return { state: "unavailable", reason: claims.error.message };
      const claim = claims.value.find((candidate) =>
        target.domain === "work_task"
          ? candidate.taskId === target.ref
          : candidate.runId === target.ref,
      );
      return claim === undefined
        ? { state: "missing" }
        : { state: "present", snapshot: snapshotOf(claim.revision, claim) };
    },
  };
}

function snapshotOf(basisRef: string, value: unknown): AnnotationTargetSnapshot {
  return { basisRef, capturedAt: new Date().toISOString(), value: JsonValueSchema.parse(value) };
}

function accessFor(
  store: WorkspaceStore,
  target: ProjectObjectReference,
): WorkspaceAccessContext | null {
  const access: WorkspaceAccessContext = {
    principalId: PrincipalIdSchema.parse("principal.annotation.runtime"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.annotation.runtime"),
    authorizedProjectIds: [target.projectId],
  };
  const detail = store.read(
    {
      ...access,
    },
    { operation: "context.get.v1", projectId: target.projectId, contextId: target.contextId },
  );
  return detail.ok ? access : null;
}

function annotationEventIdentity(
  access: WorkspaceAccessContext,
  request: AnnotationCommandRequest,
): string {
  const note = "noteId" in request ? request.noteId : "new";
  const revision = "expectedRevision" in request ? request.expectedRevision : "0";
  return scopedIdentity([
    access.principalId,
    access.actorId ?? `client:${access.clientId}`,
    request.projectId,
    request.contextId,
    request.operation,
    request.clientRequestId,
    note,
    revision,
  ]);
}

function scopedIdentity(parts: readonly string[]): string {
  return createHash("sha256").update(parts.join("\u0000")).digest("hex");
}

function observePlanningSnapshot(
  workspaceStore: WorkspaceStore,
  runtime: WayfinderAnnotationsRuntime,
  input: Parameters<WorkspacePlanningSourceObserver["observe"]>[0],
): void {
  const projectTarget: ProjectObjectReference = {
    projectId: input.projectId,
    contextId: input.contextId,
    domain: "project",
    ref: input.projectId,
  };
  const access = accessFor(workspaceStore, projectTarget);
  if (access === null) return;
  const basisRef =
    input.snapshot.plan?.basis.sourceBasisRef ?? `snapshot:${input.snapshot.revision}`;
  const authoritative = input.snapshot.sourceMode === "live" && input.snapshot.phase === "ready";
  const objects = input.snapshot.objects.map((value) => ({
    target: {
      projectId: input.projectId,
      contextId: input.contextId,
      domain: "semantic_object" as const,
      ref: value.ref,
    },
    snapshot: {
      basisRef,
      capturedAt: input.snapshot.capturedAt,
      value: JsonValueSchema.parse(value),
    },
  }));
  const relationships = input.snapshot.relationships.map((value) => ({
    target: {
      projectId: input.projectId,
      contextId: input.contextId,
      domain: "semantic_relationship" as const,
      ref: value.ref,
    },
    snapshot: {
      basisRef,
      capturedAt: input.snapshot.capturedAt,
      value: JsonValueSchema.parse(value),
    },
  }));
  runtime.service.observeSource(access, {
    projectId: input.projectId,
    contextId: input.contextId,
    state: authoritative ? "authoritative_full" : "partial",
    basisRef,
    observedAt: input.snapshot.capturedAt,
    presentTargets: [...objects, ...relationships].map(({ target }) => target),
    removedTargets: [],
    snapshots: [...objects, ...relationships],
    coveredDomains: ["semantic_object", "semantic_relationship"],
  });
}
