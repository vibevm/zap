/** Browser-safe repository workspace client. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import type {
  IntegrationAttempt,
  ProjectPlanRecord,
  RepositoryProjectBinding,
  RepositoryRecord,
  RepositoryWorktreeRecord,
} from "../repository-model/index.ts";
import type {
  ProjectId,
  WorkContextId,
  WorkspaceClientPort,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
  WorkspaceResult,
  WorkspaceReadRequest,
  WorkspaceReadResponse,
} from "../workspace-model/index.ts";

type RepositoryCommand = Extract<
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
type RepositoryResponse<Operation extends RepositoryCommand["operation"]> = Extract<
  WorkspaceCommandResponse,
  { operation: Operation }
>;

export interface RepositoryWorkspaceView {
  readonly repository: RepositoryRecord;
  readonly binding: RepositoryProjectBinding;
  readonly registeredWorktree: RepositoryWorktreeRecord;
  readonly contextWorktree: RepositoryWorktreeRecord;
  readonly observedContextHead: Extract<
    WorkspaceReadResponse,
    { operation: "repository.get.v1" }
  >["observedContextHead"];
  readonly workingTreeState: Extract<
    WorkspaceReadResponse,
    { operation: "repository.get.v1" }
  >["workingTreeState"];
  readonly testProfiles: Extract<
    WorkspaceReadResponse,
    { operation: "repository.get.v1" }
  >["testProfiles"];
  readonly plans: readonly ProjectPlanRecord[];
  readonly worktrees: readonly RepositoryWorktreeRecord[];
  readonly integrations: readonly IntegrationAttempt[];
}
export type IntegrationDiffView = Extract<
  WorkspaceReadResponse,
  { operation: "integration.diff.v1" }
>;

export async function readIntegrationDiff(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
  integrationId: Extract<
    WorkspaceReadRequest,
    { operation: "integration.diff.v1" }
  >["integrationId"],
): Promise<WorkspaceResult<IntegrationDiffView>> {
  const response = await port.read({
    operation: "integration.diff.v1",
    projectId,
    contextId,
    integrationId,
    maximumBytes: 100_000,
  });
  if (!response.ok) return response;
  return response.value.operation === "integration.diff.v1"
    ? { ok: true, value: response.value }
    : mismatch("integration.diff.v1");
}

export async function readRepositoryWorkspace(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
): Promise<WorkspaceResult<RepositoryWorkspaceView>> {
  const [repository, plans] = await Promise.all([
    port.read({ operation: "repository.get.v1", projectId, contextId }),
    port.read({ operation: "plan.workspace.list.v1", projectId }),
  ]);
  if (!repository.ok) return repository;
  if (!plans.ok) return plans;
  if (repository.value.operation !== "repository.get.v1") return mismatch("repository.get.v1");
  if (plans.value.operation !== "plan.workspace.list.v1") return mismatch("plan.workspace.list.v1");
  const scopedPlans = plans.value.plans.filter((plan) => plan.contextId === contextId);
  const children = await Promise.all(
    scopedPlans.map(async (plan) => {
      const [worktrees, integrations] = await Promise.all([
        port.read({ operation: "worktree.list.v1", projectId, contextId, planId: plan.planId }),
        port.read({ operation: "integration.list.v1", projectId, contextId, planId: plan.planId }),
      ]);
      return { worktrees, integrations };
    }),
  );
  const worktrees: RepositoryWorktreeRecord[] = [
    repository.value.registeredWorktree,
    repository.value.contextWorktree,
  ];
  const integrations: IntegrationAttempt[] = [];
  for (const child of children) {
    if (!child.worktrees.ok) return child.worktrees;
    if (!child.integrations.ok) return child.integrations;
    if (child.worktrees.value.operation !== "worktree.list.v1") return mismatch("worktree.list.v1");
    if (child.integrations.value.operation !== "integration.list.v1")
      return mismatch("integration.list.v1");
    worktrees.push(...child.worktrees.value.worktrees);
    integrations.push(...child.integrations.value.integrations);
  }
  return {
    ok: true,
    value: {
      repository: repository.value.repository,
      binding: repository.value.binding,
      registeredWorktree: repository.value.registeredWorktree,
      contextWorktree: repository.value.contextWorktree,
      observedContextHead: repository.value.observedContextHead,
      workingTreeState: repository.value.workingTreeState,
      testProfiles: repository.value.testProfiles,
      plans: scopedPlans,
      worktrees: unique(worktrees, (item) => item.worktreeId),
      integrations: unique(integrations, (item) => item.integrationId),
    },
  };
}

export async function commandRepositoryWorkspace<Operation extends RepositoryCommand["operation"]>(
  port: WorkspaceClientPort,
  request: Extract<RepositoryCommand, { operation: Operation }>,
): Promise<WorkspaceResult<Extract<WorkspaceCommandResponse, { operation: Operation }>>> {
  const response = await port.command(request);
  if (!response.ok) return response;
  return repositoryOperationIs(response.value, request.operation)
    ? { ok: true, value: response.value }
    : mismatch(request.operation);
}

function repositoryOperationIs<Operation extends RepositoryCommand["operation"]>(
  response: WorkspaceCommandResponse,
  operation: Operation,
): response is RepositoryResponse<Operation> {
  return response.operation === operation;
}

function unique<T>(values: readonly T[], key: (value: T) => string): readonly T[] {
  return [...new Map(values.map((value) => [key(value), value])).values()];
}

function mismatch(operation: string): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_input",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-014#projection: workspace response did not match ${operation}`,
    },
  };
}
