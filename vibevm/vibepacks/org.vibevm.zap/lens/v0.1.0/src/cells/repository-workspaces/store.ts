/** Protected durable repository-workspace state. @scope spec://org.vibevm.zap/lens/PROP-014#recovery */
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync, type StatementSync } from "node:sqlite";
import { z } from "zod";
import {
  IntegrationAttemptSchema,
  ProjectPlanRecordSchema,
  RepositoryProjectBindingSchema,
  RepositoryRecordSchema,
  RepositoryWorktreeRecordSchema,
  type IntegrationAttempt,
  type ProjectPlanRecord,
} from "../repository-model/index.ts";

const ProtectedRepositorySchema = z
  .object({
    record: RepositoryRecordSchema,
    commonDirectory: z.string().min(1),
    topLevel: z.string().min(1),
  })
  .strict();
export type ProtectedRepository = z.infer<typeof ProtectedRepositorySchema>;
const ProtectedBindingSchema = z
  .object({
    record: RepositoryProjectBindingSchema,
    projectDirectory: z.string().min(1),
    projectRelativePath: z.string(),
  })
  .strict();
export type ProtectedBinding = z.infer<typeof ProtectedBindingSchema>;
const ProtectedWorktreeSchema = z
  .object({
    record: RepositoryWorktreeRecordSchema,
    directory: z.string().min(1),
    projectDirectory: z.string().min(1),
  })
  .strict();
export type ProtectedWorktree = z.infer<typeof ProtectedWorktreeSchema>;

const OperationSchema = z
  .object({
    principalId: z.string().min(1),
    scope: z.string().min(1),
    requestId: z.string().min(1),
    digest: z.string().length(64),
    state: z.enum(["pending", "complete", "failed"]),
    resultKind: z.enum(["repository", "plan", "worktree", "integration"]).nullable(),
    resultId: z.string().nullable(),
    errorCode: z.string().nullable(),
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
  })
  .strict();
export type RepositoryOperation = z.infer<typeof OperationSchema>;

type StoredValue =
  | ProtectedRepository
  | ProtectedBinding
  | ProtectedWorktree
  | ProjectPlanRecord
  | IntegrationAttempt;
export interface RepositoryWorkspaceStore {
  findRepositoryByHostPath(hostId: string, commonDirectory: string): ProtectedRepository | null;
  getRepository(id: string): ProtectedRepository | null;
  putRepository(value: ProtectedRepository): boolean;
  getBinding(projectId: string): ProtectedBinding | null;
  putBinding(value: ProtectedBinding): boolean;
  getPlan(id: string): ProjectPlanRecord | null;
  listPlans(projectId: string): readonly ProjectPlanRecord[];
  putPlan(value: ProjectPlanRecord): boolean;
  getWorktree(id: string): ProtectedWorktree | null;
  listWorktrees(planId: string): readonly ProtectedWorktree[];
  putWorktree(value: ProtectedWorktree): boolean;
  getIntegration(id: string): IntegrationAttempt | null;
  listIntegrations(planId: string): readonly IntegrationAttempt[];
  putIntegration(value: IntegrationAttempt): boolean;
  getOperation(principalId: string, scope: string, requestId: string): RepositoryOperation | null;
  putOperation(value: RepositoryOperation): boolean;
  close(): void;
}

