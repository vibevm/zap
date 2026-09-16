import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import { ClientRequestIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { createAnnotationWorkAttachmentPort, openAnnotationStore } from "./index.ts";

test("annotation notes persist, enforce CAS/idempotency, archive to Trash and suppress delivery", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap-annotation-test-"));
  const databasePath = join(root, "annotations.sqlite");
  const projectId = ProjectIdSchema.parse("project.annotations.a");
  const contextId = WorkContextIdSchema.parse("context.annotations.a");
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.annotations"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.annotations"),
    authorizedProjectIds: [projectId],
  });
  const target = ProjectObjectReferenceSchema.parse({
    projectId,
    contextId,
    domain: "semantic_object",
    ref: "task.annotations.a",
  });
  const open = openAnnotationStore({ databasePath, idFactory: (kind) => `${kind}.test` });
  assert.equal(open.ok, true);
  if (!open.ok) return;
  const request = {
    operation: "annotation.note.create.v1" as const,
    clientRequestId: ClientRequestIdSchema.parse("request.annotation.create"),
    projectId,
    contextId,
    target,
    kind: "deferred" as const,
    title: "Check the task",
    bodyMarkdown: "Review this exact task before work begins.",
    sourceBasisRef: "basis.annotations.1",
    targetSnapshot: {
      basisRef: "basis.annotations.1",
      capturedAt: "2026-09-15T20:00:00.000Z",
      value: { title: "Task A" },
    },
  };
  const created = open.value.command(access, request);
  assert.equal(created.ok, true);
  if (!created.ok) return;
  assert.equal(open.value.command(access, request).ok, true);
  const conflicting = open.value.command(access, { ...request, bodyMarkdown: "changed" });
  assert.equal(conflicting.ok, false);
  if (!conflicting.ok) assert.equal(conflicting.error.code, "idempotency_conflict");
  if (created.value.operation !== "annotation.note.create.v1") return;
  const note = created.value.note;
  const stale = open.value.command(access, {
    operation: "annotation.note.update.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.annotation.stale"),
    projectId,
    contextId,
    noteId: note.noteId,
    expectedRevision: DecimalSchema.parse("0"),
    title: "Stale",
    bodyMarkdown: "Stale",
    sourceBasisRef: "basis.annotations.0",
    targetSnapshot: null,
  });
  assert.equal(stale.ok, false);
  if (!stale.ok) assert.equal(stale.error.code, "stale_revision");
  const attachments = createAnnotationWorkAttachmentPort({
    store: open.value,
    resolver: {
      resolve: async () => ({
        state: "present",
        snapshot: {
          basisRef: "basis.annotations.1",
          capturedAt: "2026-09-15T20:00:00.000Z",
          value: { title: "Task A" },
        },
      }),
    },
    idFactory: (kind) => `${kind}.test`,
    clock: () => new Date("2026-09-15T20:01:00.000Z"),
  });
  const offered = await attachments.prepareBeforeWork({
    access,
    attemptId: AttemptIdSchema.parse("attempt.annotations"),
    recipientActorId: "actor.annotations",
    targets: [target],
    sourceBasisRef: "basis.annotations.1",
    planRevision: DecimalSchema.parse("1"),
  });
  assert.equal(offered.ok, true);
  if (offered.ok) {
    assert.equal(offered.value.state, "ready");
    assert.equal(offered.value.instructions.length, 1);
  }
  const retry = await attachments.prepareBeforeWork({
    access,
    attemptId: AttemptIdSchema.parse("attempt.annotations"),
    recipientActorId: "actor.annotations",
    targets: [target],
    sourceBasisRef: "basis.annotations.1",
    planRevision: DecimalSchema.parse("1"),
  });
  assert.equal(retry.ok, true);
  if (offered.ok && retry.ok)
    assert.equal(
      retry.value.instructions[0]?.attachmentId,
      offered.value.instructions[0]?.attachmentId,
    );
  if (offered.ok && offered.value.instructions[0] !== undefined) {
    const otherAccess = WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.other"),
      actorId: "actor.other",
      clientId: ClientIdSchema.parse("client.other"),
      authorizedProjectIds: [projectId],
    });
    const refused = await attachments.acknowledge({
      access: otherAccess,
      attemptId: AttemptIdSchema.parse("attempt.annotations"),
      attachmentId: offered.value.instructions[0].attachmentId,
      version: offered.value.instructions[0].version,
    });
    assert.equal(refused.ok, false);
  }
  const archived = open.value.command(access, {
    operation: "annotation.note.archive.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.annotation.archive"),
    projectId,
    contextId,
    noteId: note.noteId,
    expectedRevision: DecimalSchema.parse("1"),
    reasonMarkdown: "Task was removed by authoritative source.",
  });
  assert.equal(archived.ok, true);
  const trash = open.value.read(access, {
    operation: "annotation.trash.list.v1",
    projectId,
    contextId,
    limit: 20,
    afterTrashId: null,
  });
  assert.equal(trash.ok, true);
  if (trash.ok && trash.value.operation === "annotation.trash.list.v1")
    assert.equal(trash.value.entries.length, 1);
  const suppressed = await attachments.prepareBeforeWork({
    access,
    attemptId: AttemptIdSchema.parse("attempt.annotations.2"),
    recipientActorId: "actor.annotations",
    targets: [target],
    sourceBasisRef: "basis.annotations.2",
    planRevision: DecimalSchema.parse("2"),
  });
  assert.equal(suppressed.ok, true);
  if (suppressed.ok) assert.equal(suppressed.value.instructions.length, 0);
  if (archived.ok && archived.value.operation === "annotation.note.archive.v1") {
    const restored = open.value.command(access, {
      operation: "annotation.note.restore.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotation.restore"),
      projectId,
      contextId,
      trashId: archived.value.trash.trashId,
      expectedRevision: archived.value.trash.revision,
    });
    assert.equal(restored.ok, true);
    if (restored.ok && restored.value.operation === "annotation.note.restore.v1") {
      assert.equal(restored.value.note.state, "active");
      assert.equal(restored.value.note.currentVersion, "2");
    }
  }
  open.value.close();
  const reopened = openAnnotationStore({ databasePath });
  assert.equal(reopened.ok, true);
  if (reopened.ok) {
    const loaded = reopened.value.read(access, {
      operation: "annotation.note.list.v1",
      projectId,
      contextId,
      includeArchived: true,
    });
    assert.equal(loaded.ok, true);
    reopened.value.close();
  }
  await rm(root, { recursive: true, force: true });
});

