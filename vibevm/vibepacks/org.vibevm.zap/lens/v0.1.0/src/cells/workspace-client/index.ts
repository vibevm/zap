/**
 * Browser-safe typed reads over the shared Lens workspace port.
 * @scope spec://org.vibevm.zap/lens/PROP-005#shared-code
 * @example
 * const projects = await listWorkspaceProjects(port);
 * if (projects.ok) show(projects.value);
 */
import type {
  AgentNetwork,
  AgentOutputItem,
  ChatMessage,
  CoordinatorSession,
  CoordinatorLaunchOption,
  HistoryCursor,
  HistoryPage,
  ProjectDescriptor,
  ProjectExecutionState,
  ProjectId,
  QuestionAnswerVersion,
  QuestionGroup,
  QuestionGroupId,
  ModelPolicyChangeView,
  ModelPolicyVersionView,
  StoredModelPolicyView,
  WorkspaceClientPort,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
  WorkspaceError,
  WorkspaceReadRequest,
  WorkspaceReadResponse,
  WorkspaceResult,
  WorkContextDescriptor,
  WorkContextId,
} from "../workspace-model/index.ts";
import type { QuicklensSnapshot } from "../quicklens-model/index.ts";
import { unavailableDataSource, type QuicklensDataSource } from "../quicklens-model/index.ts";
import { ClientRequestIdSchema, DecimalSchema, type LosslessDecimal } from "../protocol/index.ts";

type ResponseFor<Operation extends WorkspaceReadResponse["operation"]> = Extract<
  WorkspaceReadResponse,
  { readonly operation: Operation }
>;

export interface ProjectWorkspaceView {
  readonly project: ProjectDescriptor;
  readonly execution: ProjectExecutionState;
  readonly contexts: readonly WorkContextDescriptor[];
  readonly coordinator: CoordinatorSession | null;
  readonly coordinatorLaunchOptions: readonly CoordinatorLaunchOption[];
  readonly sessions: readonly CoordinatorSession[];
  readonly network: AgentNetwork;
  readonly questions: readonly QuestionGroup[];
  readonly snapshot:
    | { readonly state: "ready"; readonly snapshot: QuicklensSnapshot }
    | {
        readonly state: "unavailable";
        readonly code: "zap_not_configured" | "zap_unavailable" | "stale_context";
        readonly reason: string;
      };
}

export interface QuestionWorkspaceView {
  readonly question: QuestionGroup;
  readonly answerVersions: readonly QuestionAnswerVersion[];
}

export interface ModelPolicyWorkspaceView {
  readonly policy: StoredModelPolicyView;
  readonly versions: readonly ModelPolicyVersionView[];
  readonly changes: readonly ModelPolicyChangeView[];
}

export async function readWorkspaceModelPolicy(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
): Promise<WorkspaceResult<StoredModelPolicyView>> {
  const response = await readOperation(port, {
    operation: "model-policy.get.v1",
    projectId,
    contextId,
  });
  return response.ok ? { ok: true, value: response.value.policy } : response;
}

export async function readWorkspaceModelPolicyHistory(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  contextId: WorkContextId,
): Promise<WorkspaceResult<Pick<ModelPolicyWorkspaceView, "versions" | "changes">>> {
  const response = await readOperation(port, {
    operation: "model-policy.history.v1",
    projectId,
    contextId,
  });
  return response.ok
    ? { ok: true, value: { versions: response.value.versions, changes: response.value.changes } }
    : response;
}

export async function previewWorkspaceModelPolicy(
  port: WorkspaceClientPort,
  request: Extract<WorkspaceReadRequest, { operation: "model-policy.preview.v1" }>,
): Promise<
  WorkspaceResult<Extract<WorkspaceReadResponse, { operation: "model-policy.preview.v1" }>>
> {
  const response = await readOperation(port, request);
  return response;
}

export async function updateWorkspaceModelPolicy(
  port: WorkspaceClientPort,
  request: Extract<WorkspaceCommandRequest, { operation: "model-policy.update.v1" }>,
): Promise<
  WorkspaceResult<Extract<WorkspaceCommandResponse, { operation: "model-policy.update.v1" }>>
> {
  const response = await port.command(request);
  if (!response.ok) return response;
  return response.value.operation === "model-policy.update.v1"
    ? { ok: true, value: response.value }
    : {
        ok: false,
        error: {
          code: "invalid_input",
          message:
            "violates REQ spec://org.vibevm.zap/lens/PROP-008#policy-interface: model policy response did not match the update operation.",
        },
      };
}

export async function listWorkspaceProjects(
  port: WorkspaceClientPort,
): Promise<WorkspaceResult<readonly ProjectDescriptor[]>> {
  const response = await readOperation(port, { operation: "project.list.v1" });
  return response.ok ? { ok: true, value: response.value.projects } : response;
}

