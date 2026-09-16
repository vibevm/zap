/** Durable host catalog with admin CAS, idempotency and pinned run selections. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { randomUUID } from "node:crypto";
import { z } from "zod";
import {
  StoredExecutionSelectionSchema,
  ExecutionCatalogChangeSchema,
  ExecutionCatalogStoreAccessSchema,
  ReplaceCatalogSnapshotRequestSchema,
  ReplaceCatalogSnapshotResultSchema,
  ResolveExecutionPreviewRequestSchema,
  StoreExecutionSelectionRequestSchema,
  type ExecutionCatalogChange,
  type ExecutionCatalogResult,
  type ExecutionCatalogSnapshot,
  type ExecutionCatalogStore,
  type ExecutionCatalogStoreAccess,
  type OpenExecutionCatalogStoreOptions,
  type ReplaceCatalogSnapshotRequest,
  type ResolveExecutionPreviewRequest,
  type StoreExecutionSelectionRequest,
  type StoredExecutionSelection,
  type ProjectId,
  type WorkContextId,
  type RunId,
  type AttemptId,
} from "./types.ts";
import {
  ExecutionCatalogSnapshotSchema,
  resolveExecutionSelection,
  type ExecutionCatalogError,
} from "../execution-catalog/index.ts";
import { ExecutionCatalogDatabase } from "./database.ts";

const StateRowSchema = z.object({ snapshot_json: z.string(), updated_at: z.string() });
const IdempotencyRowSchema = z.object({ request_digest: z.string(), result_json: z.string() });
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
const ChangeRowSchema = z.object({
  change_id: z.string(),
  host_id: z.string(),
  operation: z.string(),
  subject_id: z.string(),
  source_event_id: z.string(),
  principal_id: z.string(),
  actor_id: z.string().nullable(),
  client_id: z.string(),
  from_catalog_revision: z.bigint(),
  to_catalog_revision: z.bigint(),
  from_preferences_revision: z.bigint(),
  to_preferences_revision: z.bigint(),
  changed_at: z.string(),
});

export class SqliteExecutionCatalogStore implements ExecutionCatalogStore {
  readonly #database: ExecutionCatalogDatabase;
  readonly #hostId: string;
  readonly #clock: () => Date;
  readonly #idFactory: (kind: string) => string;
  #closed = false;

  constructor(options: OpenExecutionCatalogStoreOptions) {
    this.#database = new ExecutionCatalogDatabase(options.databasePath);
    this.#hostId = options.hostId;
    this.#clock = options.clock ?? (() => new Date());
    this.#idFactory = options.idFactory ?? ((kind) => `${kind}.${randomUUID()}`);
    this.#ensureState();
  }

  readSnapshot(rawAccess: ExecutionCatalogStoreAccess, projectId: ProjectId | null) {
    const access = this.#access(rawAccess, projectId, false);
    return access.ok ? this.#snapshot() : access;
  }

  replaySnapshotMutation(
    rawAccess: ExecutionCatalogStoreAccess,
    input: {
      readonly clientRequestId: string;
      readonly operation: ReplaceCatalogSnapshotRequest["operation"];
      readonly requestDigest: string;
    },
  ) {
    const access = this.#access(rawAccess, null, true);
    if (!access.ok) return access;
    const prior = this.#idempotent(
      access.value,
      input.clientRequestId,
      input.operation,
      input.requestDigest,
      ReplaceCatalogSnapshotResultSchema,
    );
    return prior ?? { ok: true as const, value: null };
  }

  replaceSnapshot(
    rawAccess: ExecutionCatalogStoreAccess,
    rawRequest: ReplaceCatalogSnapshotRequest,
  ) {
    const access = this.#access(rawAccess, null, true);
    if (!access.ok) return access;
    const request = ReplaceCatalogSnapshotRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog mutation request is malformed");
    const digest = request.data.requestDigest;
    const prior = this.#idempotent(
      access.value,
      request.data.clientRequestId,
      request.data.operation,
      digest,
      ReplaceCatalogSnapshotResultSchema,
    );
    if (prior !== null) return prior;
    try {
      return this.#database.transaction(() => {
        const current = this.#snapshot();
        if (!current.ok) return current;
        if (
          current.value.catalogRevision !== request.data.expectedCatalogRevision ||
          current.value.preferencesRevision !== request.data.expectedPreferencesRevision
        )
          return failure("stale_revision", "catalog or preferences revision changed");
        const next = request.data.snapshot;
        if (
          BigInt(next.catalogRevision) < BigInt(current.value.catalogRevision) ||
          BigInt(next.preferencesRevision) < BigInt(current.value.preferencesRevision)
        )
          return failure("invalid_input", "catalog revisions cannot move backwards");
        const now = this.#clock().toISOString();
        const change = ExecutionCatalogChangeSchema.parse({
          changeId: this.#idFactory("catalog-change"),
          hostId: this.#hostId,
          operation: request.data.operation,
          subjectId: request.data.subjectId,
          sourceEventId: request.data.sourceEventId,
          principalId: access.value.principalId,
          actorId: access.value.actorId,
          clientId: access.value.clientId,
          fromCatalogRevision: current.value.catalogRevision,
          toCatalogRevision: next.catalogRevision,
          fromPreferencesRevision: current.value.preferencesRevision,
          toPreferencesRevision: next.preferencesRevision,
          changedAt: now,
        });
        this.#database.run(
          "UPDATE execution_catalog_state SET snapshot_json = ?, updated_at = ? WHERE host_id = ?",
          [JSON.stringify(next), now, this.#hostId],
        );
        this.#insertChange(change);
        const result = { snapshot: next, change };
        this.#saveIdempotency(
          access.value,
          request.data.clientRequestId,
          request.data.operation,
          digest,
          result,
        );
        return { ok: true as const, value: result };
      });
    } catch {
      return failure("storage_failure", "catalog mutation transaction failed");
    }
  }

  listChanges(rawAccess: ExecutionCatalogStoreAccess, projectId: ProjectId | null) {
    const access = this.#access(rawAccess, projectId, false);
    if (!access.ok) return access;
    try {
      const rows = this.#database.all(
        "SELECT * FROM execution_catalog_changes WHERE host_id = ? ORDER BY changed_at, change_id",
        ChangeRowSchema,
        [this.#hostId],
      );
      return {
        ok: true as const,
        value: rows.map((row) =>
          ExecutionCatalogChangeSchema.parse({
            changeId: row.change_id,
            hostId: row.host_id,
            operation: row.operation,
            subjectId: row.subject_id,
            sourceEventId: row.source_event_id,
            principalId: row.principal_id,
            actorId: row.actor_id,
            clientId: row.client_id,
            fromCatalogRevision: row.from_catalog_revision.toString(),
            toCatalogRevision: row.to_catalog_revision.toString(),
            fromPreferencesRevision: row.from_preferences_revision.toString(),
            toPreferencesRevision: row.to_preferences_revision.toString(),
            changedAt: row.changed_at,
          }),
        ),
      };
    } catch {
      return failure("storage_failure", "catalog history read failed");
    }
  }

  resolvePreview(
    rawAccess: ExecutionCatalogStoreAccess,
    rawRequest: ResolveExecutionPreviewRequest,
  ) {
    const request = ResolveExecutionPreviewRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog preview request is malformed");
    const access = this.#access(rawAccess, request.data.projectId, false);
    if (!access.ok) return access;
    const snapshot = this.#snapshot();
    return snapshot.ok
      ? resolveExecutionSelection(snapshot.value, request.data.request, request.data.trustedContext)
      : snapshot;
  }

  replaySelection(
    rawAccess: ExecutionCatalogStoreAccess,
    input: { readonly clientRequestId: string; readonly requestDigest: string },
  ) {
    const access = this.#access(rawAccess, null, false);
    if (!access.ok) return access;
    const prior = this.#idempotent(
      access.value,
      input.clientRequestId,
      "selection",
      input.requestDigest,
      StoredExecutionSelectionSchema,
    );
    return prior ?? { ok: true as const, value: null };
  }

  storeSelection(
    rawAccess: ExecutionCatalogStoreAccess,
    rawRequest: StoreExecutionSelectionRequest,
  ) {
    const request = StoreExecutionSelectionRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "execution selection write is malformed");
    const access = this.#access(rawAccess, request.data.projectId, false);
    if (!access.ok) return access;
    const digest = request.data.requestDigest;
    const prior = this.#idempotent(
      access.value,
      request.data.clientRequestId,
      "selection",
      digest,
      StoredExecutionSelectionSchema,
    );
    if (prior !== null) return prior;
    try {
      return this.#database.transaction(() => {
        if (
          this.#selection(
            request.data.projectId,
            request.data.contextId,
            request.data.runId,
            request.data.attemptId,
          ) !== null
        )
          return failure("conflict", "an execution selection is already pinned for this attempt");
        const stored = StoredExecutionSelectionSchema.parse({
          projectId: request.data.projectId,
          contextId: request.data.contextId,
          runId: request.data.runId,
          attemptId: request.data.attemptId,
          selection: request.data.selection,
          sourceEventId: request.data.sourceEventId,
          actorId: access.value.actorId,
          storedAt: this.#clock().toISOString(),
        });
        this.#database.run(
          "INSERT INTO execution_catalog_selections(project_id, context_id, run_id, attempt_id, selection_json, source_event_id, actor_id, stored_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
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
          digest,
          stored,
        );
        return { ok: true as const, value: stored };
      });
    } catch {
      return failure("storage_failure", "execution selection transaction failed");
    }
  }

  readSelection(
    rawAccess: ExecutionCatalogStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
    runId: RunId,
    attemptId: AttemptId,
  ) {
    const access = this.#access(rawAccess, projectId, false);
    if (!access.ok) return access;
    try {
      const row = this.#selection(projectId, contextId, runId, attemptId);
      return row === null
        ? failure("not_found", "execution selection does not exist")
        : { ok: true as const, value: this.#storedSelection(row) };
    } catch {
      return failure("storage_failure", "execution selection read failed");
    }
  }

  close() {
    if (this.#closed) return { ok: true as const, value: null };
    try {
      this.#database.close();
      this.#closed = true;
      return { ok: true as const, value: null };
    } catch {
      return failure("storage_failure", "execution catalog store close failed");
    }
  }

  #ensureState(): void {
    if (
      this.#database.get(
        "SELECT snapshot_json, updated_at FROM execution_catalog_state WHERE host_id = ?",
        StateRowSchema,
        [this.#hostId],
      ) !== null
    )
      return;
    const now = this.#clock().toISOString();
    const snapshot = ExecutionCatalogSnapshotSchema.parse({
      protocol: "zap-execution-catalog/1",
      catalogRevision: "0",
      preferencesRevision: "0",
      connections: [],
      configurations: [],
      preferences: {
        economyQuality: 50,
        quota: { deprioritizeLowRemaining: false, thresholdPercent: 10, freshnessSeconds: 300 },
      },
      usage: [],
      updatedAt: now,
    });
    this.#database.run(
      "INSERT INTO execution_catalog_state(host_id, snapshot_json, updated_at) VALUES(?, ?, ?)",
      [this.#hostId, JSON.stringify(snapshot), now],
    );
  }

  #snapshot(): ExecutionCatalogResult<ExecutionCatalogSnapshot> {
    if (this.#closed) return failure("closed", "execution catalog store is closed");
    try {
      const row = this.#database.get(
        "SELECT snapshot_json, updated_at FROM execution_catalog_state WHERE host_id = ?",
        StateRowSchema,
        [this.#hostId],
      );
      return row === null
        ? failure("not_found", "execution catalog state is missing")
        : { ok: true, value: ExecutionCatalogSnapshotSchema.parse(this.#json(row.snapshot_json)) };
    } catch {
      return failure("storage_failure", "execution catalog read failed");
    }
  }

  #access(
    raw: ExecutionCatalogStoreAccess,
    projectId: ProjectId | null,
    admin: boolean,
  ): ExecutionCatalogResult<ExecutionCatalogStoreAccess> {
    if (this.#closed) return failure("closed", "execution catalog store is closed");
    const access = ExecutionCatalogStoreAccessSchema.safeParse(raw);
    if (!access.success || access.data.hostId !== this.#hostId)
      return failure("unauthorized", "trusted catalog access is malformed");
    if (projectId !== null && !access.data.authorizedProjectIds.includes(projectId))
      return failure("forbidden", "project is outside authorized catalog scope");
    if (admin && !access.data.catalogAdministrator)
      return failure("forbidden", "catalog administration authority is required");
    return { ok: true, value: access.data };
  }

  #selection(projectId: ProjectId, contextId: WorkContextId, runId: RunId, attemptId: AttemptId) {
    return this.#database.get(
      "SELECT * FROM execution_catalog_selections WHERE project_id = ? AND context_id = ? AND run_id = ? AND attempt_id = ?",
      SelectionRowSchema,
      [projectId, contextId, runId, attemptId],
    );
  }
  #storedSelection(row: z.infer<typeof SelectionRowSchema>): StoredExecutionSelection {
    return StoredExecutionSelectionSchema.parse({
      projectId: row.project_id,
      contextId: row.context_id,
      runId: row.run_id,
      attemptId: row.attempt_id,
      selection: this.#json(row.selection_json),
      sourceEventId: row.source_event_id,
      actorId: row.actor_id,
      storedAt: row.stored_at,
    });
  }
  #insertChange(change: ExecutionCatalogChange): void {
    this.#database.run(
      "INSERT INTO execution_catalog_changes VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
      [
        change.changeId,
        change.hostId,
        change.operation,
        change.subjectId,
        change.sourceEventId,
        change.principalId,
        change.actorId,
        change.clientId,
        BigInt(change.fromCatalogRevision),
        BigInt(change.toCatalogRevision),
        BigInt(change.fromPreferencesRevision),
        BigInt(change.toPreferencesRevision),
        change.changedAt,
      ],
    );
  }
  #json(raw: string): unknown {
    const value: unknown = JSON.parse(raw);
    return value;
  }
  #idempotent<T>(
    access: ExecutionCatalogStoreAccess,
    requestId: string,
    operation: string,
    digest: string,
    schema: z.ZodType<T>,
  ): ExecutionCatalogResult<T> | null {
    const row = this.#database.get(
      "SELECT request_digest, result_json FROM execution_catalog_idempotency WHERE principal_id = ? AND actor_key = ? AND host_id = ? AND client_request_id = ? AND operation = ?",
      IdempotencyRowSchema,
      [
        access.principalId,
        access.actorId ?? `client:${access.clientId}`,
        this.#hostId,
        requestId,
        operation,
      ],
    );
    if (row === null) return null;
    if (row.request_digest !== digest)
      return failure(
        "idempotency_conflict",
        "catalog request identity was reused with new content",
      );
    const value = this.#json(row.result_json);
    return { ok: true, value: schema.parse(value) };
  }
  #saveIdempotency(
    access: ExecutionCatalogStoreAccess,
    requestId: string,
    operation: string,
    digest: string,
    result: unknown,
  ): void {
    this.#database.run("INSERT INTO execution_catalog_idempotency VALUES(?, ?, ?, ?, ?, ?, ?)", [
      access.principalId,
      access.actorId ?? `client:${access.clientId}`,
      this.#hostId,
      requestId,
      operation,
      digest,
      JSON.stringify(result),
    ]);
  }
}

function failure(
  code: ExecutionCatalogError["code"],
  message: string,
): ExecutionCatalogResult<never> {
  return { ok: false, error: { code, message } };
}
