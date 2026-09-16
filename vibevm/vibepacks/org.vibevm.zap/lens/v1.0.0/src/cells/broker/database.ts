/**
 * SQLite persistence boundary for the lens broker.
 * @scope spec://org.vibevm.zap/lens/PROP-001#broker
 */
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";

import type { ZodType } from "zod";

const SCHEMA = `
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS principals (
  principal_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  workspace_ids_json TEXT NOT NULL,
  conversation_ids_json TEXT NOT NULL,
  capabilities_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS actors (
  actor_id TEXT PRIMARY KEY,
  principal_id TEXT NOT NULL REFERENCES principals(principal_id),
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  parent_actor_id TEXT REFERENCES actors(actor_id),
  current_generation INTEGER NOT NULL,
  state TEXT NOT NULL,
  capabilities_json TEXT NOT NULL,
  host_kind TEXT NOT NULL,
  host_session_id TEXT,
  host_subagent_id TEXT,
  host_provenance TEXT NOT NULL,
  reply_policy_json TEXT NOT NULL,
  resume_token_hash TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bindings (
  binding_id TEXT PRIMARY KEY,
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  principal_id TEXT NOT NULL REFERENCES principals(principal_id),
  generation INTEGER NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  active INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  revoked_at TEXT,
  UNIQUE(actor_id, generation)
);

CREATE TABLE IF NOT EXISTS conversation_sequences (
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  value INTEGER NOT NULL,
  PRIMARY KEY(workspace_id, conversation_id)
);

CREATE TABLE IF NOT EXISTS messages (
  message_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  from_actor_id TEXT REFERENCES actors(actor_id),
  to_actor_id TEXT REFERENCES actors(actor_id),
  kind TEXT NOT NULL,
  correlation_id TEXT,
  causation_id TEXT REFERENCES messages(message_id),
  sequence INTEGER NOT NULL,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(workspace_id, conversation_id, sequence)
);

CREATE TABLE IF NOT EXISTS questions (
  question_id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL UNIQUE REFERENCES messages(message_id),
  workspace_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  origin_actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  prompt TEXT NOT NULL,
  answer_mode TEXT NOT NULL,
  choices_json TEXT NOT NULL,
  independent_work_available INTEGER NOT NULL,
  reply_policy_json TEXT NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL,
  deadline_at TEXT,
  answer_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS question_answers (
  question_id TEXT NOT NULL REFERENCES questions(question_id),
  revision INTEGER NOT NULL,
  principal_id TEXT NOT NULL REFERENCES principals(principal_id),
  answer_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(question_id, revision)
);

CREATE TABLE IF NOT EXISTS deliveries (
  delivery_id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES messages(message_id),
  logical_recipient_actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  recipient_actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  forwarded_from_delivery_id TEXT REFERENCES deliveries(delivery_id),
  acknowledged_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(message_id, recipient_actor_id)
);

CREATE TABLE IF NOT EXISTS delivery_observations (
  delivery_id TEXT NOT NULL REFERENCES deliveries(delivery_id),
  observation TEXT NOT NULL,
  binding_generation INTEGER,
  attempt_id TEXT,
  host_receipt TEXT,
  created_at TEXT NOT NULL,
  PRIMARY KEY(delivery_id, observation, created_at)
);

CREATE TABLE IF NOT EXISTS idempotency (
  principal_id TEXT NOT NULL REFERENCES principals(principal_id),
  actor_key TEXT NOT NULL,
  client_request_id TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  result_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(principal_id, actor_key, client_request_id)
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation_sequence
  ON messages(workspace_id, conversation_id, sequence);
CREATE INDEX IF NOT EXISTS idx_deliveries_recipient
  ON deliveries(recipient_actor_id, acknowledged_at);
CREATE INDEX IF NOT EXISTS idx_questions_deadline
  ON questions(state, deadline_at);
`;

/** A narrow synchronous wrapper; transactions never cross an async boundary. */
export class BrokerDatabase {
  readonly sqlite: DatabaseSync;

  constructor(databasePath: string) {
    if (databasePath !== ":memory:") {
      mkdirSync(dirname(databasePath), { recursive: true });
    }
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
        // The original failure carries the useful diagnostic.
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