test("partial source observations never archive notes and explicit removal creates object Trash", () => {
  const open = openAnnotationStore({
    databasePath: ":memory:",
    idFactory: (kind) => `${kind}.source`,
  });
  assert.equal(open.ok, true);
  if (!open.ok) return;
  const projectId = ProjectIdSchema.parse("project.annotations.source");
  const contextId = WorkContextIdSchema.parse("context.annotations.source");
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.annotations.source"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.annotations.source"),
    authorizedProjectIds: [projectId],
  });
  const target = ProjectObjectReferenceSchema.parse({
    projectId,
    contextId,
    domain: "semantic_object",
    ref: "task.source",
  });
  const partial = open.value.observeSource(access, {
    projectId,
    contextId,
    state: "partial",
    basisRef: "basis.partial",
    observedAt: "2026-09-15T20:00:00.000Z",
    presentTargets: [],
    removedTargets: [target],
    snapshots: [],
    coveredDomains: ["semantic_object"],
  });
  assert.deepEqual(partial, { ok: true, value: [] });
  const removed = open.value.observeSource(access, {
    projectId,
    contextId,
    state: "explicit_remove",
    basisRef: "basis.remove",
    observedAt: "2026-09-15T20:00:00.000Z",
    presentTargets: [],
    removedTargets: [target],
    snapshots: [],
    coveredDomains: ["semantic_object"],
  });
  assert.equal(removed.ok, true);
  if (removed.ok) assert.equal(removed.value[0]?.entryKind, "object");
  open.value.close();
});
