/** @scope spec://org.vibevm.zap/lens/PROP-008#policy-lifecycle */
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import {
  ModelPolicyStoreAccessSchema,
  InitializeDefaultPolicyRequestSchema,
  StoredModelPolicySchema,
  StoreModelSelectionRequestSchema,
  StoredModelSelectionSchema,
  UpdateModelPolicyRequestSchema,
  type InitializeDefaultPolicyRequest,
  type ModelPolicyChange,
  type ModelPolicyStore,
  type ModelPolicyStoreAccess,
  type ModelPolicyStoreResult,
  type ModelPolicyVersion,
  type ResolveModelPreviewRequest,
  type StoredModelPolicy,
  type StoredModelSelection,
  type StoreModelSelectionRequest,
  type UpdateModelPolicyRequest,
} from "./types.ts";
import { createDefaultCodexModelPolicy, ModelPolicySchema } from "../model-policy/index.ts";
import type { ModelPolicyResult, ModelSelection } from "../model-policy/index.ts";
import {
  type AttemptId,
  type ProjectId,
  type RunId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import type { ClientRequestId } from "../protocol/index.ts";
import { ModelPolicyDatabase } from "./database.ts";
import * as policyReads from "./reads.ts";

const PolicyRowSchema = z.object({
  project_id: z.string(),
  context_id: z.string(),
  revision: z.bigint(),
  policy_json: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
});
const SelectionRowSchema = z.object({
  project_id: z.string(),
  context_id: z.string(),
  run_id: z.string(),
  attempt_id: z.string(),
  selection_json: z.string(),
  source_event_id: z.string(),
  actor_id: z.string().nullable(),
  stored_at: z.string(),
});
const IdempotencyRowSchema = z.object({ request_digest: z.string(), result_json: z.string() });

function failure(
  code: Parameters<typeof error>[0],
  message: string,
): ModelPolicyStoreResult<never> {
  return { ok: false, error: error(code, message) };
}
function error(
  code:
    | "invalid_input"
    | "unauthorized"
    | "forbidden"
    | "not_found"
    | "conflict"
    | "stale_revision"
    | "idempotency_conflict"
    | "storage_failure"
    | "closed",
  message: string,
) {
  return { code, message } as const;
}

export class SqliteModelPolicyStore implements ModelPolicyStore {
  readonly #database: ModelPolicyDatabase;
  readonly #clock: () => Date;
  readonly #idFactory: (kind: string) => string;
  #closed = false;

  constructor(options: {
    readonly databasePath: string;
    readonly clock?: () => Date;
    readonly idFactory?: (kind: string) => string;
  }) {
    this.#database = new ModelPolicyDatabase(options.databasePath);
    this.#clock = options.clock ?? (() => new Date());
    this.#idFactory = options.idFactory ?? ((kind) => `${kind}.${randomUUID()}`);
  }

  initializeDefaultPolicy(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: InitializeDefaultPolicyRequest,
  ): ModelPolicyStoreResult<StoredModelPolicy> {
    const access = this.#access(rawAccess, rawRequest.projectId);
    if (!access.ok) return access;
    const request = InitializeDefaultPolicyRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "default model policy request is malformed");
    const digest = this.#digest(request.data);
    const prior = this.#idempotent(
      access.value,
      request.data.clientRequestId,
      "initialize",
      request.data.projectId,
      request.data.contextId,
      digest,
      StoredModelPolicySchema,
    );
    if (prior !== null) return prior;
    try {
      return this.#database.transaction(() => {
        const current = this.#policyRow(request.data.projectId, request.data.contextId);
        if (current !== null)
          return failure("conflict", "model policy already exists for this context");
        const now = this.#clock().toISOString();
        const policy = createDefaultCodexModelPolicy({
          policyId: request.data.policyId,
          revision: "1",
        });
        const stored = StoredModelPolicySchema.parse({
          projectId: request.data.projectId,
          contextId: request.data.contextId,
          policy,
          createdAt: now,
          updatedAt: now,
        });
        this.#insertPolicy(stored);
        this.#insertVersion(stored, request.data.sourceEventId, access.value.actorId, now);
        this.#insertChange(
          access.value,
          request.data.projectId,
          request.data.contextId,
          request.data.sourceEventId,
          null,
          "1",
          now,
        );
        this.#saveIdempotency(
          access.value,
          request.data.clientRequestId,
          "initialize",
          request.data.projectId,
          request.data.contextId,
          digest,
          stored,
        );
        return { ok: true, value: stored };
      });
    } catch {
      return failure("storage_failure", "default model policy transaction failed");
    }
  }

  updatePolicy(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: UpdateModelPolicyRequest,
  ): ModelPolicyStoreResult<StoredModelPolicy> {
    const access = this.#access(rawAccess, rawRequest.projectId);
    if (!access.ok) return access;
    const request = UpdateModelPolicyRequestSchema.safeParse(rawRequest);
    if (!request.success || !ModelPolicySchema.safeParse(request.data.policy).success)
      return failure("invalid_input", "model policy update is malformed");
    const digest = this.#digest(request.data);
    const prior = this.#idempotent(
      access.value,
      request.data.clientRequestId,
      "update",
      request.data.projectId,
      request.data.contextId,
      digest,
      StoredModelPolicySchema,
    );
    if (prior !== null) return prior;
    try {
      return this.#database.transaction(() => {
        const current = this.#policyRow(request.data.projectId, request.data.contextId);
        if (current === null) return failure("not_found", "model policy does not exist");
        if (String(current.revision) !== request.data.expectedRevision)
          return failure("stale_revision", "model policy revision changed");
        const next = BigInt(request.data.expectedRevision) + 1n;
        if (BigInt(request.data.policy.revision) !== next)
          return failure("conflict", "policy revision must advance exactly once");
        if (
          this.#sourceExists(
            request.data.projectId,
            request.data.contextId,
            request.data.sourceEventId,
          )
        )
          return failure("conflict", "source event identity was already committed");
        const now = this.#clock().toISOString();
        const stored = StoredModelPolicySchema.parse({
          projectId: request.data.projectId,
          contextId: request.data.contextId,
          policy: request.data.policy,
          createdAt: current.created_at,
          updatedAt: now,
        });
        this.#insertPolicy(stored);
        this.#insertVersion(stored, request.data.sourceEventId, access.value.actorId, now);
        this.#insertChange(
          access.value,
          request.data.projectId,
          request.data.contextId,
          request.data.sourceEventId,
          request.data.expectedRevision,
          request.data.policy.revision,
          now,
        );
        this.#saveIdempotency(
          access.value,
          request.data.clientRequestId,
          "update",
          request.data.projectId,
          request.data.contextId,
          digest,
          stored,
        );
        return { ok: true, value: stored };
      });
    } catch {
      return failure("storage_failure", "model policy update transaction failed");
    }
  }

  readPolicy(
    rawAccess: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): ModelPolicyStoreResult<StoredModelPolicy> {
    const access = this.#access(rawAccess, projectId);
    return access.ok ? policyReads.readPolicy(this.#database, projectId, contextId) : access;
  }

  listPolicyVersions(
    rawAccess: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): ModelPolicyStoreResult<readonly ModelPolicyVersion[]> {
    const access = this.#access(rawAccess, projectId);
    return access.ok
      ? policyReads.listPolicyVersions(this.#database, projectId, contextId)
      : access;
  }

  listChanges(
    rawAccess: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
  ): ModelPolicyStoreResult<readonly ModelPolicyChange[]> {
    const access = this.#access(rawAccess, projectId);
    return access.ok ? policyReads.listChanges(this.#database, projectId, contextId) : access;
  }

  resolvePreview(
    rawAccess: ModelPolicyStoreAccess,
    request: ResolveModelPreviewRequest,
  ): ModelPolicyStoreResult<ModelPolicyResult<ModelSelection>> {
    const access = this.#access(rawAccess, request.projectId);
    return access.ok ? policyReads.resolvePreview(this.#database, access.value, request) : access;
  }

  storeSelection(
    rawAccess: ModelPolicyStoreAccess,
    rawRequest: StoreModelSelectionRequest,
  ): ModelPolicyStoreResult<StoredModelSelection> {
    const access = this.#access(rawAccess, rawRequest.projectId);
    if (!access.ok) return access;
    const request = StoreModelSelectionRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "model selection request is malformed");
    const digest = this.#digest(request.data);
    const prior = this.#idempotent(
      access.value,
      request.data.clientRequestId,
      "selection",
      request.data.projectId,
      request.data.contextId,
      digest,
      StoredModelSelectionSchema,
    );
    if (prior !== null) return prior;
    try {
      return this.#database.transaction(() => {
        const existing = this.#database.get(
          "SELECT project_id, context_id, run_id, attempt_id, selection_json, source_event_id, actor_id, stored_at FROM model_selections WHERE project_id = ? AND context_id = ? AND run_id = ? AND attempt_id = ?",
          SelectionRowSchema,
          [
            request.data.projectId,
            request.data.contextId,
            request.data.runId,
            request.data.attemptId,
          ],
        );
        if (existing !== null)
          return failure("conflict", "a model selection is already pinned for this attempt");
        if (
          this.#sourceExists(
            request.data.projectId,
            request.data.contextId,
            request.data.sourceEventId,
          )
        )
          return failure("conflict", "source event identity was already committed");
        const stored = StoredModelSelectionSchema.parse({
          projectId: request.data.projectId,
          contextId: request.data.contextId,
          runId: request.data.runId,
          attemptId: request.data.attemptId,
          selection: request.data.selection,
          storedAt: this.#clock().toISOString(),
          sourceEventId: request.data.sourceEventId,
          actorId: access.value.actorId,
        });
        this.#database.run(
          "INSERT INTO model_selections(project_id, context_id, run_id, attempt_id, selection_json, source_event_id, actor_id, stored_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
          [
            stored.projectId,
            stored.contextId,
            stored.runId,
            stored.attemptId,
            JSON.stringify(stored.selection),
            stored.sourceEventId,
            stored.actorId,
            stored.storedAt,
          ],
        );
        this.#saveIdempotency(
          access.value,
          request.data.clientRequestId,
          "selection",
          request.data.projectId,
          request.data.contextId,
          digest,
          stored,
        );
        return { ok: true, value: stored };
      });
    } catch {
      return failure("storage_failure", "model selection transaction failed");
    }
  }

  readSelection(
    rawAccess: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
    runId: RunId,
    attemptId: AttemptId,
  ): ModelPolicyStoreResult<StoredModelSelection> {
    const access = this.#access(rawAccess, projectId);
    if (!access.ok) return access;
    try {
      const row = this.#database.get(
        "SELECT project_id, context_id, run_id, attempt_id, selection_json, source_event_id, actor_id, stored_at FROM model_selections WHERE project_id = ? AND context_id = ? AND run_id = ? AND attempt_id = ?",
        SelectionRowSchema,
        [projectId, contextId, runId, attemptId],
      );
      return row === null
        ? failure("not_found", "model selection does not exist")
        : {
            ok: true,
            value: StoredModelSelectionSchema.parse({
              projectId: row.project_id,
              contextId: row.context_id,
              runId: row.run_id,
              attemptId: row.attempt_id,
              selection: this.#json(row.selection_json),
              sourceEventId: row.source_event_id,
              actorId: row.actor_id,
              storedAt: row.stored_at,
            }),
          };
    } catch {
      return failure("storage_failure", "model selection read failed");
    }
  }

  close(): ModelPolicyStoreResult<null> {
    if (this.#closed) return { ok: true, value: null };
    try {
      this.#database.close();
      this.#closed = true;
      return { ok: true, value: null };
    } catch {
      return failure("storage_failure", "model policy store close failed");
    }
  }

  #access(
    raw: ModelPolicyStoreAccess,
    projectId: ProjectId,
  ): ModelPolicyStoreResult<ModelPolicyStoreAccess> {
    if (this.#closed) return failure("closed", "model policy store is closed");
    const access = ModelPolicyStoreAccessSchema.safeParse(raw);
    return !access.success
      ? failure("unauthorized", "trusted model policy access is malformed")
      : access.data.authorizedProjectIds.includes(projectId)
        ? { ok: true, value: access.data }
        : failure("forbidden", "project is outside trusted model policy scope");
  }
  #digest(value: unknown): string {
    return createHash("sha256").update(JSON.stringify(value)).digest("hex");
  }
  #policyRow(projectId: ProjectId, contextId: WorkContextId) {
    return this.#database.get(
      "SELECT project_id, context_id, revision, policy_json, created_at, updated_at FROM model_policies WHERE project_id = ? AND context_id = ?",
      PolicyRowSchema,
      [projectId, contextId],
    );
  }
  #insertPolicy(stored: StoredModelPolicy): void {
    this.#database.run(
      "INSERT INTO model_policies(project_id, context_id, revision, policy_json, created_at, updated_at) VALUES(?, ?, ?, ?, ?, ?) ON CONFLICT(project_id, context_id) DO UPDATE SET revision = excluded.revision, policy_json = excluded.policy_json, updated_at = excluded.updated_at",
      [
        stored.projectId,
        stored.contextId,
        BigInt(stored.policy.revision),
        JSON.stringify(stored.policy),
        stored.createdAt,
        stored.updatedAt,
      ],
    );
  }
  #insertVersion(
    stored: StoredModelPolicy,
    sourceEventId: string,
    actorId: string | null,
    changedAt: string,
  ): void {
    this.#database.run(
      "INSERT INTO model_policy_versions(project_id, context_id, revision, policy_json, source_event_id, actor_id, changed_at) VALUES(?, ?, ?, ?, ?, ?, ?)",
      [
        stored.projectId,
        stored.contextId,
        BigInt(stored.policy.revision),
        JSON.stringify(stored.policy),
        sourceEventId,
        actorId,
        changedAt,
      ],
    );
  }
  #insertChange(
    access: ModelPolicyStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
    sourceEventId: string,
    fromRevision: string | null,
    toRevision: string,
    changedAt: string,
  ): void {
    this.#database.run(
      "INSERT INTO model_policy_changes(change_id, project_id, context_id, source_event_id, principal_id, actor_id, client_id, from_revision, to_revision, changed_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
      [
        this.#idFactory("policy-change"),
        projectId,
        contextId,
        sourceEventId,
        access.principalId,
        access.actorId,
        access.clientId,
        fromRevision === null ? null : BigInt(fromRevision),
        BigInt(toRevision),
        changedAt,
      ],
    );
  }
  #sourceExists(projectId: ProjectId, contextId: WorkContextId, sourceEventId: string): boolean {
    return (
      this.#database.get(
        "SELECT source_event_id FROM model_policy_changes WHERE project_id = ? AND context_id = ? AND source_event_id = ? UNION SELECT source_event_id FROM model_policy_versions WHERE project_id = ? AND context_id = ? AND source_event_id = ? UNION SELECT source_event_id FROM model_selections WHERE project_id = ? AND context_id = ? AND source_event_id = ?",
        z.object({ source_event_id: z.string() }),
        [
          projectId,
          contextId,
          sourceEventId,
          projectId,
          contextId,
          sourceEventId,
          projectId,
          contextId,
          sourceEventId,
        ],
      ) !== null
    );
  }
  #idempotent<T>(
    access: ModelPolicyStoreAccess,
    requestId: ClientRequestId,
    operation: string,
    projectId: ProjectId,
    contextId: WorkContextId,
    digest: string,
    schema: z.ZodType<T>,
  ): ModelPolicyStoreResult<T> | null {
    const row = this.#database.get(
      "SELECT request_digest, result_json FROM model_policy_idempotency WHERE principal_id = ? AND actor_key = ? AND project_id = ? AND context_id = ? AND client_request_id = ? AND operation = ?",
      IdempotencyRowSchema,
      [
        access.principalId,
        access.actorId ?? `client:${access.clientId}`,
        projectId,
        contextId,
        requestId,
        operation,
      ],
    );
    if (row === null) return null;
    return row.request_digest === digest
      ? { ok: true, value: schema.parse(this.#json(row.result_json)) }
      : failure(
          "idempotency_conflict",
          "model policy request identity was reused with new content",
        );
  }
  #saveIdempotency(
    access: ModelPolicyStoreAccess,
    requestId: ClientRequestId,
    operation: string,
    projectId: ProjectId,
    contextId: WorkContextId,
    digest: string,
    result: unknown,
  ): void {
    this.#database.run(
      "INSERT INTO model_policy_idempotency(principal_id, actor_key, project_id, context_id, client_request_id, operation, request_digest, result_json) VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
      [
        access.principalId,
        access.actorId ?? `client:${access.clientId}`,
        projectId,
        contextId,
        requestId,
        operation,
        digest,
        JSON.stringify(result),
      ],
    );
  }

  #json(raw: string): unknown {
    const value: unknown = JSON.parse(raw);
    return value;
  }
}
