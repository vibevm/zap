/** Repository-backed managed workspace provisioning. @scope spec://org.vibevm.zap/lens/PROP-014#assignment */
import type { IntegrationAttempt } from "../repository-model/index.ts";
import type { RepositoryWorkspaceService } from "../repository-workspaces/index.ts";
import {
  ManagedWorkspaceAssignmentSchema,
  type ManagedWorkspaceAssignment,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import {
  createLegacyManagedWorkspaceProvisioningPort,
  type ManagedWorkspaceLaunchBinding,
  type ManagedWorkspacePreparationInput,
  type ManagedWorkspaceProvisioningPort,
} from "../managed-work/index.ts";

export interface RepositoryManagedWorkspaceOptions {
  readonly repositories: RepositoryWorkspaceService;
  readonly executionHostId: string;
  readonly id: () => string;
  readonly now: () => string;
  readonly authorizeIntegrationResolution: (input: {
    readonly access: WorkspaceAccessContext;
    readonly integration: IntegrationAttempt;
    readonly clientRequestId: string;
    readonly planId: string;
  }) =>
    | { readonly ok: true; readonly value: null }
    | { readonly ok: false; readonly message: string };
}

export function createRepositoryManagedWorkspaceProvisioningPort(
  options: RepositoryManagedWorkspaceOptions,
): ManagedWorkspaceProvisioningPort {
  const legacy = createLegacyManagedWorkspaceProvisioningPort(options);
  return {
    async prepare(input) {
      if (input.request.planId === null) return legacy.prepare(input);
      const planResult = options.repositories.getPlan(input.request.planId);
      if (!planResult.ok) return unavailable(planResult.error.message);
      const plan = planResult.value;
      if (
        plan.projectId !== input.request.projectId ||
        plan.contextId !== input.request.contextId ||
        plan.executionHostId !== options.executionHostId
      )
        return forbidden("managed plan workspace is outside the authenticated work scope");
      if (plan.state !== "ready") return unavailable("managed project plan is not ready");
      const selected = await selectWorktree(options, input, plan.planId, plan.rootWorktreeId);
      if (!selected.ok) return selected;
      const assignmentId = options.id();
      const assignmentInput = {
        requestId: `managed-workspace.${input.runId}`,
        principalId: input.access.principalId,
        executionHostId: options.executionHostId,
        worktreeId: selected.value.worktreeId,
        expectedRevision: selected.value.revision,
        assignmentId,
        attemptId: input.attemptId,
        basisCommit: selected.value.headCommit,
        actorId: input.actorId,
        taskId: input.taskId,
        runId: input.runId,
        semanticTargetRefs: input.request.targetRefs,
      };
      const assigned =
        input.request.workspaceRequest.mode === "integration_resolution"
          ? await options.repositories.assignIntegrationWorktree({
              ...assignmentInput,
              integrationId: input.request.workspaceRequest.integrationId,
              expectedIntegrationRevision:
                input.request.workspaceRequest.expectedIntegrationRevision,
            })
          : await options.repositories.assignWorktree(assignmentInput);
      if (!assigned.ok) return fromRepository(assigned);
      return {
        ok: true,
        value: ManagedWorkspaceAssignmentSchema.parse({
          assignmentId,
          kind: selectionKind(input),
          planId: plan.planId,
          projectId: plan.projectId,
          contextId: plan.contextId,
          repositoryId: assigned.value.repositoryId,
          worktreeId: assigned.value.worktreeId,
          executionHostId: assigned.value.executionHostId,
          basisCommit: selected.value.headCommit,
          initialHead: selected.value.headCommit,
          worktreeRevisionAtAssignment: assigned.value.revision,
          assignedByPrincipalId: input.access.principalId,
          assignedAt: options.now(),
        }),
      };
    },
    resolveInitialLaunch: (input) => resolve(options, legacy, input, "initial"),
    resolveResume: (input) => resolve(options, legacy, input, "resume"),
  };
}

async function selectWorktree(
  options: RepositoryManagedWorkspaceOptions,
  input: ManagedWorkspacePreparationInput,
  planId: string,
  planRootWorktreeId: string,
) {
  const request = input.request.workspaceRequest;
  if (request.mode === "isolated_child") {
    const child = await options.repositories.prepareChildWorktree({
      requestId: `managed-child.${input.runId}`,
      principalId: input.access.principalId,
      executionHostId: options.executionHostId,
      projectId: input.request.projectId,
      contextId: input.request.contextId,
      planId,
      parentWorktreeId: request.parentWorktreeId,
      expectedParentHead: request.expectedParentHead,
    });
    return child.ok ? child : fromRepository(child);
  }
  if (request.mode === "integration_resolution") {
    const integration = options.repositories.getIntegration(request.integrationId);
    if (!integration.ok) return fromRepository(integration);
    if (
      integration.value.planId !== input.request.planId ||
      integration.value.revision !== request.expectedIntegrationRevision
    )
      return conflict("integration resolution basis changed");
    const authorized = options.authorizeIntegrationResolution({
      access: input.access,
      integration: integration.value,
      clientRequestId: input.request.clientRequestId,
      planId,
    });
    if (!authorized.ok) return forbidden(authorized.message);
    const worktree = options.repositories.getWorktree(integration.value.integrationWorktreeId);
    if (!worktree.ok) return fromRepository(worktree);
    const resolved = await options.repositories.resolveIntegrationWorkspace({
      integrationId: integration.value.integrationId,
      projectId: input.request.projectId,
      contextId: input.request.contextId,
      worktreeId: worktree.value.worktreeId,
      executionHostId: options.executionHostId,
      expectedRevision: worktree.value.revision,
    });
    return resolved.ok
      ? { ok: true as const, value: resolved.value.worktree }
      : fromRepository(resolved);
  }
  if (input.parentAssignment !== null && input.parentAssignment.kind !== "legacy_registered") {
    if (
      input.parentAssignment.planId !== planId ||
      input.parentAssignment.projectId !== input.request.projectId ||
      input.parentAssignment.contextId !== input.request.contextId
    )
      return forbidden("parent workspace assignment belongs to another plan");
    const parentWorktreeId = input.parentAssignment.worktreeId;
    if (parentWorktreeId === null) return forbidden("parent workspace assignment is incomplete");
    const parent = options.repositories.getWorktree(parentWorktreeId);
    return parent.ok ? parent : fromRepository(parent);
  }
  const root = options.repositories.getWorktree(planRootWorktreeId);
  return root.ok ? root : fromRepository(root);
}

async function resolve(
  options: RepositoryManagedWorkspaceOptions,
  legacy: ManagedWorkspaceProvisioningPort,
  input: Parameters<ManagedWorkspaceProvisioningPort["resolveInitialLaunch"]>[0],
  mode: "initial" | "resume",
): Promise<
  | { readonly ok: true; readonly value: ManagedWorkspaceLaunchBinding }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "invalid_input" | "forbidden" | "conflict" | "unavailable" | "uncertain";
        readonly message: string;
      };
    }
