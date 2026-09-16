/** Authenticated managed-work command bridge. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { z } from "zod";
import {
  ManagedWorkViewSchema,
  type WorkspaceAccessContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceReadRequest,
  type WorkspaceReadResponse,
  type WorkspaceResult,
  ArtifactRefIdSchema,
  AgentDescriptorSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import type { ManagedAgentBackend, ManagedWorkClaim } from "../managed-work/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";
import { workspaceFailure } from "./errors.ts";

export function readManagedWork(
  backend: ManagedAgentBackend | undefined,
  access: WorkspaceAccessContext,
  request: Extract<
    WorkspaceReadRequest,
    {
      operation: "managed-work.profile.list.v1" | "managed-work.get.v1" | "managed-work.list.v1";
    }
  >,
): WorkspaceResult<WorkspaceReadResponse> {
  if (backend === undefined)
    return workspaceFailure("unavailable", "managed work service is not configured");
  if (request.operation === "managed-work.profile.list.v1") {
    const profiles = backend.profiles(access, request.projectId, request.contextId);
    if (!profiles.ok) return managedFailure(profiles);
    return {
      ok: true,
      value: {
        operation: request.operation,
        profiles: profiles.value.map((profile) => ({
          profileId: profile.profileId,
          tier: profile.tier,
          projectId: ProjectIdSchema.parse(profile.projectId),
          contextId: WorkContextIdSchema.parse(profile.contextId),
          provider: profile.provider,
          modelId: profile.modelId,
          effort: profile.effort,
          installed: profile.capabilities.installed,
          launchable: profile.capabilities.launchable,
          interactiveTerminal: profile.capabilities.interactiveTerminal,
          authenticated: profile.capabilities.authenticated,
          evidence: profile.capabilities.evidence,
        })),
      },
    };
  }
  if (request.operation === "managed-work.get.v1") {
    const result = backend.get(access, request.runId);
    return result.ok
      ? { ok: true, value: { operation: request.operation, work: managedWorkView(result.value) } }
      : managedFailure(result);
  }
  const result = backend.list(access, request.projectId, request.contextId);
  return result.ok
    ? {
        ok: true,
        value: { operation: request.operation, works: result.value.map(managedWorkView) },
      }
    : managedFailure(result);
}

export async function commandManagedWork(
  backend: ManagedAgentBackend | undefined,
  store: WorkspaceStore,
  access: WorkspaceAccessContext,
  request: WorkspaceCommandRequest,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  if (backend === undefined)
    return workspaceFailure("unavailable", "managed work service is not configured");
  if (request.operation === "managed-work.create.v1") {
    const { operation, ...raw } = request;
    void operation;
    const result = await backend.prepare(access, {
      ...raw,
      contextRefs: (raw.contextRefs ?? []).map((value) => ArtifactRefIdSchema.parse(value)),
      parentTaskId: raw.parentTaskId ?? null,
      parentRunId: raw.parentRunId ?? null,
      projectedParentActorId: access.actorId,
      planId: raw.planId ?? null,
      workspaceRequest: raw.workspaceRequest ?? { mode: "inherit" },
      sourceBasisRef: raw.sourceBasisRef ?? `lens.user.${raw.clientRequestId}`,
      planRevision: raw.planRevision ?? null,
      depth: raw.depth ?? 0,
      budgets: raw.budgets ?? { maximumTurns: 64, wallTimeMs: 3_600_000 },
    });
    return result.ok
      ? projectManagedWorkClaim(store, request.operation, result.value)
      : managedFailure(result);
  }
  if (
    request.operation !== "managed-work.start.v1" &&
    request.operation !== "managed-work.stop.v1" &&
    request.operation !== "managed-work.continue.v1" &&
    request.operation !== "managed-work.interrupt.v1" &&
    request.operation !== "managed-work.report.v1" &&
    request.operation !== "managed-work.review.v1"
  )
    return workspaceFailure("invalid_input", "managed work command is not recognized");
  if (request.operation === "managed-work.start.v1") {
    const execution = store.readProjectExecution(request.projectId, request.contextId);
    if (!execution.ok) return execution;
    if (execution.value.state !== "uninitialized" && execution.value.state !== "running")
      return workspaceFailure("conflict", "project execution gate blocks managed work start");
  }
  const result =
    request.operation === "managed-work.start.v1"
      ? await backend.start(access, request.runId, request.expectedRevision)
      : request.operation === "managed-work.stop.v1"
        ? await backend.stop(access, request.runId, request.expectedRevision)
        : request.operation === "managed-work.continue.v1"
          ? await backend.continueRun(access, request.runId, request.expectedRevision)
          : request.operation === "managed-work.interrupt.v1"
            ? await backend.interrupt(access, request.runId, request.expectedRevision)
            : request.operation === "managed-work.report.v1"
              ? await backend.report(access, request.runId, request.expectedRevision, {
                  summaryMarkdown: request.summaryMarkdown,
                  artifactRefs: request.artifactRefs.map((value) =>
                    ArtifactRefIdSchema.parse(value),
                  ),
                })
              : await backend.review(access, request.runId, request.expectedRevision, {
                  disposition: request.disposition,
                  commentMarkdown: request.commentMarkdown,
                });
  return result.ok
    ? projectManagedWorkClaim(store, request.operation, result.value)
    : managedFailure(result);
}

export function projectManagedWorkClaim(
  store: WorkspaceStore,
  operation: Extract<WorkspaceCommandRequest, { operation: `managed-work.${string}` }>["operation"],
  claim: ManagedWorkClaim,
): WorkspaceResult<WorkspaceCommandResponse> {
  const actor = AgentDescriptorSchema.parse({
    actorId: ActorIdSchema.parse(claim.actorId),
    sessionId: claim.sessionId,
    projectId: claim.packet.projectId,
    contextId: claim.packet.contextId,
    planId: claim.packet.planId,
    workspaceAssignment: claim.packet.workspaceAssignment,
    role: "worker",
    parentActorId: claim.parentActorId === null ? null : ActorIdSchema.parse(claim.parentActorId),
    displayName: claim.packet.goal.slice(0, 256),
    executionMode: "managed",
    hostId: ExecutionHostIdSchema.parse("host.wayfinder.managed"),
    nativeRef: null,
    terminalId: claim.terminalId,
    state:
      claim.state === "running" || claim.state === "launching" || claim.state === "waiting_for_user"
        ? "active"
        : claim.state === "stopped" || claim.state === "failed" || claim.state === "accepted"
          ? "stopped"
          : "starting",
    revision: DecimalSchema.parse(claim.revision),
  });
  const projected = store.upsertAgent(actor);
  if (!projected.ok) return projected;
  const work = managedWorkView(claim);
  const history = store.ingestEvent({
    projectId: claim.packet.projectId,
    contextId: claim.packet.contextId,
    kind: operation,
    source: "lens",
    actorId: ActorIdSchema.parse(claim.actorId),
    occurrenceAt: new Date().toISOString(),
    sourceEventId: `managed-work:${operation}:${claim.runId}:${claim.revision}`,
    correlationId: null,
    causationId: null,
    planProvenance: null,
    sourceSequence: null,
    payload: { operation, work },
  });
  if (!history.ok) return history;
  return { ok: true, value: { operation, work } };
}

export function managedWorkView(claim: ManagedWorkClaim) {
  return ManagedWorkViewSchema.parse({
    taskId: claim.taskId,
    runId: claim.runId,
    attemptId: claim.attemptId,
    actorId: claim.actorId,
    adapterSessionId: claim.adapterSessionId,
    sessionId: claim.sessionId,
    terminalId: claim.terminalId,
    projectId: claim.packet.projectId,
    contextId: claim.packet.contextId,
    planId: claim.packet.planId,
    workspaceAssignment: claim.packet.workspaceAssignment,
    provider: claim.provider,
    profileId: claim.profileId,
    goal: claim.packet.goal,
    expectedResult: claim.packet.expectedResult,
    targetRefs: claim.targetRefs,
    specialization: claim.packet.specialization,
    modelSelection: claim.modelSelection,
    executionSelection: claim.executionSelection,
    managedControl:
      claim.managedControl === null
        ? null
        : {
            ...claim.managedControl,
            automationControlEpoch: DecimalSchema.parse(
              claim.managedControl.automationControlEpoch,
            ),
          },
    state: claim.state,
    processExit: claim.processExit,
    report: claim.report,
    review: claim.review,
    revision: claim.revision,
  });
}

function managedFailure(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}) {
  const code = z
    .enum(["invalid_input", "forbidden", "conflict", "unavailable"])
    .catch("unavailable")
    .parse(result.error.code);
  return workspaceFailure(code, result.error.message);
}
