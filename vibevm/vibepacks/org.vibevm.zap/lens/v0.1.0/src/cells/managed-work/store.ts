/** Durable managed-work launch and review journal. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { deserialize, serialize } from "node:v8";
import { z } from "zod";
import {
  ManagedWorkClaimSchema,
  ManagedWorkRequestSchema,
  type ManagedWorkClaim,
  type ManagedWorkRequest,
  type ManagedWorkResult,
} from "./contracts.ts";

const RowSchema = z.looseObject({
  request_digest: z.string(),
  value_blob: z.instanceof(Uint8Array),
});

export interface ManagedWorkStore {
  create(request: ManagedWorkRequest, claim: ManagedWorkClaim): ManagedWorkResult<ManagedWorkClaim>;
  load(runId: string): ManagedWorkResult<ManagedWorkClaim>;
  list(projectId: string, contextId: string): ManagedWorkResult<readonly ManagedWorkClaim[]>;
  transition(
    runId: string,
    expectedRevision: string,
    next: ManagedWorkClaim,
  ): ManagedWorkResult<ManagedWorkClaim>;
  close(): void;
}

export function openManagedWorkStore(databasePath: string): ManagedWorkResult<ManagedWorkStore> {
  try {
    if (databasePath !== ":memory:") mkdirSync(dirname(databasePath), { recursive: true });
    const database = new DatabaseSync(databasePath);
    database.exec(
      "PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS managed_work_claims(run_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, context_id TEXT NOT NULL, client_request_id TEXT NOT NULL, request_digest TEXT NOT NULL, revision TEXT NOT NULL, value_blob BLOB NOT NULL, UNIQUE(project_id,context_id,client_request_id)) STRICT",
    );
    const loadRow = (runId: string) =>
      database
        .prepare("SELECT request_digest,value_blob FROM managed_work_claims WHERE run_id=?")
        .get(runId);
    return {
      ok: true,
      value: {
        create(request, claim) {
          const parsedRequest = ManagedWorkRequestSchema.safeParse(request);
          const parsedClaim = ManagedWorkClaimSchema.safeParse(claim);
          if (!parsedRequest.success || !parsedClaim.success)
            return fail("invalid_input", "managed work claim is invalid");
          const digest = hash(parsedRequest.data);
          try {
            database
              .prepare(
                "INSERT INTO managed_work_claims(run_id,project_id,context_id,client_request_id,request_digest,revision,value_blob) VALUES(?,?,?,?,?,?,?)",
              )
              .run(
                parsedClaim.data.runId,
                parsedRequest.data.projectId,
                parsedRequest.data.contextId,
                parsedRequest.data.clientRequestId,
                digest,
                parsedClaim.data.revision,
                serialize(parsedClaim.data),
              );
            return { ok: true, value: parsedClaim.data };
          } catch {
            const row = database
              .prepare(
                "SELECT request_digest,value_blob FROM managed_work_claims WHERE project_id=? AND context_id=? AND client_request_id=?",
              )
              .get(
                parsedRequest.data.projectId,
                parsedRequest.data.contextId,
                parsedRequest.data.clientRequestId,
              );
            return decode(row, digest, "client request identity changed content");
          }
        },
        load(runId) {
          return decode(loadRow(runId));
        },
        list(projectId, contextId) {
          try {
            const rows = database
              .prepare(
                "SELECT request_digest,value_blob FROM managed_work_claims WHERE project_id=? AND context_id=? ORDER BY rowid",
              )
              .all(projectId, contextId);
            const claims: ManagedWorkClaim[] = [];
            for (const row of rows) {
              const decoded = decode(row);
              if (!decoded.ok) return decoded;
              claims.push(decoded.value);
            }
            return { ok: true, value: claims };
          } catch {
            return fail("unavailable", "managed work claims cannot be listed");
          }
        },
        transition(runId, expectedRevision, next) {
          const parsed = ManagedWorkClaimSchema.safeParse(next);
          if (
            !parsed.success ||
            parsed.data.runId !== runId ||
            BigInt(parsed.data.revision) !== BigInt(expectedRevision) + 1n
          )
            return fail("invalid_input", "managed work transition is invalid");
          const changed = database
            .prepare(
              "UPDATE managed_work_claims SET revision=?,value_blob=? WHERE run_id=? AND revision=?",
            )
            .run(parsed.data.revision, serialize(parsed.data), runId, expectedRevision);
          return changed.changes === 1
            ? { ok: true, value: parsed.data }
            : fail("conflict", "managed work revision changed");
        },
        close() {
          database.close();
        },
      },
    };
  } catch {
    return fail("unavailable", "managed work store could not be opened");
  }
}

function decode(
  raw: unknown,
  digest?: string,
  conflict = "managed work identity conflict",
): ManagedWorkResult<ManagedWorkClaim> {
  const row = RowSchema.safeParse(raw);
  if (!row.success) return fail("unavailable", "managed work claim is unavailable");
  if (digest !== undefined && row.data.request_digest !== digest) return fail("conflict", conflict);
  try {
    const claim = ManagedWorkClaimSchema.safeParse(deserialize(row.data.value_blob));
    return claim.success
      ? { ok: true, value: claim.data }
      : fail("unavailable", "managed work claim is malformed");
  } catch {
    return fail("unavailable", "managed work claim cannot be decoded");
  }
}
function hash(value: unknown) {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}
function fail(
  code: "invalid_input" | "conflict" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