> {
  if (input.assignment.kind === "legacy_registered")
    return mode === "initial" ? legacy.resolveInitialLaunch(input) : legacy.resolveResume(input);
  const worktreeId = input.assignment.worktreeId;
  const initialHead = input.assignment.initialHead;
  const projectId = input.assignment.projectId;
  const contextId = input.assignment.contextId;
  if (worktreeId === null || initialHead === null || projectId === null || contextId === null)
    return forbidden("repository workspace assignment is incomplete");
  const resolved = await options.repositories.resolveAssignedWorkspace({
    projectId,
    contextId,
    worktreeId,
    executionHostId: options.executionHostId,
    assignmentId: input.assignment.assignmentId,
    attemptId: input.attemptId,
    mode,
    expectedInitialHead: initialHead,
  });
  if (!resolved.ok) return fromRepository(resolved);
  if (
    resolved.value.worktree.projectId !== projectId ||
    !input.access.authorizedProjectIds.includes(projectId)
  )
    return forbidden("assigned workspace is outside authenticated project scope");
  return {
    ok: true,
    value: {
      assignment: input.assignment,
      cwd: resolved.value.projectCwd,
      observedHead: resolved.value.verifiedHead,
      dirty: resolved.value.dirty,
    } satisfies ManagedWorkspaceLaunchBinding,
  };
}

function selectionKind(
  input: ManagedWorkspacePreparationInput,
): ManagedWorkspaceAssignment["kind"] {
  if (input.request.workspaceRequest.mode === "isolated_child") return "isolated_child";
  if (input.request.workspaceRequest.mode === "integration_resolution")
    return "integration_resolution";
  return "inherited";
}

function fromRepository(result: {
  readonly ok: false;
  readonly error: { readonly code: string; readonly message: string };
}): {
  readonly ok: false;
  readonly error: {
    readonly code: "forbidden" | "conflict" | "unavailable";
    readonly message: string;
  };
} {
  if (result.error.code === "conflict" || result.error.code === "stale")
    return conflict(result.error.message);
  return result.error.code === "denied"
    ? forbidden(result.error.message)
    : unavailable(result.error.message);
}
function forbidden(message: string) {
  return { ok: false as const, error: { code: "forbidden" as const, message } };
}
function conflict(message: string) {
  return { ok: false as const, error: { code: "conflict" as const, message } };
}
function unavailable(message: string) {
  return { ok: false as const, error: { code: "unavailable" as const, message } };
}