export function openRepositoryWorkspaceStore(path: string): RepositoryWorkspaceStore {
  if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
  const database = new DatabaseSync(path);
  database.exec(
    "PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL;" +
      "CREATE TABLE IF NOT EXISTS repository_workspace_records(kind TEXT NOT NULL,id TEXT NOT NULL,host_id TEXT,project_id TEXT,plan_id TEXT,path_key TEXT,value_json TEXT NOT NULL,PRIMARY KEY(kind,id));" +
      "CREATE UNIQUE INDEX IF NOT EXISTS repository_host_path ON repository_workspace_records(host_id,path_key) WHERE kind='repository';" +
      "CREATE INDEX IF NOT EXISTS repository_project_records ON repository_workspace_records(kind,project_id);" +
      "CREATE INDEX IF NOT EXISTS repository_plan_records ON repository_workspace_records(kind,plan_id);" +
      "CREATE TABLE IF NOT EXISTS repository_workspace_operations(principal_id TEXT NOT NULL,scope TEXT NOT NULL,request_id TEXT NOT NULL,digest TEXT NOT NULL,state TEXT NOT NULL,result_kind TEXT,result_id TEXT,error_code TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,PRIMARY KEY(principal_id,scope,request_id)) STRICT;",
  );
  const upsert = database.prepare(
    "INSERT INTO repository_workspace_records(kind,id,host_id,project_id,plan_id,path_key,value_json) VALUES(?,?,?,?,?,?,?) ON CONFLICT(kind,id) DO UPDATE SET host_id=excluded.host_id,project_id=excluded.project_id,plan_id=excluded.plan_id,path_key=excluded.path_key,value_json=excluded.value_json",
  );
  const byId = database.prepare(
    "SELECT value_json FROM repository_workspace_records WHERE kind=? AND id=?",
  );
  return {
    findRepositoryByHostPath(hostId, commonDirectory) {
      return read(
        database
          .prepare(
            "SELECT value_json FROM repository_workspace_records WHERE kind='repository' AND host_id=? AND path_key=?",
          )
          .get(hostId, commonDirectory),
        ProtectedRepositorySchema,
      );
    },
    getRepository: (id) => read(byId.get("repository", id), ProtectedRepositorySchema),
    putRepository(value) {
      return put(
        upsert,
        "repository",
        value.record.repositoryId,
        value.record.executionHostId,
        null,
        null,
        value.commonDirectory,
        value,
      );
    },
    getBinding: (id) => read(byId.get("binding", id), ProtectedBindingSchema),
    putBinding(value) {
      return put(
        upsert,
        "binding",
        value.record.projectId,
        value.record.executionHostId,
        value.record.projectId,
        null,
        null,
        value,
      );
    },
    getPlan: (id) => read(byId.get("plan", id), ProjectPlanRecordSchema),
    listPlans: (projectId) =>
      list(database, "project_id", "plan", projectId, ProjectPlanRecordSchema),
    putPlan(value) {
      return put(
        upsert,
        "plan",
        value.planId,
        value.executionHostId,
        value.projectId,
        value.planId,
        null,
        value,
      );
    },
    getWorktree: (id) => read(byId.get("worktree", id), ProtectedWorktreeSchema),
    listWorktrees: (planId) =>
      list(database, "plan_id", "worktree", planId, ProtectedWorktreeSchema),
    putWorktree(value) {
      return put(
        upsert,
        "worktree",
        value.record.worktreeId,
        value.record.executionHostId,
        value.record.projectId,
        value.record.planId,
        null,
        value,
      );
    },
    getIntegration: (id) => read(byId.get("integration", id), IntegrationAttemptSchema),
    listIntegrations: (planId) =>
      list(database, "plan_id", "integration", planId, IntegrationAttemptSchema),
    putIntegration(value) {
      return put(
        upsert,
        "integration",
        value.integrationId,
        value.executionHostId,
        null,
        value.planId,
        null,
        value,
      );
    },
    getOperation(principalId, scope, requestId) {
      const raw = database
        .prepare(
          "SELECT principal_id,scope,request_id,digest,state,result_kind,result_id,error_code,created_at,updated_at FROM repository_workspace_operations WHERE principal_id=? AND scope=? AND request_id=?",
        )
        .get(principalId, scope, requestId);
      return decodeOperation(raw);
    },
    putOperation(value) {
      const checked = OperationSchema.safeParse(value);
      if (!checked.success) return false;
      const operation = checked.data;
      try {
        database
          .prepare(
            "INSERT INTO repository_workspace_operations(principal_id,scope,request_id,digest,state,result_kind,result_id,error_code,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(principal_id,scope,request_id) DO UPDATE SET state=excluded.state,result_kind=excluded.result_kind,result_id=excluded.result_id,error_code=excluded.error_code,updated_at=excluded.updated_at WHERE repository_workspace_operations.digest=excluded.digest",
          )
          .run(
            operation.principalId,
            operation.scope,
            operation.requestId,
            operation.digest,
            operation.state,
            operation.resultKind,
            operation.resultId,
            operation.errorCode,
            operation.createdAt,
            operation.updatedAt,
          );
        return true;
      } catch {
        return false;
      }
    },
    close() {
      database.close();
    },
  };
}

function put(
  statement: StatementSync,
  kind: string,
  id: string,
  hostId: string | null,
  projectId: string | null,
  planId: string | null,
  pathKey: string | null,
  value: StoredValue,
): boolean {
  try {
    statement.run(kind, id, hostId, projectId, planId, pathKey, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

function read<T>(raw: unknown, schema: z.ZodType<T>): T | null {
  const row = z.looseObject({ value_json: z.string() }).safeParse(raw);
  if (!row.success) return null;
  try {
    const value: unknown = JSON.parse(row.data.value_json);
    const parsed = schema.safeParse(value);
    return parsed.success ? parsed.data : null;
  } catch {
    return null;
  }
}

function list<T>(
  database: DatabaseSync,
  column: "project_id" | "plan_id",
  kind: string,
  id: string,
  schema: z.ZodType<T>,
): readonly T[] {
  const rows = database
    .prepare(
      `SELECT value_json FROM repository_workspace_records WHERE kind=? AND ${column}=? ORDER BY rowid`,
    )
    .all(kind, id);
  const values: T[] = [];
  for (const row of rows) {
    const value = read(row, schema);
    if (value !== null) values.push(value);
  }
  return values;
}

function decodeOperation(raw: unknown): RepositoryOperation | null {
  const row = z
    .looseObject({
      principal_id: z.string(),
      scope: z.string(),
      request_id: z.string(),
      digest: z.string(),
      state: z.string(),
      result_kind: z.string().nullable(),
      result_id: z.string().nullable(),
      error_code: z.string().nullable(),
      created_at: z.string(),
      updated_at: z.string(),
    })
    .safeParse(raw);
  if (!row.success) return null;
  const value: unknown = {
    principalId: row.data.principal_id,
    scope: row.data.scope,
    requestId: row.data.request_id,
    digest: row.data.digest,
    state: row.data.state,
    resultKind: row.data.result_kind,
    resultId: row.data.result_id,
    errorCode: row.data.error_code,
    createdAt: row.data.created_at,
    updatedAt: row.data.updated_at,
  };
  const parsed = OperationSchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}
