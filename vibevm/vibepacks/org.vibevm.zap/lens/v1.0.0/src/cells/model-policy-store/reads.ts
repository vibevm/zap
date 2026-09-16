/** @scope spec://org.vibevm.zap/lens/PROP-008#policy-lifecycle */
import { z } from "zod";
import { resolveModelSelection } from "../model-policy/index.ts";
import {
  ModelPolicyStoreAccessSchema,
  ModelPolicyVersionSchema,
  ModelPolicyChangeSchema,
  ResolveModelPreviewRequestSchema,
  StoredModelPolicySchema,
  type ModelPolicyStoreAccess,
  type ModelPolicyStoreResult,
  type ModelPolicyVersion,
  type ModelPolicyChange,
  type ResolveModelPreviewRequest,
  type StoredModelPolicy,
} from "./types.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import type { ModelPolicyDatabase } from "./database.ts";

const PolicyRowSchema = z.object({
  project_id: z.string(),
  context_id: z.string(),
  revision: z.bigint(),
  policy_json: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
});
const VersionRowSchema = z.object({
  project_id: z.string(),
  context_id: z.string(),
  revision: z.bigint(),
  policy_json: z.string(),
  source_event_id: z.string(),
  actor_id: z.string().nullable(),
  changed_at: z.string(),
});
const ChangeRowSchema = z.object({
  change_id: z.string(),
  project_id: z.string(),
  context_id: z.string(),
  source_event_id: z.string(),
  principal_id: z.string(),
  actor_id: z.string().nullable(),
  client_id: z.string(),
  from_revision: z.bigint().nullable(),
  to_revision: z.bigint(),
  changed_at: z.string(),
});

function failure(
  code: "invalid_input" | "unauthorized" | "forbidden" | "not_found" | "storage_failure",
  message: string,
): ModelPolicyStoreResult<never> {
  return { ok: false, error: { code, message } };
}
function json(raw: string): unknown {
  const value: unknown = JSON.parse(raw);
  return value;
}

export function readPolicy(
  db: ModelPolicyDatabase,
  projectId: ProjectId,
  contextId: WorkContextId,
): ModelPolicyStoreResult<StoredModelPolicy> {
  try {
    const row = db.get(
      "SELECT project_id, context_id, revision, policy_json, created_at, updated_at FROM model_policies WHERE project_id = ? AND context_id = ?",
      PolicyRowSchema,
      [projectId, contextId],
    );
    return row === null
      ? failure("not_found", "model policy does not exist")
      : {
          ok: true,
          value: StoredModelPolicySchema.parse({
            projectId: row.project_id,
            contextId: row.context_id,
            policy: json(row.policy_json),
            createdAt: row.created_at,
            updatedAt: row.updated_at,
          }),
        };
  } catch {
    return failure("storage_failure", "model policy read failed");
  }
}

export function listPolicyVersions(
  db: ModelPolicyDatabase,
  projectId: ProjectId,
  contextId: WorkContextId,
): ModelPolicyStoreResult<readonly ModelPolicyVersion[]> {
  try {
    return {
      ok: true,
      value: db
        .all(
          "SELECT project_id, context_id, revision, policy_json, source_event_id, actor_id, changed_at FROM model_policy_versions WHERE project_id = ? AND context_id = ? ORDER BY revision",
          VersionRowSchema,
          [projectId, contextId],
        )
        .map((row) =>
          ModelPolicyVersionSchema.parse({
            projectId: row.project_id,
            contextId: row.context_id,
            revision: String(row.revision),
            policy: json(row.policy_json),
            sourceEventId: row.source_event_id,
            actorId: row.actor_id,
            changedAt: row.changed_at,
          }),
        ),
    };
  } catch {
    return failure("storage_failure", "model policy versions read failed");
  }
}

export function listChanges(
  db: ModelPolicyDatabase,
  projectId: ProjectId,
  contextId: WorkContextId,
): ModelPolicyStoreResult<readonly ModelPolicyChange[]> {
  try {
    return {
      ok: true,
      value: db
        .all(
          "SELECT change_id, project_id, context_id, source_event_id, principal_id, actor_id, client_id, from_revision, to_revision, changed_at FROM model_policy_changes WHERE project_id = ? AND context_id = ? ORDER BY to_revision",
          ChangeRowSchema,
          [projectId, contextId],
        )
        .map((row) =>
          ModelPolicyChangeSchema.parse({
            changeId: row.change_id,
            projectId: row.project_id,
            contextId: row.context_id,
            sourceEventId: row.source_event_id,
            principalId: row.principal_id,
            actorId: row.actor_id,
            clientId: row.client_id,
            fromRevision: row.from_revision === null ? null : String(row.from_revision),
            toRevision: String(row.to_revision),
            changedAt: row.changed_at,
          }),
        ),
    };
  } catch {
    return failure("storage_failure", "model policy changes read failed");
  }
}

export function resolvePreview(
  db: ModelPolicyDatabase,
  access: ModelPolicyStoreAccess,
  rawRequest: ResolveModelPreviewRequest,
): ModelPolicyStoreResult<ReturnType<typeof resolveModelSelection>> {
  const request = ResolveModelPreviewRequestSchema.safeParse(rawRequest);
  if (!request.success)
    return failure("invalid_input", "model selection preview request is malformed");
  const trusted = ModelPolicyStoreAccessSchema.safeParse(access);
  if (!trusted.success || !trusted.data.authorizedProjectIds.includes(request.data.projectId))
    return failure("forbidden", "project is outside trusted model policy scope");
  const policy = readPolicy(db, request.data.projectId, request.data.contextId);
  if (!policy.ok) return policy;
  return {
    ok: true,
    value: resolveModelSelection(
      policy.value.policy,
      request.data.request,
      request.data.trustedContext,
    ),
  };
}

export { PolicyRowSchema };
