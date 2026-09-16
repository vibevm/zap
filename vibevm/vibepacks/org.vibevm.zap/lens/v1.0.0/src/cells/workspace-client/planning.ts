/** Project-scoped Quicklens plan data source over WorkspaceClientPort. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import type {
  InvalidationReason,
  PlanApplyInput,
  PlanDecisionInput,
  PlanIntentInput,
  PlanOperationResult,
  PlanPreviewInput,
  PlanReconcileInput,
  QuicklensDataSource,
  QuicklensError,
  QuicklensResult,
  QuicklensSnapshot,
} from "../quicklens-model/index.ts";
import {
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceClientPort,
  type WorkspaceError,
  HistoryCursorSchema,
  type HistoryEvent,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";

export interface WorkspacePlanningDataSourceOptions {
  readonly port: WorkspaceClientPort;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
}

type PlanCommandOperation =
  | "plan.intent.v1"
  | "plan.preview.v1"
  | "plan.apply.v1"
  | "plan.reconcile.v1"
  | "plan.decide.v1";

export function createWorkspacePlanningDataSource(
  options: WorkspacePlanningDataSourceOptions,
): QuicklensDataSource {
  const listeners = new Set<(reason: InvalidationReason) => void>();
  let stream: { readonly controller: AbortController } | undefined;

  const read = async (input: {
    readonly signal: AbortSignal;
  }): Promise<QuicklensResult<QuicklensSnapshot>> => {
    if (input.signal.aborted) return cancelled();
    const response = await options.port.read({
      operation: "project.snapshot.v1",
      projectId: options.projectId,
      contextId: options.contextId,
    });
    if (!response.ok) return mapError(response.error);
    if (response.value.operation !== "project.snapshot.v1") return mismatch("project.snapshot.v1");
    return response.value.snapshot.state === "ready"
      ? {
          ok: true,
          value: {
            ...response.value.snapshot.snapshot,
            questionAnswer: {
              enabled: false,
              reason: "Answer questions in the shared workspace Questions view.",
            },
          },
        }
      : {
          ok: false,
          error: {
            code: "unavailable",
            message: response.value.snapshot.reason,
            recovery: "Connect the configured planning adapter, then refresh the project.",
          },
        };
  };

  const subscribe = (listener: (reason: InvalidationReason) => void): (() => void) => {
    listeners.add(listener);
    if (stream === undefined) {
      const controller = new AbortController();
      stream = { controller };
      void consumeEvents(controller);
    }
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0 && stream !== undefined) {
        stream.controller.abort();
        stream = undefined;
      }
    };
  };

  return {
    read,
    subscribe,
    answerQuestion: () => unavailable("Workspace planning does not own question answers."),
    proposePlanIntent: (input) =>
      sendPlanCommand(options, {
        operation: "plan.intent.v1",
        input,
      }),
    previewPlan: (input) =>
      sendPlanCommand(options, {
        operation: "plan.preview.v1",
        input,
      }),
    applyPlan: (input) =>
      sendPlanCommand(options, {
        operation: "plan.apply.v1",
        input,
      }),
    reconcilePlan: (input) =>
      sendPlanCommand(options, {
        operation: "plan.reconcile.v1",
        input,
      }),
    decidePlan: (input) =>
      sendPlanCommand(options, {
        operation: "plan.decide.v1",
        input,
      }),
  };

  async function consumeEvents(controller: AbortController): Promise<void> {
    const cursor = HistoryCursorSchema.parse({
      scope: { kind: "project", projectId: options.projectId },
      afterGlobalSequence: DecimalSchema.parse("0"),
    });
    try {
      for await (const result of options.port.subscribe({ cursor, signal: controller.signal })) {
        if (controller.signal.aborted) return;
        if (!result.ok) {
          notify("reconnect");
          return;
        }
        if (
          result.value.projectId !== options.projectId ||
          (result.value.contextId !== null && result.value.contextId !== options.contextId)
        )
          continue;
        notify(reasonFor(result.value));
      }
    } catch {
      if (!controller.signal.aborted) notify("reconnect");
    } finally {
      if (stream?.controller === controller) stream = undefined;
    }
  }

  function notify(reason: InvalidationReason): void {
    for (const next of listeners) next(reason);
  }
}

type PlanInput =
  | { readonly operation: "plan.intent.v1"; readonly input: PlanIntentInput }
  | { readonly operation: "plan.preview.v1"; readonly input: PlanPreviewInput }
  | { readonly operation: "plan.apply.v1"; readonly input: PlanApplyInput }
  | { readonly operation: "plan.reconcile.v1"; readonly input: PlanReconcileInput }
  | { readonly operation: "plan.decide.v1"; readonly input: PlanDecisionInput };

async function sendPlanCommand(
  options: WorkspacePlanningDataSourceOptions,
  input: PlanInput,
): Promise<QuicklensResult<PlanOperationResult>> {
  const request = planRequest(options, input);
  const response = await options.port.command(request);
  if (!response.ok) return mapError(response.error);
  return planResponse(response.value, input.operation);
}

function planRequest(
  options: WorkspacePlanningDataSourceOptions,
  input: PlanInput,
): WorkspaceCommandRequest {
  const scope = {
    clientRequestId: ClientRequestIdSchema.parse(`request.workspace.plan.${crypto.randomUUID()}`),
    projectId: options.projectId,
    contextId: options.contextId,
  };
  switch (input.operation) {
    case "plan.intent.v1":
      return { ...scope, operation: input.operation, input: input.input };
    case "plan.preview.v1":
      return { ...scope, operation: input.operation, input: input.input };
    case "plan.apply.v1":
      return { ...scope, operation: input.operation, input: input.input };
    case "plan.reconcile.v1":
      return { ...scope, operation: input.operation, input: input.input };
    case "plan.decide.v1":
      return { ...scope, operation: input.operation, input: input.input };
  }
}

function planResponse(
  response: WorkspaceCommandResponse,
  operation: PlanCommandOperation,
): QuicklensResult<PlanOperationResult> {
  switch (response.operation) {
    case "plan.intent.v1":
    case "plan.preview.v1":
    case "plan.apply.v1":
    case "plan.reconcile.v1":
    case "plan.decide.v1":
      return response.operation === operation
        ? { ok: true, value: response.result }
        : mismatch(operation);
    default:
      return mismatch(operation);
  }
}

function reasonFor(event: HistoryEvent): InvalidationReason {
  if (event.kind.startsWith("question.")) return "questions";
  if (event.kind.startsWith("plan.") || event.planProvenance !== null) return "plan";
  return "events";
}

function mapError(error: WorkspaceError): QuicklensResult<never> {
  const code: QuicklensError["code"] =
    error.code === "invalid_input"
      ? "invalid_data"
      : error.code === "forbidden" || error.code === "unauthorized"
        ? "forbidden"
        : error.code === "stale_revision"
          ? "stale_basis"
          : error.code === "unsupported_operation"
            ? "unsupported"
            : "unavailable";
  return {
    ok: false,
    error: {
      code,
      message: error.message,
      recovery: `Workspace service returned ${error.code}; refresh the project context and retry.`,
    },
  };
}

function mismatch(operation: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_data",
      message: `Workspace service returned an unexpected ${operation} response.`,
      recovery: "Refresh the project context and retry.",
    },
  };
}

function cancelled(): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code: "cancelled",
      message: "Workspace plan read was cancelled.",
      recovery: "Open the project plan again.",
    },
  };
}

function unavailable<T>(message: string): Promise<QuicklensResult<T>> {
  const error: QuicklensError = {
    code: "unavailable",
    message,
    recovery: "Use the workspace question and interaction controls for this action.",
  };
  return Promise.resolve({ ok: false, error });
}
