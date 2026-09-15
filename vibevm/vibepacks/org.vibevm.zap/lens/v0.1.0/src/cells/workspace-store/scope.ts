/** Stored project/context scope checks. @scope spec://org.vibevm.zap/lens/PROP-005#project-context */
import { z } from "zod";
import {
  WorkContextDescriptorSchema,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import type { ConversationId, WorkspaceId } from "../protocol/index.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";

const JsonRowSchema = z.object({ public_json: z.string() });
const CountRowSchema = z.object({ value: z.bigint() });

export function context(state: WorkspaceState, projectId: ProjectId, contextId: WorkContextId) {
  const row = state.database.get(
    "SELECT public_json FROM workspace_contexts WHERE project_id = ? AND context_id = ?",
    JsonRowSchema,
    [projectId, contextId],
  );
  return row === null ? null : state.parse(row.public_json, WorkContextDescriptorSchema);
}

export function actorExists(
  state: WorkspaceState,
  projectId: ProjectId,
  contextId: WorkContextId,
  actorId: string,
): boolean {
  return (
    state.database.get(
      `SELECT COUNT(*) AS value FROM workspace_agents
       WHERE project_id = ? AND context_id = ? AND actor_id = ?`,
      CountRowSchema,
      [projectId, contextId, actorId],
    )?.value === 1n
  );
}

export function registerAgentScope(
  state: WorkspaceState,
  brokerScope: { readonly workspaceId: WorkspaceId; readonly conversationId: ConversationId },
  projectId: ProjectId,
  contextId: WorkContextId,
) {
  const rowSchema = z.object({ project_id: z.string(), context_id: z.string() });
  const byBroker = state.database.get(
    `SELECT project_id, context_id FROM workspace_agent_scopes
     WHERE workspace_id = ? AND conversation_id = ?`,
    rowSchema,
    [brokerScope.workspaceId, brokerScope.conversationId],
  );
  const byContext = state.database.get(
    `SELECT project_id, context_id FROM workspace_agent_scopes
     WHERE project_id = ? AND context_id = ?`,
    rowSchema,
    [projectId, contextId],
  );
  if (byBroker !== null || byContext !== null) {
    return byBroker?.project_id === projectId &&
      byBroker.context_id === contextId &&
      byContext?.project_id === projectId &&
      byContext.context_id === contextId
      ? { ok: true as const, value: null }
      : failure("conflict", "broker scope is already registered to another project context");
  }
  state.database.run(
    `INSERT INTO workspace_agent_scopes(workspace_id, conversation_id, project_id, context_id)
     VALUES(?, ?, ?, ?)`,
    [brokerScope.workspaceId, brokerScope.conversationId, projectId, contextId],
  );
  return { ok: true as const, value: null };
}
