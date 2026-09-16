/** Authenticated managed actor work tools. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { createHash } from "node:crypto";
import type { z } from "zod";
import {
  ManagedAgentAttachmentAckInputSchema,
  ManagedAgentCreateInputSchema,
  ManagedAgentReadInputSchema,
  ManagedAgentReportInputSchema,
  ManagedAgentStartInputSchema,
  ManagedWorkRequestSchema,
  type ManagedAgentBackend,
  type ManagedWorkAgentPort,
  type ManagedWorkClaim,
  type ManagedWorkResult,
} from "../managed-work/index.ts";
import { JsonValueSchema, type PublicConnection, type Result } from "../protocol/index.ts";
import { failure, type AdapterSessionId, type AgentTransportPort } from "../transport/index.ts";
import {
  ClientIdSchema,
  WorkspaceAccessContextSchema,
  type ProjectId,
  type WorkContextId,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import {
  managedWorkView,
  projectManagedWorkClaim,
  type OwnedCoordinatorAgentPort,
} from "../workspace-service/index.ts";

export function createManagedWorkAgentPort(options: {
  readonly agent: AgentTransportPort;
  readonly backend: () => ManagedAgentBackend | undefined;
  readonly store: WorkspaceStore;
  readonly coordinatorAgents: OwnedCoordinatorAgentPort;
  readonly ensurePlan?: ManagedRepositoryPlanPort;
}): ManagedWorkAgentPort {
  const bound = async (session: AdapterSessionId) => {
    const actor = await options.agent.context(session);
    if (!actor.ok) return actor;
    const scope = options.store.resolveAgentScope(
      actor.value.actor.workspaceId,
      actor.value.actor.conversationId,
    );
    if (!scope.ok)
      return failure("forbidden", "managed actor has no exact registered project scope");
    const backend = options.backend();
    if (backend === undefined)
      return failure("unsupported_operation", "managed work runtime is not configured");
    const access = WorkspaceAccessContextSchema.parse({
      principalId: actor.value.actor.principalId,
      actorId: actor.value.actor.actorId,
      clientId: ClientIdSchema.parse("client.managed-agent." + digest(actor.value.actor.actorId)),
      authorizedProjectIds: [scope.value.projectId],
    });
    return {
      ok: true as const,
      value: { actor: actor.value, scope: scope.value, backend, access },
    };
  };
  return {
    async profiles(session) {
      const context = await bound(session);
      if (!context.ok) return context;
      return managedJson(
        context.value.backend.profiles(
          context.value.access,
          context.value.scope.projectId,
          context.value.scope.contextId,
        ),
      );
    },
    async create(session, raw) {
      const input = ManagedAgentCreateInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "managed create input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const parent = callerParent(
        context.value.actor,
        context.value.scope.contextId,
        context.value.backend,
        context.value.access,
        options.coordinatorAgents,
      );
      if (!parent.ok) return parent;
      let planId =
        parent.value.claim?.packet.planId ??
        contextPlanId(
          options.store,
          context.value.access,
          context.value.scope.projectId,
          context.value.scope.contextId,
        );
      if (planId === null && input.data.workspace.mode !== "inherit") {
        if (options.ensurePlan === undefined)
          return failure("unsupported_operation", "repository plan adoption is not configured");
        const ensured = await options.ensurePlan({
          access: context.value.access,
          projectId: context.value.scope.projectId,
          contextId: context.value.scope.contextId,
        });
        if (!ensured.ok) return ensured;
        planId = ensured.value;
      }
      if (
        input.data.targetRefs.some(
          (target) =>
            target.projectId !== context.value.scope.projectId ||
            target.contextId !== context.value.scope.contextId,
        )
      )
        return failure("forbidden", "managed targets are outside the caller project scope");
      const { workspace, ...managedInput } = input.data;
      const request = ManagedWorkRequestSchema.parse({
        ...managedInput,
        projectId: context.value.scope.projectId,
        contextId: context.value.scope.contextId,
        planId,
        workspaceRequest: workspace,
        parentTaskId: parent.value.claim?.taskId ?? null,
        parentRunId: parent.value.claim?.runId ?? null,
        projectedParentActorId: parent.value.parentActorId,
        sourceBasisRef: input.data.sourceBasisRef ?? "agent." + input.data.clientRequestId,
        depth: parent.value.claim === null ? 0 : parent.value.claim.packet.depth + 1,
      });
      const prepared = await context.value.backend.prepare(context.value.access, request);
      return prepared.ok
        ? projectedWork(options.store, "managed-work.create.v1", prepared.value)
        : managedFailure(prepared);
    },
    async start(session, raw) {
      const input = ManagedAgentStartInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "managed start input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const claim = context.value.backend.get(context.value.access, input.data.runId);
      if (!claim.ok) return managedFailure(claim);
      if (!supervises(context.value.actor, claim.value))
        return failure("forbidden", "caller cannot start this managed run");
      const started = await context.value.backend.start(
        context.value.access,
        input.data.runId,
        input.data.expectedRevision,
      );
      return started.ok
        ? projectedWork(options.store, "managed-work.start.v1", started.value)
        : managedFailure(started);
    },
    async read(session, raw) {
      const input = ManagedAgentReadInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "managed read input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const claim = context.value.backend.get(context.value.access, input.data.runId);
      return claim.ok &&
        (claim.value.actorId === context.value.actor.actor.actorId ||
          supervises(context.value.actor, claim.value))
        ? json(managedWorkView(claim.value))
        : claim.ok
          ? failure("forbidden", "caller cannot read this managed run")
          : managedFailure(claim);
    },
    async report(session, raw) {
      const input = ManagedAgentReportInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "managed report input is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const claim = context.value.backend.get(context.value.access, input.data.runId);
      if (!claim.ok) return managedFailure(claim);
      if (claim.value.actorId !== context.value.actor.actor.actorId)
        return failure("forbidden", "worker can report only its own managed run");
      const reported = await context.value.backend.report(
        context.value.access,
        input.data.runId,
        input.data.expectedRevision,
        {
          summaryMarkdown: input.data.summaryMarkdown,
          artifactRefs: input.data.artifactRefs,
        },
      );
      return reported.ok
        ? projectedWork(options.store, "managed-work.report.v1", reported.value)
        : managedFailure(reported);
    },
    async acknowledgeAttachment(session, raw) {
      const input = ManagedAgentAttachmentAckInputSchema.safeParse(raw);
      if (!input.success)
        return failure("invalid_input", "managed attachment acknowledgement is invalid");
      const context = await bound(session);
      if (!context.ok) return context;
      const claim = context.value.backend.get(context.value.access, input.data.runId);
      if (!claim.ok) return managedFailure(claim);
      if (
        claim.value.actorId !== context.value.actor.actor.actorId ||
        claim.value.attemptId !== input.data.attemptId
      )
        return failure("forbidden", "attachment acknowledgement is outside worker attempt");
      return managedJson(
        await context.value.backend.acknowledgeAttachment(
          context.value.access,
          input.data.runId,
          input.data.attemptId,
          input.data.attachmentId,
          input.data.version,
        ),
      );
    },
  };
}

export type ManagedRepositoryPlanPort = (input: {
  readonly access: WorkspaceAccessContext;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
}) => Promise<Result<string>>;

function contextPlanId(
  store: WorkspaceStore,
  access: WorkspaceAccessContext,
  projectId: ProjectId,
  contextId: WorkContextId,
) {
  const detail = store.read(access, { operation: "project.get.v1", projectId });
  if (!detail.ok || detail.value.operation !== "project.get.v1") return null;
  return (
    detail.value.detail.contexts.find((context) => context.contextId === contextId)?.planId ?? null
  );
}

function callerParent(
  actor: PublicConnection,
  contextId: string,
  backend: ManagedAgentBackend,
  access: WorkspaceAccessContext,
  coordinatorAgents: OwnedCoordinatorAgentPort,
): Result<{ readonly claim: ManagedWorkClaim | null; readonly parentActorId: string | null }> {
  const projectId = access.authorizedProjectIds[0];
  if (projectId === undefined) return failure("forbidden", "managed project scope is empty");
  const scope = backend.list(access, projectId, contextId);
  if (!scope.ok) return managedFailure(scope);
  const parent = scope.value.find((claim) => claim.actorId === actor.actor.actorId) ?? null;
  if (parent === null) {
    if (actor.actor.parentActorId !== null || !actor.actor.capabilities.includes("plan:propose"))
      return failure("forbidden", "caller is not a managed worker or root coordinator");
    const routed = coordinatorAgents.route(actor.actor.actorId);
    return routed.ok && routed.value !== null
      ? {
          ok: true,
          value: { claim: null, parentActorId: routed.value.coordinatorActorId },
        }
      : failure("forbidden", "root coordinator projection is unavailable");
  }
  return ["running", "waiting_for_user"].includes(parent.state)
    ? { ok: true, value: { claim: parent, parentActorId: parent.actorId } }
    : failure("conflict", "managed parent is not active");
}

function supervises(actor: PublicConnection, claim: ManagedWorkClaim): boolean {
  return (
    claim.creatorActorId === actor.actor.actorId || claim.parentActorId === actor.actor.actorId
  );
}

function managedJson<T>(result: ManagedWorkResult<T>): Result<z.infer<typeof JsonValueSchema>> {
  return result.ok ? json(result.value) : managedFailure(result);
}
function projectedWork(
  store: WorkspaceStore,
  operation: "managed-work.create.v1" | "managed-work.start.v1" | "managed-work.report.v1",
  claim: ManagedWorkClaim,
): Result<z.infer<typeof JsonValueSchema>> {
  const projected = projectManagedWorkClaim(store, operation, claim);
  return projected.ok && "work" in projected.value
    ? json(projected.value.work)
    : projected.ok
      ? failure("storage_failure", "managed work projection returned no public work view")
      : managedFailure(projected);
}
function json(value: unknown): Result<z.infer<typeof JsonValueSchema>> {
  const parsed = JsonValueSchema.safeParse(value);
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("storage_failure", "managed work result is not JSON-safe");
}
function managedFailure(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}): Result<never> {
  const code =
    result.error.code === "forbidden" || result.error.code === "conflict"
      ? result.error.code
      : result.error.code === "invalid_input"
        ? "invalid_input"
        : "storage_failure";
  return failure(code, result.error.message);
}
function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 32);
}
