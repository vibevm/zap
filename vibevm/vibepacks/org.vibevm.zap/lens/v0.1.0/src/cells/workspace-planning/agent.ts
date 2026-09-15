/** Shared planning agent audit wrapper. @scope spec://org.vibevm.zap/lens/PROP-005#history */
import { createHash } from "node:crypto";
import { z } from "zod";
import { JsonValueSchema, type PublicConnection } from "../protocol/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import type { AgentPlanningPort } from "./types.ts";

export function auditAgentPlanning(
  port: AgentPlanningPort,
  store: WorkspaceStore,
  scope: { readonly projectId: ProjectId; readonly contextId: WorkContextId },
  actor: PublicConnection,
): AgentPlanningPort {
  const observed = async (
    operation: string,
    input: unknown,
    call: () => ReturnType<AgentPlanningPort["author"]>,
  ) => {
    const result = await call();
    if (result.ok) record(store, scope, actor, operation, input, result.value);
    return result;
  };
  return {
    register: (connection, session, input) =>
      observed("intent", input, () => port.register(connection, session, input)),
    submit: (connection, session, input) =>
      observed("proposal", input, () => port.submit(connection, session, input)),
    preview: (connection, session, input) =>
      observed("preview", input, () => port.preview(connection, session, input)),
    apply: (connection, session, input) =>
      observed("apply", input, () => port.apply(connection, session, input)),
    reconcile: (connection, session, input) =>
      observed("reconcile", input, () => port.reconcile(connection, session, input)),
    prepare: (connection, session, kind, input) => port.prepare(connection, session, kind, input),
    discover: (connection, session) => port.discover(connection, session),
    author: (connection, session, input) =>
      observed("author", input, () => port.author(connection, session, input)),
    prepareComposite: (connection, session, input) =>
      observed("prepare-composite", input, () => port.prepareComposite(connection, session, input)),
    authorComposite: (connection, session, input) =>
      observed("author-composite", input, () => port.authorComposite(connection, session, input)),
  };
}

function record(
  store: WorkspaceStore,
  scope: { readonly projectId: ProjectId; readonly contextId: WorkContextId },
  actor: PublicConnection,
  operation: string,
  input: unknown,
  result: unknown,
): void {
  const payload = JsonValueSchema.safeParse({ operation, result });
  if (!payload.success) return;
  store.ingestEvent({
    projectId: scope.projectId,
    contextId: scope.contextId,
    kind: `agent.plan.${operation}`,
    source: "lens",
    actorId: actor.actor.actorId,
    occurrenceAt: new Date().toISOString(),
    sourceEventId: `agent-plan:${actor.actor.actorId}:${digest(operation, input)}`,
    correlationId: correlation(result),
    causationId: null,
    planProvenance: null,
    sourceSequence: null,
    payload: payload.data,
  });
}

function digest(operation: string, input: unknown): string {
  return createHash("sha256")
    .update(operation)
    .update(JSON.stringify(input, jsonReplacer))
    .digest("hex");
}

function jsonReplacer(_key: string, value: unknown): unknown {
  return typeof value === "bigint" ? value.toString() : value;
}

function correlation(value: unknown): string | null {
  const parsed = z
    .looseObject({ operationId: z.string().optional(), operationRef: z.string().optional() })
    .safeParse(value);
  return parsed.success ? (parsed.data.operationId ?? parsed.data.operationRef ?? null) : null;
}
