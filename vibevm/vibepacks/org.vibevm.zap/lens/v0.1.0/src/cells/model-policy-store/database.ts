/** @scope spec://org.vibevm.zap/lens/PROP-008#policy-lifecycle */
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import type { ZodType } from "zod";

const SCHEMA = `
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS model_policies (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  policy_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(project_id, context_id)
);
CREATE TABLE IF NOT EXISTS model_policy_versions (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  policy_json TEXT NOT NULL,
  source_event_id TEXT NOT NULL,
  actor_id TEXT,
  changed_at TEXT NOT NULL,
  PRIMARY KEY(project_id, context_id, revision),
  UNIQUE(project_id, context_id, source_event_id)
);
CREATE TABLE IF NOT EXISTS model_policy_changes (
  change_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  source_event_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  actor_id TEXT,
  client_id TEXT NOT NULL,
  from_revision INTEGER,
  to_revision INTEGER NOT NULL,
  changed_at TEXT NOT NULL,
  UNIQUE(project_id, context_id, source_event_id)
);
CREATE TABLE IF NOT EXISTS model_selections (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  attempt_id TEXT NOT NULL,
  selection_json TEXT NOT NULL,
  source_event_id TEXT NOT NULL,
  actor_id TEXT,
  stored_at TEXT NOT NULL,
  PRIMARY KEY(project_id, context_id, run_id, attempt_id),
  UNIQUE(project_id, context_id, source_event_id)
);
CREATE TABLE IF NOT EXISTS model_policy_idempotency (
  principal_id TEXT NOT NULL,
  actor_key TEXT NOT NULL,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  client_request_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  result_json TEXT NOT NULL,
  PRIMARY KEY(principal_id, actor_key, project_id, context_id, client_request_id, operation)
);
`;

export class ModelPolicyDatabase {
  readonly sqlite: DatabaseSync;

  constructor(path: string) {
    if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
    this.sqlite = new DatabaseSync(path);
    this.sqlite.exec("PRAGMA busy_timeout = 5000;");
    if (path !== ":memory:") {
      this.sqlite.exec("PRAGMA journal_mode = WAL;");
      this.sqlite.exec("PRAGMA synchronous = FULL;");
    }
    this.sqlite.exec(SCHEMA);
  }

  transaction<T>(work: () => T): T {
    this.sqlite.exec("BEGIN IMMEDIATE;");
    try {
      const result = work();
      this.sqlite.exec("COMMIT;");
      return result;
    } catch (cause: unknown) {
      try {
        this.sqlite.exec("ROLLBACK;");
      } catch {
        /* preserve original */
      }
      throw cause;
    }
  }

  run(sql: string, values: readonly SQLInputValue[] = []): void {
    this.sqlite.prepare(sql).run(...values);
  }

  get<T>(sql: string, schema: ZodType<T>, values: readonly SQLInputValue[] = []): T | null {
    const statement = this.sqlite.prepare(sql);
    statement.setReadBigInts(true);
    const row = statement.get(...values);
    return row === undefined ? null : schema.parse(row);
  }

  all<T>(sql: string, schema: ZodType<T>, values: readonly SQLInputValue[] = []): readonly T[] {
    const statement = this.sqlite.prepare(sql);
    statement.setReadBigInts(true);
    return statement.all(...values).map((row) => schema.parse(row));
  }

  close(): void {
    this.sqlite.close();
  }
}
