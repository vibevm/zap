/** Authorized workspace read projections. @scope spec://org.vibevm.zap/lens/PROP-005#project-views */
import { z } from "zod";
import {
  AgentDescriptorSchema,
  AgentNetworkSchema,
  AgentOutputItemSchema,
  AgentOutputPageSchema,
  AgentRelationshipSchema,
  ChatMessageSchema,
  ChatPageSchema,
  CoordinatorSessionSchema,
  CoordinatorLaunchOptionSchema,
  HistoryEventSchema,
  HistoryPageSchema,
  ProjectDescriptorSchema,
  ProjectDetailSchema,
  ProjectExecutionStateSchema,
  QuestionAnswerVersionSchema,
  QuestionDetailSchema,
  QuestionGroupSchema,
  NativeInteractionRecordSchema,
  WorkContextDescriptorSchema,
  type HistoryPage,
  type ProjectId,
  type WorkspaceAccessContext,
  type WorkspaceEventsRequest,
  type WorkspaceReadRequest,
  type WorkspaceReadResponse,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";

const JsonRowSchema = z.object({ public_json: z.string() });
const ProjectRowSchema = JsonRowSchema.extend({ coordinator_launch_options_json: z.string() });

export function readWorkspace(
  state: WorkspaceState,
  access: WorkspaceAccessContext,
  request: WorkspaceReadRequest,
): WorkspaceResult<WorkspaceReadResponse> {
  if (request.operation === "project.list.v1") {
    const projects = state.database
      .all(
        "SELECT public_json FROM workspace_projects ORDER BY project_id LIMIT 256",
        JsonRowSchema,
      )
      .map((row) => state.parse(row.public_json, ProjectDescriptorSchema))
      .filter((project) => authorized(access, project.projectId));
    return { ok: true, value: { operation: request.operation, projects } };
  }
  if (!authorized(access, request.projectId))
    return failure("forbidden", "project read is outside scope");
  switch (request.operation) {
    case "project.get.v1": {
      const row = state.database.get(
        "SELECT public_json, coordinator_launch_options_json FROM workspace_projects WHERE project_id = ?",
        ProjectRowSchema,
        [request.projectId],
      );
      if (row === null) return failure("not_found", "project does not exist");
      const contexts = state.database
        .all(
          "SELECT public_json FROM workspace_contexts WHERE project_id = ? ORDER BY context_id LIMIT 256",
          JsonRowSchema,
          [request.projectId],
        )
        .map((item) => state.parse(item.public_json, WorkContextDescriptorSchema));
      const session = state.database.get(
        "SELECT public_json FROM workspace_sessions WHERE project_id = ? ORDER BY rowid DESC LIMIT 1",
        JsonRowSchema,
        [request.projectId],
      );
      return {
        ok: true,
        value: {
          operation: request.operation,
          detail: ProjectDetailSchema.parse({
            project: state.parse(row.public_json, ProjectDescriptorSchema),
            contexts,
            coordinator:
              session === null ? null : state.parse(session.public_json, CoordinatorSessionSchema),
            coordinatorLaunchOptions: z
              .array(CoordinatorLaunchOptionSchema)
              .parse(JSON.parse(row.coordinator_launch_options_json)),
          }),
        },
      };
    }
    case "project.snapshot.v1":
      return {
        ok: true,
        value: {
          operation: request.operation,
          snapshot: {
            state: "unavailable",
            code: "zap_not_configured",
            reason: "Workspace persistence does not own the ZAP snapshot adapter",
          },
        },
      };
    case "project.execution.get.v1": {
      const execution = scopedRow(
        state,
        "workspace_project_execution",
        "context_id",
        request.projectId,
        request.contextId,
      );
      return execution === null
        ? failure("not_found", "project execution state does not exist")
        : {
            ok: true,
            value: {
              operation: request.operation,
              execution: state.parse(execution.public_json, ProjectExecutionStateSchema),
            },
          };
    }
    case "context.get.v1": {
      const context = scopedRow(
        state,
        "workspace_contexts",
        "context_id",
        request.projectId,
        request.contextId,
      );
      return context === null
        ? failure("not_found", "work context does not exist")
        : {
            ok: true,
            value: {
              operation: request.operation,
              context: state.parse(context.public_json, WorkContextDescriptorSchema),
            },
          };
    }
    case "chat.page.v1": {
      const rows = state.database.all(
        `SELECT public_json FROM workspace_chat
         WHERE project_id = ? AND context_id = ? AND conversation_id = ? AND sequence > ?
         ORDER BY sequence LIMIT ?`,
        JsonRowSchema,
        [
          request.projectId,
          request.contextId,
          request.conversationId,
          BigInt(request.afterSequence),
          request.limit + 1,
        ],
      );
      const more = rows.length > request.limit;
      const messages = rows
        .slice(0, request.limit)
        .map((row) => state.parse(row.public_json, ChatMessageSchema));
      const next = messages.at(-1)?.revision ?? null;
      return {
        ok: true,
        value: {
          operation: request.operation,
          page: ChatPageSchema.parse({
            messages,
            afterSequence: request.afterSequence,
            nextSequence: more ? next : null,
          }),
        },
      };
    }
    case "question.get.v1": {
      const question = questionRow(
        state,
        request.projectId,
        request.contextId,
        request.questionGroupId,
      );
      if (question === null) return failure("not_found", "question group does not exist");
      const versions = state.database
        .all(
          `SELECT a.public_json FROM workspace_question_answers a
           JOIN workspace_questions q ON q.question_group_id = a.question_group_id
           WHERE q.project_id = ? AND q.context_id = ? AND a.question_group_id = ?
           ORDER BY a.revision`,
          JsonRowSchema,
          [request.projectId, request.contextId, request.questionGroupId],
        )
        .map((row) => state.parse(row.public_json, QuestionAnswerVersionSchema));
      return {
        ok: true,
        value: {
          operation: request.operation,
          detail: QuestionDetailSchema.parse({
            question: state.parse(question.public_json, QuestionGroupSchema),
            answerVersions: versions,
          }),
        },
      };
    }
    case "question.list.v1": {
      const rows = state.database.all(
        `SELECT public_json FROM workspace_questions
         WHERE project_id = ? AND context_id = ? AND (? IS NULL OR state = ?)
         ORDER BY rowid DESC LIMIT ?`,
        JsonRowSchema,
        [request.projectId, request.contextId, request.state, request.state, request.limit],
      );
      return {
        ok: true,
        value: {
          operation: request.operation,
          questions: rows.map((row) => state.parse(row.public_json, QuestionGroupSchema)),
        },
      };
    }
    case "native-approval.get.v1": {
      const row = state.database.get(
        `SELECT public_json FROM workspace_native_interactions
         WHERE project_id = ? AND context_id = ? AND native_approval_id = ?`,
        JsonRowSchema,
        [request.projectId, request.contextId, request.nativeApprovalId],
      );
      if (row === null) return failure("not_found", "native approval does not exist");
      const record = state.parse(row.public_json, NativeInteractionRecordSchema);
      return record.approval === null
        ? failure("storage_failure", "native approval projection is missing")
        : { ok: true, value: { operation: request.operation, approval: record.approval } };
    }
    case "native-approval.list.v1": {
      const rows = state.database.all(
        `SELECT public_json FROM workspace_native_interactions
         WHERE project_id = ? AND context_id = ? AND kind != 'user_input'
           AND (? IS NULL OR state = ?)
         ORDER BY updated_at DESC LIMIT ?`,
        JsonRowSchema,
        [request.projectId, request.contextId, request.state, request.state, request.limit],
      );
      const approvals = rows
        .map((row) => state.parse(row.public_json, NativeInteractionRecordSchema).approval)
        .filter((approval) => approval !== null);
      return { ok: true, value: { operation: request.operation, approvals } };
    }
    case "session.get.v1": {
      const row = scopedRow(
        state,
        "workspace_sessions",
        "session_id",
        request.projectId,
        request.sessionId,
      );
      return row === null
        ? failure("not_found", "coordinator session does not exist")
        : {
            ok: true,
            value: {
              operation: request.operation,
              session: state.parse(row.public_json, CoordinatorSessionSchema),
            },
          };
    }
    case "session.list.v1": {
      const sessions = listRows(
        state,
        "workspace_sessions",
        request.projectId,
        request.contextId,
      ).map((row) => state.parse(row.public_json, CoordinatorSessionSchema));
      return { ok: true, value: { operation: request.operation, sessions } };
    }
    case "agent.list.v1": {
      const agents = listRows(state, "workspace_agents", request.projectId, request.contextId).map(
        (row) => state.parse(row.public_json, AgentDescriptorSchema),
      );
      return { ok: true, value: { operation: request.operation, agents } };
    }
    case "agent.network.v1": {
      const agents = listRows(state, "workspace_agents", request.projectId, request.contextId).map(
        (row) => state.parse(row.public_json, AgentDescriptorSchema),
      );
      const relationships = listRows(
        state,
        "workspace_agent_relationships",
        request.projectId,
        request.contextId,
      ).map((row) => state.parse(row.public_json, AgentRelationshipSchema));
      return {
        ok: true,
        value: {
          operation: request.operation,
          network: AgentNetworkSchema.parse({
            projectId: request.projectId,
            contextId: request.contextId,
            agents,
            relationships,
            coverage: { state: "complete" },
          }),
        },
      };
    }
    case "agent.output.page.v1": {
      const actor = scopedRow(
        state,
        "workspace_agents",
        "actor_id",
        request.projectId,
        request.actorId,
      );
      if (actor === null) return failure("not_found", "actor does not exist in this project");
      const rows = state.database.all(
        `SELECT public_json FROM workspace_agent_output
         WHERE project_id = ? AND context_id = ? AND actor_id = ? AND sequence > ?
         ORDER BY sequence LIMIT ?`,
        JsonRowSchema,
        [
          request.projectId,
          request.contextId,
          request.actorId,
          BigInt(request.afterSequence),
          request.limit + 1,
        ],
      );
      const more = rows.length > request.limit;
      const items = rows
        .slice(0, request.limit)
        .map((row) => state.parse(row.public_json, AgentOutputItemSchema));
      return {
        ok: true,
        value: {
          operation: request.operation,
          page: AgentOutputPageSchema.parse({
            items,
            afterSequence: request.afterSequence,
            nextSequence: more ? items.at(-1)?.sequence : null,
          }),
        },
      };
    }
    case "terminal.list.v1":
    case "terminal.output.page.v1":
      return failure(
        "unsupported_operation",
        "managed terminal reads require the shared application runtime",
      );
    case "model-policy.get.v1":
    case "model-policy.preview.v1":
    case "model-policy.history.v1":
    case "model-selection.get.v1":
      return failure(
        "unsupported_operation",
        "model policy reads require the shared application runtime",
      );
  }
}

export function eventsWorkspace(
  state: WorkspaceState,
  access: WorkspaceAccessContext,
  request: WorkspaceEventsRequest,
): WorkspaceResult<HistoryPage> {
  const scope = request.cursor.scope;
  const selected =
    scope.kind === "all_authorized" ? access.authorizedProjectIds : [scope.projectId];
  if (selected.some((projectId) => !authorized(access, projectId))) {
    return failure("forbidden", "history scope contains an unauthorized project");
  }
  const placeholders = selected.map(() => "?").join(", ");
  const clauses = [`project_id IN (${placeholders})`, "global_sequence > ?"];
  const parameters: Array<string | bigint | number> = [
    ...selected,
    BigInt(request.cursor.afterGlobalSequence),
  ];
  if (scope.kind === "context" || scope.kind === "actor") {
    clauses.push("context_id = ?");
    parameters.push(scope.contextId);
  }
  if (scope.kind === "actor") {
    clauses.push("actor_id = ?");
    parameters.push(scope.actorId);
  }
  parameters.push(request.limit + 1);
  const rows = state.database.all(
    `SELECT public_json FROM workspace_history WHERE ${clauses.join(" AND ")}
     ORDER BY global_sequence LIMIT ?`,
    JsonRowSchema,
    parameters,
  );
  const more = rows.length > request.limit;
  const events = rows
    .slice(0, request.limit)
    .map((row) => state.parse(row.public_json, HistoryEventSchema));
  const last = events.at(-1)?.globalSequence ?? request.cursor.afterGlobalSequence;
  return {
    ok: true,
    value: HistoryPageSchema.parse({
      events,
      resume: { scope, afterGlobalSequence: last },
      next: more ? { scope, afterGlobalSequence: last } : null,
      coverage: { state: "complete" },
    }),
  };
}

function authorized(access: WorkspaceAccessContext, projectId: ProjectId): boolean {
  return access.authorizedProjectIds.includes(projectId);
}

function scopedRow(
  state: WorkspaceState,
  table: string,
  key: string,
  projectId: string,
  value: string,
) {
  const allowed = new Set([
    "workspace_contexts",
    "workspace_sessions",
    "workspace_agents",
    "workspace_project_execution",
  ]);
  const keys = new Set(["context_id", "session_id", "actor_id"]);
  if (!allowed.has(table) || !keys.has(key)) return null;
  return state.database.get(
    `SELECT public_json FROM ${table} WHERE project_id = ? AND ${key} = ?`,
    JsonRowSchema,
    [projectId, value],
  );
}

function questionRow(state: WorkspaceState, projectId: string, contextId: string, groupId: string) {
  return state.database.get(
    `SELECT public_json FROM workspace_questions
     WHERE project_id = ? AND context_id = ? AND question_group_id = ?`,
    JsonRowSchema,
    [projectId, contextId, groupId],
  );
}

function listRows(state: WorkspaceState, table: string, projectId: string, contextId: string) {
  const allowed = new Set([
    "workspace_sessions",
    "workspace_agents",
    "workspace_agent_relationships",
  ]);
  return allowed.has(table)
    ? state.database.all(
        `SELECT public_json FROM ${table} WHERE project_id = ? AND context_id = ? ORDER BY rowid LIMIT 512`,
        JsonRowSchema,
        [projectId, contextId],
      )
    : [];
}
