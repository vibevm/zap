/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { DatabaseSync } from "node:sqlite";
import { deserialize, serialize } from "node:v8";
import { z } from "zod";
import type { QuicklensResult } from "../quicklens-model/index.ts";
import { ConversationIdSchema, WorkspaceIdSchema, type Result } from "../protocol/index.ts";
import { WorkflowRecordSchema, type PlanWorkflowStore, type WorkflowRecord } from "./workflow.ts";

export function openSqlitePlanWorkflowStore(
  path: string,
  scope: { readonly workspaceId: string; readonly conversationId: string },
): QuicklensResult<PlanWorkflowStore & { close(): void }> {
  try {
    const workspaceId = WorkspaceIdSchema.parse(scope.workspaceId);
    const conversationId = ConversationIdSchema.parse(scope.conversationId);
    const database = new DatabaseSync(path);
    database.exec("PRAGMA busy_timeout = 5000");
    database.exec("PRAGMA journal_mode = WAL");
    database.exec(
      "CREATE TABLE IF NOT EXISTS quicklens_plan_workflow (operation_ref TEXT PRIMARY KEY, intent_ref TEXT UNIQUE NOT NULL, workspace_id TEXT NOT NULL, conversation_id TEXT NOT NULL, phase TEXT NOT NULL, value_blob BLOB NOT NULL) STRICT",
    );
    const columns = z
      .array(z.looseObject({ name: z.string() }))
      .parse(database.prepare("PRAGMA table_info(quicklens_plan_workflow)").all());
    if (!columns.some((column) => column.name === "phase")) {
      database.exec("ALTER TABLE quicklens_plan_workflow ADD COLUMN phase TEXT");
    }
    if (!columns.some((column) => column.name === "workspace_id")) {
      database.exec("ALTER TABLE quicklens_plan_workflow ADD COLUMN workspace_id TEXT");
    }
    if (!columns.some((column) => column.name === "conversation_id")) {
      database.exec("ALTER TABLE quicklens_plan_workflow ADD COLUMN conversation_id TEXT");
    }
    const byOperation = database.prepare(
      "SELECT value_blob FROM quicklens_plan_workflow WHERE operation_ref = ? AND workspace_id = ? AND conversation_id = ?",
    );
    const byIntent = database.prepare(
      "SELECT value_blob FROM quicklens_plan_workflow WHERE intent_ref = ? AND workspace_id = ? AND conversation_id = ?",
    );
    const insert = database.prepare(
      "INSERT INTO quicklens_plan_workflow(operation_ref,intent_ref,workspace_id,conversation_id,phase,value_blob) VALUES(?,?,?,?,?,?)",
    );
    const update = database.prepare(
      "UPDATE quicklens_plan_workflow SET phase = ?, value_blob = ? WHERE operation_ref = ? AND workspace_id = ? AND conversation_id = ?",
    );
    const held = database.prepare(
      "SELECT value_blob FROM quicklens_plan_workflow WHERE phase = 'owner_decision_required' AND workspace_id = ? AND conversation_id = ? ORDER BY rowid LIMIT 2",
    );
    const decode = (row: unknown): Result<WorkflowRecord | null> => {
      if (row === undefined) return { ok: true, value: null };
      const parsed = z.looseObject({ value_blob: z.instanceof(Uint8Array) }).safeParse(row);
      if (!parsed.success) return storageFailure();
      try {
        const value = WorkflowRecordSchema.safeParse(deserialize(parsed.data.value_blob));
        return value.success ? { ok: true, value: value.data } : storageFailure();
      } catch {
        return storageFailure();
      }
    };
    const transaction = <T>(body: () => Result<T>): Result<T> => {
      database.exec("BEGIN IMMEDIATE");
      try {
        const result = body();
        if (result.ok) database.exec("COMMIT");
        else database.exec("ROLLBACK");
        return result;
      } catch {
        database.exec("ROLLBACK");
        return storageFailure();
      }
    };
    const getByOperation = (value: string) =>
      decode(byOperation.get(value, workspaceId, conversationId));
    const getByIntent = (value: string) => decode(byIntent.get(value, workspaceId, conversationId));
    return {
      ok: true,
      value: {
        getByIntent,
        getByOperation,
        create: (record) =>
          transaction<WorkflowRecord>(() => {
            if (record.workspaceId !== workspaceId || record.conversationId !== conversationId) {
              return storageConflict("plan record is outside this workflow scope");
            }
            const byOperationResult = getByOperation(record.proposal.operationRef);
            const byIntentResult = getByIntent(record.proposal.intentRef);
            if (!byOperationResult.ok || !byIntentResult.ok) return storageFailure();
            const current = byOperationResult.value ?? byIntentResult.value;
            if (current !== null) {
              return current.proposalDigest === record.proposalDigest
                ? { ok: true, value: current }
                : storageConflict("plan identity is already bound to different bytes");
            }
            insert.run(
              String(record.proposal.operationRef),
              String(record.proposal.intentRef),
              workspaceId,
              conversationId,
              record.phase,
              serialize(record),
            );
            return { ok: true, value: record };
          }),
        transition: (operationRef, digest, expected, next) =>
          transaction<WorkflowRecord>(() => {
            const current = getByOperation(operationRef);
            if (!current.ok || current.value === null) return storageFailure();
            if (
              current.value.proposalDigest !== digest ||
              next.proposalDigest !== digest ||
              next.workspaceId !== workspaceId ||
              next.conversationId !== conversationId ||
              !expected.includes(current.value.phase)
            ) {
              return storageConflict("plan transition lost its immutable compare-and-set");
            }
            update.run(next.phase, serialize(next), operationRef, workspaceId, conversationId);
            return { ok: true, value: next };
          }),
        currentDecision: () => {
          const rows = held.all(workspaceId, conversationId);
          if (rows.length > 1)
            return storageConflict("multiple held Owner decisions are ambiguous");
          return decode(rows[0]);
        },
        close: () => {
          database.close();
        },
      },
    };
  } catch {
    return qerror("Plan workflow store could not be opened");
  }
}

function storageFailure(): Result<never> {
  return {
    ok: false,
    error: {
      code: "storage_failure",
      message:
        "violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: plan workflow row is unavailable; fix surface: repair the protected workflow store",
    },
  };
}

function storageConflict(message: string): Result<never> {
  return {
    ok: false,
    error: {
      code: "idempotency_conflict",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: ${message}; fix surface: use new plan identities for changed bytes`,
    },
  };
}

function qerror(message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code: "unavailable",
      message,
      recovery: "Repair the protected plan workflow store and retry.",
    },
  };
}
