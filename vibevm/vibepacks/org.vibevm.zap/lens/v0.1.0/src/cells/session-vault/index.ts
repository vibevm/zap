/**
 * Durable trusted adapter-session credentials stored beside the broker.
 * @scope spec://org.vibevm.zap/lens/PROP-001#identity
 */
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import {
  ActorStateSchema,
  ConnectionSchema,
  CredentialSchema,
  DecimalSchema,
  HostBindingSchema,
  ReplyPolicySchema,
  type ActorId,
  type Result,
} from "../protocol/index.ts";
import {
  AdapterSessionIdSchema,
  failure,
  type AdapterSessionId,
  type AdapterSessionVault,
  type RetainedSession,
} from "../transport/index.ts";

const StoredSchema = z
  .object({
    principalToken: CredentialSchema,
    connection: ConnectionSchema,
    host: HostBindingSchema,
    replyPolicy: ReplyPolicySchema,
  })
  .strict();
const RowSchema = z.looseObject({
  adapter_session_id: AdapterSessionIdSchema,
  value_json: z.string(),
});

export interface SqliteAdapterSessionVault extends AdapterSessionVault {
  close(): void;
}

/** @implements spec://org.vibevm.zap/lens/PROP-001#identity */
export function openSqliteAdapterSessionVault(
  databasePath: string,
): Result<SqliteAdapterSessionVault> {
  try {
    const database = new DatabaseSync(databasePath);
    database.exec(`
      CREATE TABLE IF NOT EXISTS lens_adapter_sessions (
        adapter_session_id TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
      ) STRICT
      ;
      CREATE TABLE IF NOT EXISTS lens_hook_cursors (
        actor_id TEXT PRIMARY KEY,
        after_sequence TEXT NOT NULL
      ) STRICT
    `);
    const get = database.prepare(
      "SELECT adapter_session_id, value_json FROM lens_adapter_sessions WHERE adapter_session_id = ?",
    );
    const actorState = database.prepare("SELECT state FROM actors WHERE actor_id = ?");
    const list = database.prepare(
      "SELECT adapter_session_id, value_json FROM lens_adapter_sessions ORDER BY adapter_session_id",
    );
    const put = database.prepare(`
      INSERT INTO lens_adapter_sessions(adapter_session_id, value_json) VALUES (?, ?)
      ON CONFLICT(adapter_session_id) DO UPDATE SET value_json = excluded.value_json
    `);
    const remove = database.prepare(
      "DELETE FROM lens_adapter_sessions WHERE adapter_session_id = ?",
    );
    const getCursor = database.prepare(
      "SELECT after_sequence FROM lens_hook_cursors WHERE actor_id = ?",
    );
    const putCursor = database.prepare(`
      INSERT INTO lens_hook_cursors(actor_id, after_sequence) VALUES (?, ?)
      ON CONFLICT(actor_id) DO UPDATE SET after_sequence = excluded.after_sequence
    `);
    return {
      ok: true,
      value: {
        actorState: (actorId: ActorId) => {
          const row = z.looseObject({ state: ActorStateSchema }).safeParse(actorState.get(actorId));
          return row.success
            ? { ok: true, value: row.data.state }
            : vaultFailure("authoritative adapter actor state is unavailable");
        },
        get: (id) => decodeRow(get.get(id)),
        list: () => {
          const entries: [AdapterSessionId, RetainedSession][] = [];
          for (const raw of list.all()) {
            const row = RowSchema.safeParse(raw);
            if (!row.success) return vaultFailure("stored adapter-session row is malformed");
            const decoded = decodeValue(row.data.value_json);
            if (!decoded.ok) return decoded;
            entries.push([row.data.adapter_session_id, decoded.value]);
          }
          return { ok: true, value: entries };
        },
        put: (id, session) => {
          put.run(id, JSON.stringify(session));
          return { ok: true, value: null };
        },
        remove: (id) => {
          remove.run(id);
          return { ok: true, value: null };
        },
        offerCursor: (actorId) => {
          const row = z
            .looseObject({ after_sequence: DecimalSchema })
            .safeParse(getCursor.get(actorId));
          return {
            ok: true,
            value: row.success ? row.data.after_sequence : DecimalSchema.parse("0"),
          };
        },
        setOfferCursor: (actorId, cursor) => {
          putCursor.run(actorId, cursor);
          return { ok: true, value: null };
        },
        close: () => {
          database.close();
        },
      },
    };
  } catch {
    return vaultFailure("adapter-session vault could not open");
  }
}

function decodeRow(raw: unknown): Result<RetainedSession> {
  const row = RowSchema.safeParse(raw);
  return row.success
    ? decodeValue(row.data.value_json)
    : vaultFailure("adapter session is missing or malformed");
}

function decodeValue(raw: string): Result<RetainedSession> {
  try {
    const value: unknown = JSON.parse(raw);
    const parsed = StoredSchema.safeParse(value);
    return parsed.success
      ? { ok: true, value: parsed.data }
      : vaultFailure("stored adapter-session credentials are malformed");
  } catch {
    return vaultFailure("stored adapter-session JSON is malformed");
  }
}

function vaultFailure(why: string): Result<never> {
  return failure("storage_failure", why, "repair the protected user-local adapter-session vault");
}
