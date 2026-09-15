/** Workspace SQLite boundary. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import type { ZodType } from "zod";

const SCHEMA = `
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS workspace_projects (
  project_id TEXT PRIMARY KEY,
  public_json TEXT NOT NULL,
  coordinator_launch_options_json TEXT NOT NULL,
  default_context_id TEXT NOT NULL,
  protected_cwd TEXT NOT NULL,
  protected_profile_ref TEXT NOT NULL,
  registration_id TEXT NOT NULL UNIQUE,
  request_digest TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_contexts (
  context_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES workspace_projects(project_id),
  public_json TEXT NOT NULL,
  protected_cwd TEXT NOT NULL,
  protected_profile_ref TEXT NOT NULL,
  UNIQUE(project_id, context_id)
);
CREATE TABLE IF NOT EXISTS workspace_agent_scopes (
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  PRIMARY KEY(workspace_id, conversation_id),
  UNIQUE(project_id, context_id),
  FOREIGN KEY(project_id, context_id) REFERENCES workspace_contexts(project_id, context_id)
);
CREATE TABLE IF NOT EXISTS workspace_global_sequence (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  value INTEGER NOT NULL
);
INSERT OR IGNORE INTO workspace_global_sequence(singleton, value) VALUES(1, 0);
CREATE TABLE IF NOT EXISTS workspace_project_sequences (
  project_id TEXT PRIMARY KEY REFERENCES workspace_projects(project_id),
  value INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_conversation_sequences (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  value INTEGER NOT NULL,
  PRIMARY KEY(project_id, context_id, conversation_id)
);
CREATE TABLE IF NOT EXISTS workspace_chat (
  message_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  public_json TEXT NOT NULL,
  UNIQUE(project_id, context_id, conversation_id, sequence)
);
CREATE TABLE IF NOT EXISTS workspace_chat_dispatch (
  message_id TEXT PRIMARY KEY REFERENCES workspace_chat(message_id),
  session_id TEXT NOT NULL,
  process_epoch TEXT NOT NULL,
  native_turn_id TEXT,
  state TEXT NOT NULL,
  UNIQUE(session_id, process_epoch, native_turn_id)
);
CREATE TABLE IF NOT EXISTS workspace_chat_reply_sources (
  source_event_id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES workspace_chat(message_id)
);
CREATE TABLE IF NOT EXISTS workspace_project_execution (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  public_json TEXT NOT NULL,
  PRIMARY KEY(project_id, context_id)
);
CREATE TABLE IF NOT EXISTS workspace_questions (
  question_group_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  public_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_question_answers (
  question_group_id TEXT NOT NULL REFERENCES workspace_questions(question_group_id),
  revision INTEGER NOT NULL,
  answer_version_id TEXT NOT NULL UNIQUE,
  public_json TEXT NOT NULL,
  PRIMARY KEY(question_group_id, revision)
);
CREATE TABLE IF NOT EXISTS workspace_native_interactions (
  coordinator_session_id TEXT NOT NULL,
  process_epoch TEXT NOT NULL,
  request_id_type TEXT NOT NULL,
  request_id_value TEXT NOT NULL,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  question_group_id TEXT UNIQUE REFERENCES workspace_questions(question_group_id),
  native_approval_id TEXT UNIQUE,
  request_digest TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  public_json TEXT NOT NULL,
  PRIMARY KEY(coordinator_session_id, process_epoch, request_id_type, request_id_value)
);
CREATE TABLE IF NOT EXISTS workspace_agent_question_bindings (
  question_group_id TEXT PRIMARY KEY REFERENCES workspace_questions(question_group_id),
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  public_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_sessions (
  session_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  public_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_coordinator_claims (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  claim_id TEXT NOT NULL,
  principal_id TEXT NOT NULL,
  actor_key TEXT NOT NULL,
  client_request_id TEXT NOT NULL,
  profile_id TEXT NOT NULL,
  interaction_kind TEXT NOT NULL,
  state TEXT NOT NULL,
  session_id TEXT,
  actor_id TEXT,
  native_thread_id TEXT,
  process_epoch TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(project_id, context_id)
);
CREATE TABLE IF NOT EXISTS workspace_agents (
  actor_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  public_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_agent_relationships (
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  from_actor_id TEXT NOT NULL,
  to_actor_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  provenance TEXT NOT NULL,
  source_event_key TEXT NOT NULL,
  public_json TEXT NOT NULL,
  PRIMARY KEY(project_id, context_id, from_actor_id, to_actor_id, kind, provenance, source_event_key)
);
CREATE TABLE IF NOT EXISTS workspace_agent_output (
  actor_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  public_json TEXT NOT NULL,
  PRIMARY KEY(actor_id, sequence)
);
CREATE TABLE IF NOT EXISTS workspace_agent_output_sources (
  actor_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  source_event_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  PRIMARY KEY(actor_id, project_id, context_id, source_event_id)
);
CREATE TABLE IF NOT EXISTS workspace_history (
  global_sequence INTEGER PRIMARY KEY,
  project_id TEXT NOT NULL,
  context_id TEXT,
  context_key TEXT NOT NULL,
  actor_id TEXT,
  project_sequence INTEGER NOT NULL,
  source TEXT NOT NULL,
  source_event_id TEXT,
  public_json TEXT NOT NULL,
  UNIQUE(project_id, context_key, source, source_event_id)
);
CREATE TABLE IF NOT EXISTS workspace_idempotency (
  principal_id TEXT NOT NULL,
  actor_key TEXT NOT NULL,
  project_id TEXT NOT NULL,
  context_id TEXT NOT NULL,
  client_request_id TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  result_json TEXT NOT NULL,
  PRIMARY KEY(principal_id, actor_key, project_id, context_id, client_request_id)
);
CREATE INDEX IF NOT EXISTS workspace_history_project ON workspace_history(project_id, global_sequence);
CREATE INDEX IF NOT EXISTS workspace_history_actor ON workspace_history(project_id, context_id, actor_id, global_sequence);
CREATE INDEX IF NOT EXISTS workspace_chat_page ON workspace_chat(project_id, context_id, conversation_id, sequence);
CREATE INDEX IF NOT EXISTS workspace_native_interaction_scope
  ON workspace_native_interactions(project_id, context_id, state, updated_at);
`;

export class WorkspaceDatabase {
  readonly sqlite: DatabaseSync;

  constructor(databasePath: string) {
    if (databasePath !== ":memory:") mkdirSync(dirname(databasePath), { recursive: true });
    this.sqlite = new DatabaseSync(databasePath);
    this.sqlite.exec("PRAGMA busy_timeout = 5000;");
    if (databasePath !== ":memory:") {
      this.sqlite.exec("PRAGMA journal_mode = WAL;");
      this.sqlite.exec("PRAGMA synchronous = FULL;");
    }
    this.sqlite.exec(SCHEMA);
  }

  close(): void {
    this.sqlite.close();
  }

  transaction<T>(operation: () => T): T {
    this.sqlite.exec("BEGIN IMMEDIATE;");
    try {
      const result = operation();
      this.sqlite.exec("COMMIT;");
      return result;
    } catch (cause: unknown) {
      try {
        this.sqlite.exec("ROLLBACK;");
      } catch {
        // Preserve the original storage failure.
      }
      throw cause;
    }
  }

  run(sql: string, parameters: readonly SQLInputValue[] = []): void {
    this.sqlite.prepare(sql).run(...parameters);
  }

  get<T>(sql: string, schema: ZodType<T>, parameters: readonly SQLInputValue[] = []): T | null {
    const statement = this.sqlite.prepare(sql);
    statement.setReadBigInts(true);
    const row = statement.get(...parameters);
    return row === undefined ? null : schema.parse(row);
  }

  all<T>(sql: string, schema: ZodType<T>, parameters: readonly SQLInputValue[] = []): T[] {
    const statement = this.sqlite.prepare(sql);
    statement.setReadBigInts(true);
    return statement.all(...parameters).map((row) => schema.parse(row));
  }
}