/** @implements spec://org.vibevm.zap/lens/PROP-005#project-views */
export async function readProjectWorkspace(
  port: WorkspaceClientPort,
  projectId: ProjectId,
  preferredContextId?: WorkContextId,
): Promise<WorkspaceResult<ProjectWorkspaceView>> {
  const detail = await readOperation(port, { operation: "project.get.v1", projectId });
  if (!detail.ok) return detail;
  const contextId = preferredContextId ?? detail.value.detail.project.defaultContextId;
  const [snapshot, execution, network, questions, sessions] = await Promise.all([
    readOperation(port, { operation: "project.snapshot.v1", projectId, contextId }),
    readOperation(port, { operation: "project.execution.get.v1", projectId, contextId }),
    readOperation(port, { operation: "agent.network.v1", projectId, contextId }),
    readOperation(port, {
      operation: "question.list.v1",
      projectId,
      contextId,
      state: null,
      limit: 100,
    }),
    readOperation(port, { operation: "session.list.v1", projectId, contextId }),
  ]);
  if (!snapshot.ok) return snapshot;
  if (!execution.ok) return execution;
  if (!network.ok) return network;
  if (!questions.ok) return questions;
  if (!sessions.ok) return sessions;
  return {
    ok: true,
    value: {
      project: detail.value.detail.project,
      execution: execution.value.execution,
      contexts: detail.value.detail.contexts,
      coordinator: detail.value.detail.coordinator,
      coordinatorLaunchOptions: detail.value.detail.coordinatorLaunchOptions,
      sessions: sessions.value.sessions,
      network: network.value.network,
      questions: questions.value.questions,
      snapshot: snapshot.value.snapshot,
    },
  };
}

export async function readAgentOutput(
  port: WorkspaceClientPort,
  scope: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly actorId: AgentNetwork["agents"][number]["actorId"];
    readonly afterSequence: LosslessDecimal;
  },
): Promise<
  WorkspaceResult<{
    readonly items: readonly AgentOutputItem[];
    readonly nextSequence: LosslessDecimal | null;
  }>
> {
  const response = await readOperation(port, {
    operation: "agent.output.page.v1",
    ...scope,
    limit: 100,
  });
  return response.ok
    ? {
        ok: true,
        value: {
          items: response.value.page.items,
          nextSequence: response.value.page.nextSequence,
        },
      }
    : response;
}

export async function readWorkspaceChat(
  port: WorkspaceClientPort,
  scope: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly conversationId: CoordinatorSession["conversationId"];
    readonly afterSequence: LosslessDecimal;
  },
): Promise<
  WorkspaceResult<{
    readonly messages: readonly ChatMessage[];
    readonly nextSequence: LosslessDecimal | null;
  }>
> {
  const response = await readOperation(port, {
    operation: "chat.page.v1",
    ...scope,
    limit: 100,
  });
  return response.ok
    ? {
        ok: true,
        value: {
          messages: response.value.page.messages,
          nextSequence: response.value.page.nextSequence,
        },
      }
    : response;
}

export async function readQuestionWorkspace(
  port: WorkspaceClientPort,
  scope: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly questionGroupId: QuestionGroupId;
  },
): Promise<WorkspaceResult<QuestionWorkspaceView>> {
  const response = await readOperation(port, { operation: "question.get.v1", ...scope });
  return response.ok ? { ok: true, value: response.value.detail } : response;
}

export async function readWorkspaceHistory(
  port: WorkspaceClientPort,
  cursor: HistoryCursor,
): Promise<WorkspaceResult<HistoryPage>> {
  return port.events({ cursor, limit: 100 });
}

export function initialHistoryCursor(scope: HistoryCursor["scope"]): HistoryCursor {
  return { scope, afterGlobalSequence: DecimalSchema.parse("0") };
}

export function workspaceRequestId(purpose: string) {
  return ClientRequestIdSchema.parse(`request.workspace.${purpose}.${crypto.randomUUID()}`);
}

export function readOnlyProjectDataSource(snapshot: QuicklensSnapshot): QuicklensDataSource {
  const reason = "Workspace plan embedding is read-only; use the connected plan-control surface.";
  const unavailable = unavailableDataSource(reason);
  const disabled = { enabled: false, reason } as const;
  const view: QuicklensSnapshot = {
    ...snapshot,
    questionAnswer: disabled,
    plan:
      snapshot.plan === null
        ? null
        : {
            ...snapshot.plan,
            actions: {
              propose: disabled,
              preview: disabled,
              apply: disabled,
              reconcile: disabled,
              decide: disabled,
            },
          },
  };
  return {
    ...unavailable,
    read: ({ signal }) =>
      Promise.resolve(
        signal.aborted
          ? {
              ok: false,
              error: {
                code: "cancelled",
                message: "Workspace plan read was cancelled.",
                recovery: "Open the project plan again.",
              },
            }
          : { ok: true, value: view },
      ),
  };
}

async function readOperation<Operation extends WorkspaceReadRequest["operation"]>(
  port: WorkspaceClientPort,
  request: Extract<WorkspaceReadRequest, { readonly operation: Operation }>,
): Promise<WorkspaceResult<ResponseFor<Operation>>> {
  const response = await port.read(request);
  if (!response.ok) return response;
  return operationIs(response.value, request.operation)
    ? { ok: true, value: response.value }
    : mismatch(request.operation);
}

function operationIs<Operation extends WorkspaceReadResponse["operation"]>(
  response: WorkspaceReadResponse,
  operation: Operation,
): response is ResponseFor<Operation> {
  return response.operation === operation;
}

function mismatch(expected: string): WorkspaceResult<never> {
  const error: WorkspaceError = {
    code: "invalid_input",
    message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#shared-code: workspace response did not match ${expected}`,
    details: { expected },
  };
  return { ok: false, error };
}

export type { AgentSessionId, WorkspaceClientPort } from "../workspace-model/index.ts";
export {
  createWorkspaceHttpClient,
  parseWorkspaceGateway,
  type WorkspaceHttpClientOptions,
} from "./http.ts";
export {
  createWorkspaceWebClient,
  type WorkspaceWebClient,
  type WorkspaceWebClientOptions,
} from "./web.ts";
export {
  createWorkspacePlanningDataSource,
  type WorkspacePlanningDataSourceOptions,
} from "./planning.ts";
