/** Lazy per-context ZAP planning runtime. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { isAbsolute } from "node:path";
import { createHash } from "node:crypto";
import { z } from "zod";
import {
  openQuicklensSourceRuntime,
  QuicklensSourceRuntimeConfigSchema,
  type QuicklensSourceRuntime,
} from "../quicklens-service/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { QuicklensRefSchema } from "../quicklens-model/index.ts";
import {
  ProjectIdSchema,
  WorkContextIdSchema,
  type WorkspaceAccessContext,
  type WorkspaceCommandResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type {
  WorkspacePlanCommand,
  WorkspacePlanningFeature,
  WorkspacePlanningSourceObserver,
} from "./types.ts";
import type { PlanOperationResult, QuicklensSnapshot } from "../quicklens-model/index.ts";
import { auditAgentPlanning } from "./agent.ts";
import { recordSourceChange } from "./source-events.ts";

type WorkspacePlanResponse = Extract<WorkspaceCommandResponse, { operation: `plan.${string}` }>;

const ContextConfigSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    source: QuicklensSourceRuntimeConfigSchema,
  })
  .strict();
export const WorkspacePlanningRuntimeConfigSchema = z
  .object({
    configDirectory: z.string().min(1).refine(isAbsolute),
    contexts: z.array(ContextConfigSchema).min(1).max(256),
  })
  .strict()
  .superRefine((config, context) => {
    const keys = config.contexts.map((entry) => key(entry.projectId, entry.contextId));
    if (new Set(keys).size !== keys.length)
      context.addIssue({
        code: "custom",
        path: ["contexts"],
        message: "planning contexts must be unique",
      });
  });
export type WorkspacePlanningRuntimeConfig = z.infer<typeof WorkspacePlanningRuntimeConfigSchema>;

export interface WorkspacePlanningController {
  readonly feature: WorkspacePlanningFeature;
  start(): Promise<WorkspaceResult<null>>;
  close(): void;
}

export function createWorkspacePlanningController(
  raw: unknown,
  store: WorkspaceStore,
  sourceObserver?: WorkspacePlanningSourceObserver,
): WorkspaceResult<WorkspacePlanningController> {
  const config = WorkspacePlanningRuntimeConfigSchema.safeParse(raw);
  if (!config.success) return failure("invalid_input", "planning runtime configuration is invalid");
  const contexts = new Map<string, QuicklensSourceRuntime>();
  const snapshots = new Map<string, QuicklensSnapshot>();
  const unsubscribers: (() => void)[] = [];
  const feature: WorkspacePlanningFeature = {
    async snapshot(access, projectId, contextId) {
      const selected = select(access, contexts, projectId, contextId);
      if (!selected.ok) return selected;
      const result = await selected.value.source.read({ signal: new AbortController().signal });
      return result.ok
        ? {
            ok: true,
            value: {
              operation: "project.snapshot.v1",
              snapshot: { state: "ready", snapshot: result.value },
            },
          }
        : {
            ok: true,
            value: {
              operation: "project.snapshot.v1",
              snapshot: {
                state: "unavailable",
                code: result.error.code === "stale_basis" ? "stale_context" : "zap_unavailable",
                reason: result.error.message,
              },
            },
          };
    },
    async command(access, request) {
      const selected = select(access, contexts, request.projectId, request.contextId);
      if (!selected.ok) return selected;
      const source = selected.value.source;
      const result =
        request.operation === "plan.intent.v1"
          ? await source.proposePlanIntentWithIdentity(
              request.input,
              planIntentIdentity(access, request),
            )
          : request.operation === "plan.preview.v1"
            ? await source.previewPlan(request.input)
            : request.operation === "plan.apply.v1"
              ? await source.applyPlan(request.input)
              : request.operation === "plan.reconcile.v1"
                ? await source.reconcilePlan(request.input)
                : await source.decidePlan(request.input);
      if (!result.ok) return quicklensFailure(result.error.code, result.error.message);
      const response = planResponse(request, result.value);
      const recorded = recordPlanEvent(store, access, request, response);
      return recorded.ok ? { ok: true, value: response } : recorded;
    },
    agent(actor, transport) {
      const scope = store.resolveAgentScope(actor.actor.workspaceId, actor.actor.conversationId);
      if (!scope.ok) return scope;
      const selected = contexts.get(key(scope.value.projectId, scope.value.contextId));
      return selected === undefined
        ? failure("unavailable", "planning runtime is not configured for this broker scope")
        : {
            ok: true,
            value: auditAgentPlanning(
              selected.createAgentPlanning(transport),
              store,
              scope.value,
              actor,
            ),
          };
    },
    close() {
      for (const unsubscribe of unsubscribers.splice(0)) unsubscribe();
      for (const runtime of contexts.values()) runtime.close();
      contexts.clear();
      snapshots.clear();
    },
  };
  return {
    ok: true,
    value: {
      feature,
      async start() {
        if (contexts.size > 0) return { ok: true, value: null };
        for (const entry of config.data.contexts) {
          const scope = store.resolveAgentScope(
            entry.source.workspaceId,
            entry.source.conversationId,
          );
          if (
            !scope.ok ||
            scope.value.projectId !== entry.projectId ||
            scope.value.contextId !== entry.contextId
          ) {
            feature.close();
            return failure(
              "conflict",
              "planning broker scope does not match registered project context",
            );
          }
          const opened = await openQuicklensSourceRuntime(entry.source, {
            configDirectory: config.data.configDirectory,
          });
          if (!opened.ok) {
            feature.close();
            return failure("unavailable", opened.error.message);
          }
          contexts.set(key(entry.projectId, entry.contextId), opened.value);
          const contextKey = key(entry.projectId, entry.contextId);
          const initial = await opened.value.source.read({ signal: new AbortController().signal });
          if (initial.ok) {
            snapshots.set(contextKey, initial.value);
            sourceObserver?.observe({
              projectId: entry.projectId,
              contextId: entry.contextId,
              reason: "initial authoritative planning snapshot",
              snapshot: initial.value,
            });
          }
          unsubscribers.push(
            opened.value.source.subscribe?.((reason) => {
              void refreshSource(
                store,
                entry.projectId,
                entry.contextId,
                reason,
                opened.value,
                snapshots,
                sourceObserver,
              );
            }) ?? (() => undefined),
          );
        }
        return { ok: true, value: null };
      },
      close: () => {
        feature.close();
      },
    },
  };
}

function planResponse(
  request: WorkspacePlanCommand,
  result: PlanOperationResult,
): WorkspacePlanResponse {
  switch (request.operation) {
    case "plan.intent.v1":
      return { operation: request.operation, result };
    case "plan.preview.v1":
      return { operation: request.operation, result };
    case "plan.apply.v1":
      return { operation: request.operation, result };
    case "plan.reconcile.v1":
      return { operation: request.operation, result };
    case "plan.decide.v1":
      return { operation: request.operation, result };
  }
}

function select(
  access: { readonly authorizedProjectIds: readonly string[] },
  contexts: ReadonlyMap<string, QuicklensSourceRuntime>,
  projectId: string,
  contextId: string,
): WorkspaceResult<QuicklensSourceRuntime> {
  if (!access.authorizedProjectIds.includes(projectId))
    return failure("forbidden", "planning project is outside authenticated scope");
  const selected = contexts.get(key(projectId, contextId));
  return selected === undefined
    ? failure("unavailable", "planning runtime is not configured for this project context")
    : { ok: true, value: selected };
}

function recordPlanEvent(
  store: WorkspaceStore,
  access: WorkspaceAccessContext,
  request: WorkspacePlanCommand,
  response: WorkspacePlanResponse,
) {
  const payload = JsonValueSchema.safeParse(response);
  if (!payload.success) return failure("storage_failure", "planning result is not public JSON");
  return store.ingestEvent({
    projectId: request.projectId,
    contextId: request.contextId,
    kind: request.operation,
    source: "lens",
    actorId: access.actorId,
    occurrenceAt: new Date().toISOString(),
    sourceEventId: `plan-command:${request.clientRequestId}`,
    correlationId: response.result.operationRef,
    causationId: null,
    // The public workflow result does not expose a source-proved plan identity.
    // State and before/next bases remain in payload; provenance stays unknown.
    planProvenance: null,
    sourceSequence: null,
    payload: payload.data,
  });
}

function key(projectId: string, contextId: string): string {
  return `${projectId}\u0000${contextId}`;
}

async function refreshSource(
  store: WorkspaceStore,
  projectId: ReturnType<typeof ProjectIdSchema.parse>,
  contextId: ReturnType<typeof WorkContextIdSchema.parse>,
  reason: string,
  runtime: QuicklensSourceRuntime,
  snapshots: Map<string, QuicklensSnapshot>,
  sourceObserver: WorkspacePlanningSourceObserver | undefined,
): Promise<void> {
  const result = await runtime.source.read({ signal: new AbortController().signal });
  if (!result.ok) return;
  const contextKey = key(projectId, contextId);
  const before = snapshots.get(contextKey) ?? null;
  recordSourceChange({ store, projectId, contextId, reason, before, after: result.value });
  sourceObserver?.observe({ projectId, contextId, reason, snapshot: result.value });
  snapshots.set(contextKey, result.value);
}

function quicklensFailure(code: string, message: string) {
  return failure(
    code === "forbidden" ? "forbidden" : code === "stale_basis" ? "stale_revision" : "conflict",
    message,
  );
}

function planIntentIdentity(access: WorkspaceAccessContext, request: WorkspacePlanCommand) {
  const value = createHash("sha256")
    .update(
      `${access.principalId}\u0000${access.actorId ?? `client:${access.clientId}`}\u0000${request.projectId}\u0000${request.contextId}\u0000${request.clientRequestId}`,
    )
    .digest("hex");
  return {
    intentRef: QuicklensRefSchema.parse(`intent:${value}`),
    clientRequestId: ClientRequestIdSchema.parse(`plan-intent.${value}`),
  };
}

function failure(
  code:
    | "invalid_input"
    | "forbidden"
    | "conflict"
    | "stale_revision"
    | "unavailable"
    | "storage_failure",
  message: string,
): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: ${message}`,
    },
  };
}
