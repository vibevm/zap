/** Durable annotation and Trash store. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import {
  AnnotationCommandRequestSchemas,
  AnnotationCommandResponseSchemas,
  AnnotationDeliverySchema,
  AnnotationNoteSchema,
  AnnotationNoteVersionSchema,
  AnnotationReadRequestSchemas,
  AnnotationTrashEntrySchema,
  type AnnotationCommandRequest,
  type AnnotationCommandResponse,
  type AnnotationNote,
  type AnnotationNoteVersion,
  type AnnotationTrashEntry,
} from "../workspace-model/index.ts";
import {
  projectObjectReferenceKey,
  type ProjectObjectReference,
} from "../workspace-model/index.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";
import type { AnnotationErrorCode, AnnotationResult, AnnotationStore } from "./types.ts";
const ReadSchema = z.discriminatedUnion("operation", [...AnnotationReadRequestSchemas]);
const CommandSchema = z.discriminatedUnion("operation", [...AnnotationCommandRequestSchemas]);
const JsonRowSchema = z.object({ public_json: z.string() }).strict();
const RequestRowSchema = z.object({ digest: z.string(), response_json: z.string() }).strict();
export function openAnnotationStore(options: {
  readonly databasePath: string;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}): AnnotationResult<AnnotationStore> {
  try {
    if (options.databasePath !== ":memory:")
      mkdirSync(dirname(options.databasePath), { recursive: true });
    const database = new DatabaseSync(options.databasePath);
    database.exec(
      "PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS annotation_notes(note_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, context_id TEXT NOT NULL, state TEXT NOT NULL, revision TEXT NOT NULL, public_json TEXT NOT NULL) STRICT; CREATE TABLE IF NOT EXISTS annotation_versions(note_id TEXT NOT NULL, version TEXT NOT NULL, public_json TEXT NOT NULL, PRIMARY KEY(note_id,version)) STRICT; CREATE TABLE IF NOT EXISTS annotation_trash(trash_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, context_id TEXT NOT NULL, state TEXT NOT NULL, public_json TEXT NOT NULL) STRICT; CREATE TABLE IF NOT EXISTS annotation_requests(project_id TEXT NOT NULL, context_id TEXT NOT NULL, client_request_id TEXT NOT NULL, digest TEXT NOT NULL, response_json TEXT NOT NULL, PRIMARY KEY(project_id,context_id,client_request_id)) STRICT; CREATE TABLE IF NOT EXISTS annotation_deliveries(delivery_id TEXT PRIMARY KEY, attempt_id TEXT NOT NULL, note_id TEXT NOT NULL, note_version TEXT NOT NULL, state TEXT NOT NULL, public_json TEXT NOT NULL, UNIQUE(attempt_id,note_id,note_version)) STRICT",
    );
    const clock = options.clock ?? (() => new Date());
    const idFactory = options.idFactory ?? ((kind) => `${kind}.${crypto.randomUUID()}`);
    let closed = false;
    return {
      ok: true,
      value: {
        read(access, rawRequest) {
          if (closed) return fail("closed", "annotation store is closed");
          const request = ReadSchema.safeParse(rawRequest);
          if (!request.success) return fail("invalid_input", "annotation read request is invalid");
          if (!authorized(access, request.data.projectId))
            return fail("forbidden", "annotation project is outside scope");
          if (request.data.operation === "annotation.note.list.v1") {
            const includeArchived = request.data.includeArchived;
            const rows = database
              .prepare(
                "SELECT public_json FROM annotation_notes WHERE project_id=? AND context_id=? ORDER BY rowid",
              )
              .all(request.data.projectId, request.data.contextId);
            const notes = rows
              .map((row) => parseRow(row, AnnotationNoteSchema))
              .filter(
                (note): note is AnnotationNote =>
                  note !== null && (includeArchived || note.state === "active"),
              );
            return { ok: true, value: { operation: request.data.operation, notes } };
          }
          if (request.data.operation === "annotation.note.get.v1") {
            const note = loadNote(database, request.data.noteId);
            if (
              note === null ||
              note.projectId !== request.data.projectId ||
              note.contextId !== request.data.contextId
            )
              return fail("not_found", "annotation note was not found");
            const rows = database
              .prepare(
                "SELECT public_json FROM annotation_versions WHERE note_id=? ORDER BY CAST(version AS INTEGER)",
              )
              .all(note.noteId);
            const versions = rows
              .map((row) => parseRow(row, AnnotationNoteVersionSchema))
              .filter((version): version is AnnotationNoteVersion => version !== null);
            return { ok: true, value: { operation: request.data.operation, note, versions } };
          }
          const rows = database
            .prepare(
              "SELECT public_json FROM annotation_trash WHERE project_id=? AND context_id=? ORDER BY rowid DESC",
            )
            .all(request.data.projectId, request.data.contextId);
          const allEntries = rows
            .map((row) => parseRow(row, AnnotationTrashEntrySchema))
            .filter((entry): entry is AnnotationTrashEntry => entry !== null);
          const afterTrashId = request.data.afterTrashId;
          const limit = request.data.limit;
          const start =
            afterTrashId === null
              ? 0
              : Math.max(0, allEntries.findIndex((entry) => entry.trashId === afterTrashId) + 1);
          const entries = allEntries.slice(start, start + limit);
          const nextTrashId =
            start + limit < allEntries.length ? (entries.at(-1)?.trashId ?? null) : null;
          return {
            ok: true,
            value: {
              operation: request.data.operation,
              entries,
              nextTrashId,
              total: allEntries.length,
            },
          };
        },
        command(access, rawRequest) {
          if (closed) return fail("closed", "annotation store is closed");
          const request = CommandSchema.safeParse(rawRequest);
          if (!request.success) return fail("invalid_input", "annotation command is invalid");
          if (!authorized(access, request.data.projectId))
            return fail("forbidden", "annotation project is outside scope");
          const digest = hash(request.data);
          const prior = RequestRowSchema.safeParse(
            database
              .prepare(
                "SELECT digest,response_json FROM annotation_requests WHERE project_id=? AND context_id=? AND client_request_id=?",
              )
              .get(request.data.projectId, request.data.contextId, request.data.clientRequestId),
          );
          if (prior.success) {
            if (prior.data.digest !== digest)
              return fail("idempotency_conflict", "annotation request identity changed content");
            return decodeResponse(prior.data.response_json);
          }
          const result = executeCommand(database, access, request.data, clock, idFactory);
          if (!result.ok) return result;
          database
            .prepare(
              "INSERT INTO annotation_requests(project_id,context_id,client_request_id,digest,response_json) VALUES(?,?,?,?,?)",
            )
            .run(
              request.data.projectId,
              request.data.contextId,
              request.data.clientRequestId,
              digest,
              JSON.stringify(result.value),
            );
          return result;
        },
        observeSource(access, observation) {
          if (closed) return fail("closed", "annotation store is closed");
          if (!authorized(access, observation.projectId))
            return fail("forbidden", "annotation project is outside scope");
          if (
            observation.presentTargets.some(
              (target) =>
                target.projectId !== observation.projectId ||
                target.contextId !== observation.contextId,
            ) ||
            observation.removedTargets.some(
              (target) =>
                target.projectId !== observation.projectId ||
                target.contextId !== observation.contextId,
            ) ||
            observation.snapshots.some(
              ({ target }) =>
                target.projectId !== observation.projectId ||
                target.contextId !== observation.contextId,
            )
          )
            return fail("forbidden", "source observation target is outside scope");
          if (observation.state !== "authoritative_full" && observation.state !== "explicit_remove")
            return { ok: true, value: [] };
          const removed = new Map(
            observation.removedTargets.map((target) => [projectObjectReferenceKey(target), target]),
          );
          const rows = database
            .prepare(
              "SELECT public_json FROM annotation_notes WHERE project_id=? AND context_id=? AND state='active'",
            )
            .all(observation.projectId, observation.contextId);
          const trashed: AnnotationTrashEntry[] = [];
          if (observation.state === "authoritative_full") {
            const present = new Set(
              observation.presentTargets.map((target) => projectObjectReferenceKey(target)),
            );
            const activeRows = database
              .prepare(
                "SELECT public_json FROM annotation_notes WHERE project_id=? AND context_id=? AND state='active'",
              )
              .all(observation.projectId, observation.contextId);
            for (const row of activeRows) {
              const note = parseRow(row, AnnotationNoteSchema);
              if (
                note !== null &&
                observation.coveredDomains.includes(note.target.domain) &&
                !present.has(projectObjectReferenceKey(note.target))
              ) {
                removed.set(projectObjectReferenceKey(note.target), note.target);
              }
            }
          }
          for (const row of rows) {
            const note = parseRow(row, AnnotationNoteSchema);
            if (note === null || !removed.has(projectObjectReferenceKey(note.target))) continue;
            const entry = archiveNote(
              database,
              note,
              access,
              "Authoritative source removal",
              clock,
              idFactory,
            );
            trashed.push(entry);
          }
          for (const target of observation.removedTargets) {
            const key = projectObjectReferenceKey(target);
            const already = trashed.some((entry) => entry.formerIdentity === key);
            if (already) continue;
            const snapshot =
              observation.snapshots.find((item) => projectObjectReferenceKey(item.target) === key)
                ?.snapshot ?? null;
            trashed.push(
              insertObjectTrash(
                database,
                target,
                snapshot,
                observation.basisRef,
                access,
                clock,
                idFactory,
              ),
            );
          }
          return { ok: true, value: trashed };
        },
        readTrash(access, trashId) {
          const entry = loadTrash(database, trashId);
          return entry === null
            ? fail("not_found", "annotation Trash entry was not found")
            : authorized(access, entry.projectId)
              ? { ok: true, value: entry }
              : fail("forbidden", "annotation Trash entry is outside scope");
        },
        listDeferred(access, projectId, contextId, targets) {
          if (closed) return fail("closed", "annotation store is closed");
          if (!authorized(access, projectId))
            return fail("forbidden", "annotation project is outside scope");
          const keys = new Set(targets.map((target) => projectObjectReferenceKey(target)));
          const rows = database
            .prepare(
              "SELECT public_json FROM annotation_notes WHERE project_id=? AND context_id=? AND state='active'",
            )
            .all(projectId, contextId);
          const notes = rows
            .map((row) => parseRow(row, AnnotationNoteSchema))
            .filter(
              (note): note is AnnotationNote =>
                note !== null &&
                note.kind === "deferred" &&
                keys.has(projectObjectReferenceKey(note.target)),
            );
          return { ok: true, value: notes };
        },
        offerDelivery(input) {
          if (closed) return fail("closed", "annotation store is closed");
          const prior = database
            .prepare(
              "SELECT public_json FROM annotation_deliveries WHERE attempt_id=? AND note_id=? AND note_version=?",
            )
            .get(input.attemptId, input.noteId, input.noteVersion);
          if (prior !== undefined) {
            const loaded = parseRow(prior, AnnotationDeliverySchema);
            return loaded === null
              ? fail("storage_failure", "annotation delivery is malformed")
              : { ok: true, value: loaded };
          }
          const delivery = AnnotationDeliverySchema.parse({
            ...input,
            state: "offered",
            acknowledgedAt: null,
            resolvedAt: null,
            messageId: null,
          });
          database
            .prepare(
              "INSERT INTO annotation_deliveries(delivery_id,attempt_id,note_id,note_version,state,public_json) VALUES(?,?,?,?,?,?)",
            )
            .run(
              delivery.deliveryId,
              delivery.attemptId,
              delivery.noteId,
              delivery.noteVersion,
              delivery.state,
              JSON.stringify(delivery),
            );
          return { ok: true, value: delivery };
        },
        acknowledgeDelivery(input) {
          if (closed) return fail("closed", "annotation store is closed");
          const row = database
            .prepare("SELECT public_json FROM annotation_deliveries WHERE delivery_id=?")
            .get(input.deliveryId);
          const current = parseRow(row, AnnotationDeliverySchema);
          if (current === null) return fail("not_found", "annotation delivery was not found");
          if (current.attemptId !== input.attemptId || current.noteVersion !== input.version)
            return fail("conflict", "delivery identity does not match the work attempt");
          if (input.access.actorId === null || input.access.actorId !== current.recipientActorId)
            return fail("forbidden", "only the addressed work actor can acknowledge delivery");
          if (!authorized(input.access, current.projectId))
            return fail("forbidden", "delivery project is outside scope");
          if (current.state === "suppressed" || current.state === "resolved")
            return { ok: true, value: current };
          const updated = AnnotationDeliverySchema.parse({
            ...current,
            state: "acknowledged",
            acknowledgedAt: input.acknowledgedAt,
            messageId: input.messageId,
          });
          database
            .prepare("UPDATE annotation_deliveries SET state=?,public_json=? WHERE delivery_id=?")
            .run(updated.state, JSON.stringify(updated), updated.deliveryId);
          return { ok: true, value: updated };
        },
        close() {
          if (!closed) {
            closed = true;
            database.close();
          }
        },
      },
    };
  } catch {
    return fail("storage_failure", "annotation store could not be opened");
  }
}
// prettier-ignore
function executeCommand(
  database: DatabaseSync,
  access: WorkspaceAccessContext,
  request: AnnotationCommandRequest,
  clock: () => Date,
  idFactory: (kind: string) => string,
): AnnotationResult<AnnotationCommandResponse> {
  const now = clock().toISOString();
  const author = { principalId: access.principalId, actorId: access.actorId, clientId: access.clientId };
  if (request.operation === "annotation.note.create.v1") {
    if (request.target.projectId !== request.projectId || request.target.contextId !== request.contextId) return fail("forbidden", "annotation target is outside command scope");
    const noteId = idFactory("annotation-note");
    const version = AnnotationNoteVersionSchema.parse({ noteId, version: "1", kind: request.kind, title: request.title, bodyMarkdown: request.bodyMarkdown, target: request.target, sourceBasisRef: request.sourceBasisRef, targetSnapshot: request.targetSnapshot, author, createdAt: now });
    const note = AnnotationNoteSchema.parse({ noteId, projectId: request.projectId, contextId: request.contextId, target: request.target, kind: request.kind, state: "active", currentVersion: "1", title: request.title, bodyMarkdown: request.bodyMarkdown, sourceBasisRef: request.sourceBasisRef, targetSnapshot: request.targetSnapshot, author, createdAt: now, updatedAt: now, archivedAt: null, archivedReason: null });
    insertNote(database, note, version);
    return { ok: true, value: { operation: request.operation, note } };
  }
  if (request.operation === "annotation.note.restore.v1")
    return restoreNote(database, request, access, clock);
  if (request.operation === "annotation.note.send.v1" || request.operation === "annotation.object.restore.intent.v1") return fail("unavailable", "this annotation operation requires the service delivery port");
  if (!("noteId" in request))
    return fail("invalid_input", "annotation command note identity is missing");
  const note = loadNote(database, request.noteId);
  if (note === null || note.projectId !== request.projectId || note.contextId !== request.contextId) return fail("not_found", "annotation note was not found");
  if (note.currentVersion !== request.expectedRevision) return fail("stale_revision", "annotation note revision changed");
  if (
    request.operation === "annotation.note.update.v1" ||
    request.operation === "annotation.note.relink.v1"
  ) {
    const target = request.operation === "annotation.note.relink.v1" ? request.target : note.target;
    if (target.projectId !== request.projectId || target.contextId !== request.contextId)
      return fail("forbidden", "annotation target is outside command scope");
    const version = nextRevision(note.currentVersion);
    const updated = AnnotationNoteSchema.parse({ ...note, target, state: "active", currentVersion: version, title: request.operation === "annotation.note.update.v1" ? request.title : note.title, bodyMarkdown: request.operation === "annotation.note.update.v1" ? request.bodyMarkdown : note.bodyMarkdown, sourceBasisRef: request.sourceBasisRef, targetSnapshot: request.targetSnapshot, updatedAt: now, archivedAt: null, archivedReason: null });
    const versionRow = AnnotationNoteVersionSchema.parse({ noteId: note.noteId, version, kind: note.kind, title: updated.title, bodyMarkdown: updated.bodyMarkdown, target, sourceBasisRef: updated.sourceBasisRef, targetSnapshot: updated.targetSnapshot, author, createdAt: now });
    updateNote(database, updated, versionRow);
    return { ok: true, value: { operation: request.operation, note: updated } };
  }
  const archived = archiveNote(database, note, access, request.reasonMarkdown, clock, idFactory);
  return {
    ok: true,
    value: {
      operation: request.operation,
      note: loadNote(database, note.noteId) ?? note,
      trash: archived,
    },
  };
}
// prettier-ignore
function restoreNote(database: DatabaseSync, request: Extract<AnnotationCommandRequest, { operation: "annotation.note.restore.v1" }>, access: WorkspaceAccessContext, clock: () => Date): AnnotationResult<AnnotationCommandResponse> {
  const trash = loadTrash(database, request.trashId);
  if (trash === null || trash.projectId !== request.projectId || trash.contextId !== request.contextId) return fail("not_found", "annotation Trash entry was not found");
  if (trash.state !== "trashed" || trash.entryKind !== "note") return fail("conflict", "Trash entry is not restorable");
  const restored = loadNote(database, trash.relatedNoteIds[0] ?? "");
  if (restored === null) return fail("not_found", "trashed note is unavailable");
  if (trash.revision !== request.expectedRevision)
    return fail("stale_revision", "Trash entry revision changed");
  const now = clock().toISOString();
  const author = { principalId: access.principalId, actorId: access.actorId, clientId: access.clientId };
  const next = nextRevision(restored.currentVersion);
  const active = AnnotationNoteSchema.parse({
    ...restored,
    state: "active",
    currentVersion: next,
    updatedAt: now,
    archivedAt: null,
    archivedReason: null,
  });
  const versionRow = AnnotationNoteVersionSchema.parse({
    noteId: active.noteId,
    version: next,
    kind: active.kind,
    title: active.title,
    bodyMarkdown: active.bodyMarkdown,
    target: active.target,
    sourceBasisRef: active.sourceBasisRef,
    targetSnapshot: active.targetSnapshot,
    author,
    createdAt: now,
  });
  updateNote(database, active, versionRow);
  const restoredTrash = AnnotationTrashEntrySchema.parse({ ...trash, revision: nextRevision(trash.revision), state: "restored", restoredAt: now, restoredBy: author });
  database.prepare("UPDATE annotation_trash SET state=?,public_json=? WHERE trash_id=?").run(restoredTrash.state, JSON.stringify(restoredTrash), restoredTrash.trashId);
  return { ok: true, value: { operation: request.operation, note: active } };
}
// prettier-ignore
function archiveNote(database: DatabaseSync, note: AnnotationNote, access: WorkspaceAccessContext, reason: string, clock: () => Date, idFactory: (kind: string) => string): AnnotationTrashEntry {
  const now = clock().toISOString();
  const author = { principalId: access.principalId, actorId: access.actorId, clientId: access.clientId };
  const archived = AnnotationNoteSchema.parse({
    ...note,
    state: "archived",
    updatedAt: now,
    archivedAt: now,
    archivedReason: reason,
  });
  database.prepare("UPDATE annotation_notes SET state=?,public_json=? WHERE note_id=?").run(archived.state, JSON.stringify(archived), archived.noteId);
  const entry = AnnotationTrashEntrySchema.parse({
    trashId: idFactory("trash-note"),
    revision: "1",
    projectId: note.projectId,
    contextId: note.contextId,
    entryKind: "note",
    target: note.target,
    formerIdentity: projectObjectReferenceKey(note.target),
    sourceBasisRef: note.sourceBasisRef,
    snapshot: note.targetSnapshot,
    relatedNoteIds: [note.noteId],
    removalReason: reason,
    removedBy: author,
    removedAt: now,
    state: "trashed",
    restoredAt: null,
    restoredBy: null,
  });
  insertTrash(database, entry); return entry;
}
// prettier-ignore
function insertObjectTrash(database: DatabaseSync, target: ProjectObjectReference, snapshot: AnnotationNote["targetSnapshot"], basis: string, access: WorkspaceAccessContext, clock: () => Date, idFactory: (kind: string) => string): AnnotationTrashEntry {
  const entry = AnnotationTrashEntrySchema.parse({
    trashId: idFactory("trash-object"),
    revision: "1",
    projectId: target.projectId,
    contextId: target.contextId,
    entryKind: "object",
    target,
    formerIdentity: projectObjectReferenceKey(target),
    sourceBasisRef: basis,
    snapshot,
    relatedNoteIds: [],
    removalReason: "Authoritative source removal",
    removedBy: { principalId: access.principalId, actorId: access.actorId, clientId: access.clientId },
    removedAt: clock().toISOString(),
    state: "trashed",
    restoredAt: null,
    restoredBy: null,
  });
  insertTrash(database, entry); return entry;
}
function insertNote(
  database: DatabaseSync,
  note: AnnotationNote,
  version: AnnotationNoteVersion,
): void {
  database
    .prepare(
      "INSERT INTO annotation_notes(note_id,project_id,context_id,state,revision,public_json) VALUES(?,?,?,?,?,?)",
    )
    .run(
      note.noteId,
      note.projectId,
      note.contextId,
      note.state,
      note.currentVersion,
      JSON.stringify(note),
    );
  database
    .prepare("INSERT INTO annotation_versions(note_id,version,public_json) VALUES(?,?,?)")
    .run(version.noteId, version.version, JSON.stringify(version));
}
function updateNote(
  database: DatabaseSync,
  note: AnnotationNote,
  version: AnnotationNoteVersion,
): void {
  database
    .prepare("UPDATE annotation_notes SET state=?,revision=?,public_json=? WHERE note_id=?")
    .run(note.state, note.currentVersion, JSON.stringify(note), note.noteId);
  database
    .prepare("INSERT INTO annotation_versions(note_id,version,public_json) VALUES(?,?,?)")
    .run(version.noteId, version.version, JSON.stringify(version));
}
function insertTrash(database: DatabaseSync, entry: AnnotationTrashEntry): void {
  database
    .prepare(
      "INSERT INTO annotation_trash(trash_id,project_id,context_id,state,public_json) VALUES(?,?,?,?,?)",
    )
    .run(entry.trashId, entry.projectId, entry.contextId, entry.state, JSON.stringify(entry));
}
function loadNote(database: DatabaseSync, noteId: string): AnnotationNote | null {
  const row = database
    .prepare("SELECT public_json FROM annotation_notes WHERE note_id=?")
    .get(noteId);
  return parseRow(row, AnnotationNoteSchema);
}
function loadTrash(database: DatabaseSync, trashId: string): AnnotationTrashEntry | null {
  const row = database
    .prepare("SELECT public_json FROM annotation_trash WHERE trash_id=?")
    .get(trashId);
  return parseRow(row, AnnotationTrashEntrySchema);
}
function parseRow<T>(raw: unknown, schema: z.ZodType<T>): T | null {
  const row = JsonRowSchema.safeParse(raw);
  if (!row.success) return null;
  try {
    const value: unknown = JSON.parse(row.data.public_json);
    const parsed = schema.safeParse(value);
    return parsed.success ? parsed.data : null;
  } catch {
    return null;
  }
}
function decodeResponse(raw: string): AnnotationResult<AnnotationCommandResponse> {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail("storage_failure", "stored annotation response is malformed");
  }
  for (const schema of AnnotationCommandResponseSchemas) {
    const parsed = schema.safeParse(value);
    if (parsed.success) return { ok: true, value: parsed.data };
  }
  return fail("storage_failure", "stored annotation response is malformed");
}
function authorized(access: WorkspaceAccessContext, projectId: string): boolean {
  return access.authorizedProjectIds.some((candidate) => candidate === projectId);
}
function nextRevision(value: string): string {
  return String(BigInt(value) + 1n);
}
function hash(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}
function fail<T>(code: AnnotationErrorCode, message: string): AnnotationResult<T> {
  return { ok: false, error: { code, message } };
}
