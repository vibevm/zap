/** @scope spec://org.vibevm.zap/lens/PROP-006#terminal-evidence */
/** Durable managed-terminal output store owned by the trusted Wayfinder runtime. */
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import {
  TerminalLifecycleEventSchema,
  TerminalOutputSchema,
  type TerminalLifecycleEvent,
  type TerminalOutput,
} from "../managed-terminal/index.ts";

export interface ManagedTerminalOutputPage {
  readonly events: readonly TerminalOutput[];
  readonly nextSequence: number | null;
  readonly gap: { readonly firstAvailableSequence: number; readonly reason: string } | null;
}

export interface ManagedTerminalOutputStore {
  append(event: TerminalOutput): void;
  appendLifecycle(event: TerminalLifecycleEvent): void;
  page(terminalId: string, afterSequence: number, limit: number): ManagedTerminalOutputPage;
  lifecycle(terminalId: string): readonly TerminalLifecycleEvent[];
  latestLifecycle(projectId: string, contextId: string): readonly TerminalLifecycleEvent[];
  close(): void;
}

export function openManagedTerminalOutputStore(
  databasePath: string,
  historyLimit = 2_000,
): ManagedTerminalOutputStore {
  if (!Number.isInteger(historyLimit) || historyLimit < 1 || historyLimit > 100_000)
    throw new RangeError(
      "violates REQ spec://org.vibevm.zap/lens/PROP-006#terminal-evidence: managed terminal output history limit is invalid; fix surface: configure a limit from 1 through 100000",
    );
  if (databasePath !== ":memory:") mkdirSync(dirname(databasePath), { recursive: true });
  const database = new DatabaseSync(databasePath);
  database.exec(
    "PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL; CREATE TABLE IF NOT EXISTS managed_terminal_output (terminal_id TEXT NOT NULL, sequence INTEGER NOT NULL, public_json TEXT NOT NULL, PRIMARY KEY(terminal_id, sequence)); CREATE TABLE IF NOT EXISTS managed_terminal_lifecycle (event_id INTEGER PRIMARY KEY AUTOINCREMENT, terminal_id TEXT NOT NULL, public_json TEXT NOT NULL); CREATE INDEX IF NOT EXISTS managed_terminal_lifecycle_terminal ON managed_terminal_lifecycle(terminal_id, event_id);",
  );
  return {
    append(event) {
      database
        .prepare(
          "INSERT OR IGNORE INTO managed_terminal_output(terminal_id, sequence, public_json) VALUES(?, ?, ?)",
        )
        .run(event.terminalId, event.sequence, JSON.stringify(event));
      database
        .prepare(
          "DELETE FROM managed_terminal_output WHERE terminal_id = ? AND sequence <= (SELECT COALESCE(MAX(sequence), 0) - ? FROM managed_terminal_output WHERE terminal_id = ?)",
        )
        .run(event.terminalId, historyLimit, event.terminalId);
    },
    appendLifecycle(event) {
      database
        .prepare("INSERT INTO managed_terminal_lifecycle(terminal_id, public_json) VALUES(?, ?)")
        .run(event.terminalId, JSON.stringify(event));
    },
    page(terminalId, afterSequence, limit) {
      const firstRow = z
        .object({ value: z.union([z.number(), z.bigint()]).nullable().optional() })
        .optional()
        .parse(
          database
            .prepare(
              "SELECT MIN(sequence) AS value FROM managed_terminal_output WHERE terminal_id = ?",
            )
            .get(terminalId),
        );
      const first =
        firstRow?.value === null || firstRow?.value === undefined ? null : Number(firstRow.value);
      const rows = z
        .array(z.object({ public_json: z.string() }).strict())
        .parse(
          database
            .prepare(
              "SELECT public_json FROM managed_terminal_output WHERE terminal_id = ? AND sequence > ? ORDER BY sequence LIMIT ?",
            )
            .all(terminalId, afterSequence, limit),
        );
      const events = rows.map((row) => TerminalOutputSchema.parse(parseJson(row.public_json)));
      return {
        events,
        nextSequence: events.at(-1)?.sequence ?? null,
        gap:
          first !== null && first > afterSequence + 1
            ? { firstAvailableSequence: first, reason: "terminal output history was bounded" }
            : null,
      };
    },
    lifecycle(terminalId) {
      const rows = z
        .array(z.object({ public_json: z.string() }).strict())
        .parse(
          database
            .prepare(
              "SELECT public_json FROM managed_terminal_lifecycle WHERE terminal_id = ? ORDER BY event_id",
            )
            .all(terminalId),
        );
      return rows.map((row) => TerminalLifecycleEventSchema.parse(parseJson(row.public_json)));
    },
    latestLifecycle(projectId, contextId) {
      const rows = z
        .array(z.object({ public_json: z.string() }).strict())
        .parse(
          database
            .prepare(
              "SELECT public_json FROM managed_terminal_lifecycle WHERE event_id IN (SELECT MAX(event_id) FROM managed_terminal_lifecycle GROUP BY terminal_id) AND json_extract(public_json, '$.projectId') = ? AND json_extract(public_json, '$.contextId') = ? ORDER BY terminal_id LIMIT 512",
            )
            .all(projectId, contextId),
        );
      return rows.map((row) => TerminalLifecycleEventSchema.parse(parseJson(row.public_json)));
    },
    close() {
      database.close();
    },
  };
}

function parseJson(text: string): unknown {
  return JSON.parse(text);
}
