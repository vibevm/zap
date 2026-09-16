/** Shared repository workspace HTTP/service feature. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { createHash } from "node:crypto";
import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  JsonValueSchema,
  PrincipalIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import type { ProductAppService } from "../product-app/index.ts";
import type { AnnotationService } from "../workspace-annotations/index.ts";
import type { IntegrationAttempt } from "../repository-model/index.ts";
import type { RepositoryWorkspaceResult } from "../repository-workspaces/index.ts";
import {
  ProductPlanContextSchema,
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import {
  TrustedPlanContextRegistrationSchema,
  type WorkspaceStore,
} from "../workspace-store/index.ts";
import {
  type RepositoryWorkspaceCommandRequest,
  type RepositoryWorkspaceFeature,
} from "../workspace-service/index.ts";
import type { RuntimeRepositoryWorkspaces } from "./repository-runtime.ts";
import type { RepositoryResolutionPreparationPort } from "./repository-resolution.ts";
import { canOpenRepositoryWriter, ensureRepository } from "./repository-context.ts";
import { observeRepositoryAnnotationTargets } from "./repository-annotations.ts";
import { readRuntimeRepository } from "./repository-reads.ts";
export { ensureRuntimeRepositoryContextPlan } from "./repository-context.ts";

export function createRuntimeRepositoryFeature(input: {
  readonly runtime: RuntimeRepositoryWorkspaces;
  readonly store: WorkspaceStore;
  readonly product: ProductAppService;
  readonly annotations?: AnnotationService;
  readonly resolution?: RepositoryResolutionPreparationPort;
}): RepositoryWorkspaceFeature {
  return {
    canOpenWriter: (accessContext, projectId, contextId) =>
      publicRepositoryResult(canOpenRepositoryWriter(input, accessContext, projectId, contextId)),
    read: async (access, request) => {
      await Promise.resolve();
      if (!access.authorizedProjectIds.includes(request.projectId))
        return workspaceFailure("forbidden", "repository read is outside project scope");
      const response = await readRuntimeRepository(input, access.principalId, request);
      if (response.ok && "contextId" in request)
        observeRepositoryAnnotationTargets({
          annotations: input.annotations,
          runtime: input.runtime,
          access,
          projectId: request.projectId,
          contextId: request.contextId,
        });
      return publicRepositoryResult(response);
    },
    command: async (access, request) => {
      if (!access.authorizedProjectIds.includes(request.projectId))
        return workspaceFailure("forbidden", "repository command is outside project scope");
      const response = await command(input, access, request);
      if (response.ok) {
        observeRepositoryAnnotationTargets({
          annotations: input.annotations,
          runtime: input.runtime,
          access,
          projectId: request.projectId,
          contextId: request.contextId,
        });
        const payload = JsonValueSchema.safeParse({
          response: response.value,
          requestedByPrincipalId: access.principalId,
        });
        if (payload.success)
          input.store.ingestEvent({
            projectId: request.projectId,
            contextId: request.contextId,
            kind: request.operation,
            source: "lens",
            actorId: access.actorId,
            occurrenceAt: new Date().toISOString(),
            sourceEventId: `repository-command:${request.clientRequestId}`,
            sourceSequence: null,
            correlationId: correlation(response.value),
            causationId: null,
            planProvenance: null,
            payload: payload.data,
          });
      }
      return publicRepositoryResult(response);
    },
  };
}

async function command(
  input: Parameters<typeof createRuntimeRepositoryFeature>[0],
  accessContext: ReturnType<typeof WorkspaceAccessContextSchema.parse>,
  request: RepositoryWorkspaceCommandRequest,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const principalId = accessContext.principalId;
  if (request.operation === "plan.workspace.prepare.v1")
    return preparePlan(input, principalId, request);
  if (request.operation === "worktree.prepare.v1") {
    const result = await input.runtime.service.prepareChildWorktree({
      requestId: request.clientRequestId,
      principalId,
      executionHostId: input.runtime.executionHostId,
      projectId: request.projectId,
      contextId: request.contextId,
      planId: request.planId,
      parentWorktreeId: request.parentWorktreeId,
      expectedParentHead: request.expectedParentHead,
    });
    return result.ok
      ? { ok: true, value: { operation: request.operation, worktree: result.value } }
      : repositoryFailure(result);
  }
  if (request.operation === "integration.prepare.v1") {
    const scoped = planInScope(input, request.planId, request.projectId, request.contextId);
    if (!scoped.ok) return scoped;
    const source = await syncWorktreeHead(
      input,
      principalId,
      `${request.clientRequestId}.source-head`,
      request.projectId,
      request.contextId,
      request.planId,
      "source",
      request.sourceWorktreeId,
      request.expectedSourceHead,
    );
    if (!source.ok) return source;
    const target = await syncWorktreeHead(
      input,
      principalId,
      `${request.clientRequestId}.target-head`,
      request.projectId,
      request.contextId,
      request.planId,
      "target",
      request.targetWorktreeId,
      request.expectedTargetHead,
    );
    if (!target.ok) return target;
    const result = await input.runtime.service.prepareIntegration({
      requestId: request.clientRequestId,
      principalId,
      executionHostId: input.runtime.executionHostId,
      integrationId: stableId("integration", request.clientRequestId),
      planId: request.planId,
      sourceWorktreeId: request.sourceWorktreeId,
      targetWorktreeId: request.targetWorktreeId,
      expectedSourceHead: request.expectedSourceHead,
      expectedTargetHead: request.expectedTargetHead,
    });
    return integrationResponse(request.operation, result);
  }
  const integration = input.runtime.service.getIntegration(request.integrationId);
  if (!integration.ok) return repositoryFailure(integration);
  const scoped = planInScope(input, integration.value.planId, request.projectId, request.contextId);
  if (!scoped.ok) return scoped;
  if (request.operation === "integration.resolution.prepare.v1")
    return input.resolution === undefined
      ? workspaceFailure("unavailable", "integration resolution work is not configured")
      : input.resolution.prepare({
          access: accessContext,
          request,
          integration: integration.value,
        });
  const common = {
    requestId: request.clientRequestId,
    principalId,
    executionHostId: input.runtime.executionHostId,
    integrationId: request.integrationId,
    expectedRevision: request.expectedRevision,
  };
  if (request.operation === "integration.test.v1")
    return integrationResponse(
      request.operation,
      await input.runtime.service.runIntegrationTest({ ...common, profileId: request.profileId }),
    );
  if (request.operation === "integration.review.v1")
    return integrationResponse(
      request.operation,
      await input.runtime.service.recordIntegrationReview({
        ...common,
        accepted: request.accepted,
        rationale: request.rationale,
      }),
    );
  return integrationResponse(
    request.operation,
    await input.runtime.service.promoteIntegration(common),
  );
}

function planInScope(
  input: Parameters<typeof createRuntimeRepositoryFeature>[0],
  planId: string,
  projectId: string,
  contextId: string,
): WorkspaceResult<null> {
  const plan = input.runtime.service.getPlan(planId);
  if (!plan.ok) return repositoryFailure(plan);
  return plan.value.projectId === projectId && plan.value.contextId === contextId
    ? { ok: true, value: null }
    : workspaceFailure("not_found", "plan workspace is outside requested context");
}

async function syncWorktreeHead(
  input: Parameters<typeof createRuntimeRepositoryFeature>[0],
  principalId: string,
  requestId: string,
  projectId: string,
  contextId: string,
  planId: string,
  role: "source" | "target",
  worktreeId: string,
  expectedHead: string,
): Promise<WorkspaceResult<null>> {
  const worktree = input.runtime.service.getWorktree(worktreeId);
  if (!worktree.ok) return repositoryFailure(worktree);
  const plan = input.runtime.service.getPlan(planId);
  if (!plan.ok) return repositoryFailure(plan);
  const sourceMatches =
    worktree.value.projectId === projectId &&
    worktree.value.contextId === contextId &&
    worktree.value.planId === planId;
  const targetMatches =
    worktree.value.projectId === projectId &&
    worktree.value.repositoryId === plan.value.repositoryId &&
    [plan.value.rootWorktreeId, plan.value.integrationTargetWorktreeId].includes(
      worktree.value.worktreeId,
    );
  if ((role === "source" && !sourceMatches) || (role === "target" && !targetMatches))
    return workspaceFailure("not_found", "repository workspace is outside requested plan scope");
  const observed = await input.runtime.service.observeWorktree({
    worktreeId,
    executionHostId: input.runtime.executionHostId,
    projectId: worktree.value.projectId,
    contextId: worktree.value.contextId,
  });
  if (!observed.ok) return repositoryFailure(observed);
  if (observed.value.currentHead !== expectedHead)
    return workspaceFailure("conflict", "repository workspace HEAD changed");
  if (worktree.value.headCommit === expectedHead) return { ok: true, value: null };
  const recorded = await input.runtime.service.recordWorktreeHead({
    requestId,
    principalId,
    executionHostId: input.runtime.executionHostId,
    worktreeId,
    expectedRevision: worktree.value.revision,
    expectedOldHead: worktree.value.headCommit,
    newHead: expectedHead,
  });
  return recorded.ok ? { ok: true, value: null } : repositoryFailure(recorded);
}

async function preparePlan(
  input: Parameters<typeof createRuntimeRepositoryFeature>[0],
  principalId: string,
  request: Extract<RepositoryWorkspaceCommandRequest, { operation: "plan.workspace.prepare.v1" }>,
): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
  const registered = await ensureRepository(input, principalId, request.projectId);
  if (!registered.ok) return registered;
  const selected = await readRuntimeRepository(input, principalId, {
    operation: "repository.get.v1",
    projectId: request.projectId,
    contextId: request.contextId,
  });
  if (!selected.ok) return selected;
  if (selected.value.operation !== "repository.get.v1")
    return workspaceFailure("storage_failure", "repository read returned another operation");
  if (selected.value.observedContextHead !== request.expectedBaseHead)
    return workspaceFailure("conflict", "selected repository HEAD changed");
  let baseWorktree = selected.value.contextWorktree;
  if (baseWorktree.headCommit !== selected.value.observedContextHead) {
    const recorded = await input.runtime.service.recordWorktreeHead({
      requestId: `${request.clientRequestId}.head`,
      principalId,
      executionHostId: input.runtime.executionHostId,
      worktreeId: baseWorktree.worktreeId,
      expectedRevision: baseWorktree.revision,
      expectedOldHead: baseWorktree.headCommit,
      newHead: selected.value.observedContextHead,
    });
    if (!recorded.ok) return repositoryFailure(recorded);
    baseWorktree = recorded.value;
  }
  const key = digest(`${request.projectId}\u0000${request.clientRequestId}`);
  const planId = `plan.workspace.${key}`;
  const contextId = `context.plan.${key}`;
  const prepared = await input.runtime.service.preparePlanRoot({
    requestId: request.clientRequestId,
    principalId,
    executionHostId: input.runtime.executionHostId,
    projectId: request.projectId,
    contextId,
    planId,
    displayName: request.displayName,
    baseWorktreeId: baseWorktree.worktreeId,
    expectedBaseHead: request.expectedBaseHead,
    algorithmBinding: { state: "pending" },
  });
  if (!prepared.ok) return repositoryFailure(prepared);
  const resolved = await input.runtime.service.resolveExecutionWorkspace({
    projectId: request.projectId,
    contextId,
    worktreeId: prepared.value.worktree.worktreeId,
    executionHostId: input.runtime.executionHostId,
    expectedRevision: prepared.value.worktree.revision,
  });
  if (!resolved.ok) return repositoryFailure(resolved);
  const baseLaunch = input.store.resolveProjectLaunch(request.projectId, request.contextId);
  const baseView = input.store.read(access(principalId, request.projectId), {
    operation: "project.get.v1",
    projectId: request.projectId,
    contextId: request.contextId,
  });
  if (!baseLaunch.ok || !baseView.ok || baseView.value.operation !== "project.get.v1")
    return workspaceFailure("unavailable", "selected context launch binding is unavailable");
  const conversationId = ConversationIdSchema.parse(`conversation.plan.${key}`);
  const registration = TrustedPlanContextRegistrationSchema.parse({
    registrationId: ClientRequestIdSchema.parse(`request.plan-context.${key}`),
    projectId: request.projectId,
    plan: prepared.value.plan,
    rootWorktree: prepared.value.worktree,
    context: {
      displayName: request.displayName,
      workspaceRef: `workspace.plan.${key}`,
      branchLabel: prepared.value.worktree.branchRef,
      revisionBinding: prepared.value.worktree.headCommit,
      planning: { state: "unavailable", reason: "Algorithm plan binding is pending." },
      coordinatorConversationId: conversationId,
      brokerScope: {
        workspaceId: WorkspaceIdSchema.parse(`workspace.plan.${key}`),
        conversationId,
      },
    },
    coordinatorLaunchOptions: baseView.value.detail.coordinatorLaunchOptions,
    protected: {
      cwd: resolved.value.projectCwd,
      launchProfileRef: baseLaunch.value.launchProfileRef,
    },
  });
  const productContext = ProductPlanContextSchema.parse({
    planId,
    contextId,
    displayName: request.displayName,
    profileId: baseLaunch.value.launchProfileRef,
    rootWorktreeId: prepared.value.worktree.worktreeId,
    registeredAt: new Date().toISOString(),
  });
  const productKnown = input.product.projectIds().includes(request.projectId);
  const registeredContext = productKnown
    ? await input.product.registerPlanContext({
        context: productContext,
        registration,
        requestDigest: digest(registration),
      })
    : input.store.registerPlanContext(registration).ok
      ? { ok: true as const, value: null }
      : {
          ok: false as const,
          error: { code: "unavailable" as const, message: "context registration failed" },
        };
  if (!registeredContext.ok)
    return workspaceFailure("unavailable", registeredContext.error.message);
  const context = input.store.read(access(principalId, request.projectId), {
    operation: "context.get.v1",
    projectId: request.projectId,
    contextId: WorkContextIdSchema.parse(contextId),
  });
  return context.ok && context.value.operation === "context.get.v1"
    ? {
        ok: true,
        value: {
          operation: request.operation,
          plan: prepared.value.plan,
          context: context.value.context,
          worktree: prepared.value.worktree,
        },
      }
    : workspaceFailure("storage_failure", "prepared plan context is unavailable");
}

function integrationResponse(
  operation:
    | "integration.prepare.v1"
    | "integration.test.v1"
    | "integration.review.v1"
    | "integration.promote.v1",
  result: RepositoryWorkspaceResult<IntegrationAttempt>,
): WorkspaceResult<WorkspaceCommandResponse> {
  return result.ok
    ? { ok: true, value: { operation, integration: result.value } }
    : repositoryFailure(result);
}

function access(principalId: string, projectId: string) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(principalId),
    actorId: null,
    clientId: ClientIdSchema.parse(`client.repository.${digest(principalId)}`),
    authorizedProjectIds: [ProjectIdSchema.parse(projectId)],
  });
}

function correlation(response: WorkspaceCommandResponse): string {
  if (response.operation === "plan.workspace.prepare.v1") return response.plan.planId;
  if (response.operation === "worktree.prepare.v1") return response.worktree.worktreeId;
  if (
    response.operation === "integration.prepare.v1" ||
    response.operation === "integration.test.v1" ||
    response.operation === "integration.review.v1" ||
    response.operation === "integration.resolution.prepare.v1" ||
    response.operation === "integration.promote.v1"
  )
    return response.integration.integrationId;
  return response.operation;
}

function stableId(kind: string, basis: string): string {
  return `${kind}.${digest(basis)}`;
}

function digest(value: unknown): string {
  return createHash("sha256")
    .update(typeof value === "string" ? value : JSON.stringify(value))
    .digest("hex")
    .slice(0, 24);
}

function repositoryFailure(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}): WorkspaceResult<never> {
  const code =
    result.error.code === "invalid_input"
      ? "invalid_input"
      : result.error.code === "denied"
        ? "forbidden"
        : result.error.code === "conflict" ||
            result.error.code === "stale" ||
            result.error.code === "dirty" ||
            result.error.code === "not_ready" ||
            result.error.code === "conflicted"
          ? "conflict"
          : "unavailable";
  return workspaceFailure(code, result.error.message);
}

function workspaceFailure(
  code:
    | "invalid_input"
    | "forbidden"
    | "not_found"
    | "conflict"
    | "unavailable"
    | "storage_failure",
  message: string,
): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-014#projection: ${message}; fix surface: refresh the exact authenticated repository context and retry`,
    },
  };
}

function publicRepositoryResult<T>(result: WorkspaceResult<T>): WorkspaceResult<T> {
  if (result.ok || result.error.message.startsWith("violates REQ spec://")) return result;
  return {
    ok: false,
    error: {
      ...result.error,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-014#projection: ${result.error.message}; fix surface: refresh the exact authenticated repository context and retry`,
    },
  };
}
