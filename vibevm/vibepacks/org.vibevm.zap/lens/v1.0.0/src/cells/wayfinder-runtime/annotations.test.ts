/** Real notes runtime integration. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { ClientRequestIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AgentDescriptorSchema,
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import {
  deterministicIds,
  httpClient,
  planningFeature,
  planningSnapshot,
  runtimeConfig,
  trustedRegistration,
  unavailableNotifications,
} from "./annotations.test-support.ts";
import {
  createWayfinderAnnotationBindings,
  openWayfinderAnnotationsRuntime,
} from "./annotations.ts";
import { createWayfinderRuntime, type WayfinderRuntime } from "./index.ts";

test("default Wayfinder annotations persist through authenticated HTTP without agent turns", async () => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-annotations-http-"));
  const projectDirectory = await mkdtemp(join(root, "project-"));
  const config = runtimeConfig(root);
  const first = createWayfinderRuntime(config);
  assert.equal(first.ok, true);
  if (!first.ok) return;
  let reopened: WayfinderRuntime | undefined;
  try {
    const started = await first.value.start();
    assert.equal(started.ok, true);
    if (!started.ok) return;
    const client = httpClient(first.value, started.value);
    const registered = await client.product.request({
      operation: "product.project.register.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.project"),
      directoryPath: projectDirectory,
      displayName: "Notes project",
      profileId: "profile.annotations.fixture",
    });
    assert.equal(registered.ok, true, registered.ok ? undefined : registered.error.message);
    if (!registered.ok || registered.value.operation !== "product.project.register.v1") return;
    const { projectId, contextId } = registered.value.project;
    const target = ProjectObjectReferenceSchema.parse({
      projectId,
      contextId,
      domain: "project",
      ref: projectId,
    });
    const firstNote = await client.workspace.command({
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.note.one"),
      projectId,
      contextId,
      target,
      kind: "passive",
      title: "First note",
      bodyMarkdown: "Remember the first product detail.",
      sourceBasisRef: "untrusted.client.basis",
      targetSnapshot: null,
    });
    const secondNote = await client.workspace.command({
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.note.two"),
      projectId,
      contextId,
      target,
      kind: "passive",
      title: "Second note",
      bodyMarkdown: "Remember the second product detail.",
      sourceBasisRef: "untrusted.client.basis",
      targetSnapshot: null,
    });
    assert.equal(firstNote.ok, true);
    assert.equal(secondNote.ok, true);
    if (
      !firstNote.ok ||
      firstNote.value.operation !== "annotation.note.create.v1" ||
      !secondNote.ok ||
      secondNote.value.operation !== "annotation.note.create.v1"
    )
      return;
    assert.notEqual(firstNote.value.note.noteId, secondNote.value.note.noteId);
    assert.notEqual(firstNote.value.note.sourceBasisRef, "untrusted.client.basis");
    const network = await client.workspace.read({
      operation: "agent.network.v1",
      projectId,
      contextId,
    });
    assert.equal(
      network.ok && network.value.operation === "agent.network.v1"
        ? network.value.network.agents.length
        : -1,
      0,
    );
    const history = await client.workspace.events({
      cursor: {
        scope: { kind: "context", projectId, contextId },
        afterGlobalSequence: DecimalSchema.parse("0"),
      },
      limit: 100,
    });
    assert.equal(history.ok, true);
    if (history.ok) {
      const noteEvents = history.value.events.filter(
        (event) => event.kind === "annotation_changed",
      );
      assert.equal(noteEvents.length, 2);
      assert.notEqual(noteEvents[0]?.sourceEventId, noteEvents[1]?.sourceEventId);
    }
    const archived = await client.workspace.command({
      operation: "annotation.note.archive.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.archive"),
      projectId,
      contextId,
      noteId: firstNote.value.note.noteId,
      expectedRevision: firstNote.value.note.currentVersion,
      reasonMarkdown: "Move this note to recoverable Trash.",
    });
    assert.equal(archived.ok, true);
    const invalidReference = await client.workspace.command({
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.invalid-ref"),
      projectId,
      contextId,
      target: { ...target, ref: "project.wrong" },
      kind: "passive",
      title: "Invalid target",
      bodyMarkdown: "This must not resolve by label or context alone.",
      sourceBasisRef: "untrusted.client.basis",
      targetSnapshot: null,
    });
    assert.equal(invalidReference.ok, false);
    const invalidScope = await client.workspace.command({
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.invalid-scope"),
      projectId,
      contextId,
      target: {
        ...target,
        projectId: ProjectIdSchema.parse("project.annotations.foreign"),
      },
      kind: "passive",
      title: "Foreign target",
      bodyMarkdown: "This must be rejected before trusted target resolution.",
      sourceBasisRef: "untrusted.client.basis",
      targetSnapshot: null,
    });
    assert.equal(invalidScope.ok, false);
    await first.value.close();

    const openedAgain = createWayfinderRuntime(config);
    assert.equal(openedAgain.ok, true);
    if (!openedAgain.ok) return;
    reopened = openedAgain.value;
    const restarted = await reopened.start();
    assert.equal(restarted.ok, true);
    if (!restarted.ok) return;
    const afterRestart = httpClient(reopened, restarted.value);
    const notes = await afterRestart.workspace.read({
      operation: "annotation.note.list.v1",
      projectId,
      contextId,
      includeArchived: true,
    });
    const trash = await afterRestart.workspace.read({
      operation: "annotation.trash.list.v1",
      projectId,
      contextId,
      limit: 20,
      afterTrashId: null,
    });
    assert.equal(
      notes.ok && notes.value.operation === "annotation.note.list.v1"
        ? notes.value.notes.length
        : -1,
      2,
    );
    assert.equal(
      trash.ok && trash.value.operation === "annotation.trash.list.v1"
        ? trash.value.entries.length
        : -1,
      1,
    );
  } finally {
    if (reopened !== undefined) await reopened.close();
    await first.value.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("trusted semantic snapshots archive only covered targets and real bindings deliver notes", async () => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-annotations-bindings-"));
  const openedStore = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(openedStore.ok, true);
  if (!openedStore.ok) return;
  const store = openedStore.value;
  const registration = trustedRegistration();
  const registered = store.registerProject(registration);
  assert.equal(registered.ok, true);
  const projectId = registration.projectId;
  const contextId = registration.context.contextId;
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.annotations.bindings"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.annotations.bindings"),
    authorizedProjectIds: [projectId],
  });
  const semanticTarget = ProjectObjectReferenceSchema.parse({
    projectId,
    contextId,
    domain: "semantic_object",
    ref: "object.annotations.task",
  });
  const agent = AgentDescriptorSchema.parse({
    actorId: "actor.annotations.retained",
    sessionId: "session.annotations.retained",
    projectId,
    contextId,
    role: "coordinator",
    parentActorId: null,
    displayName: "Retained worker",
    executionMode: "native",
    hostId: "host.annotations.fixture",
    nativeRef: null,
    state: "active",
    revision: "1",
  });
  assert.equal(store.upsertAgent(agent).ok, true);
  const agentTarget = ProjectObjectReferenceSchema.parse({
    projectId,
    contextId,
    domain: "agent",
    ref: agent.actorId,
  });
  let snapshot = planningSnapshot(true);
  const planning = planningFeature(() => snapshot);
  const bindings = createWayfinderAnnotationBindings(store);
  const beforeBind = await bindings.attachments.prepareBeforeWork({
    access,
    attemptId: AttemptIdSchema.parse("attempt.annotations.before-bind"),
    recipientActorId: "actor.annotations.recipient",
    targets: [semanticTarget],
    sourceBasisRef: "basis.annotations.1",
    planRevision: "1",
  });
  assert.equal(beforeBind.ok && beforeBind.value.state, "waiting_for_target");
  const openedAnnotations = openWayfinderAnnotationsRuntime({
    databasePath: join(root, "annotations.sqlite"),
    workspaceStore: store,
    planning,
    notifications: unavailableNotifications(),
    idFactory: deterministicIds(),
    clock: () => new Date("2026-09-16T00:00:00.000Z"),
  });
  assert.equal(openedAnnotations.ok, true);
  if (!openedAnnotations.ok) return;
  bindings.bind(openedAnnotations.value);
  try {
    const deferred = await openedAnnotations.value.service.command(access, {
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.deferred"),
      projectId,
      contextId,
      target: semanticTarget,
      kind: "deferred",
      title: "Check exact task",
      bodyMarkdown: "Apply this instruction before the matching task.",
      sourceBasisRef: "untrusted",
      targetSnapshot: null,
    });
    const retained = await openedAnnotations.value.service.command(access, {
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.agent"),
      projectId,
      contextId,
      target: agentTarget,
      kind: "passive",
      title: "Agent note",
      bodyMarkdown: "This agent note is outside semantic-source coverage.",
      sourceBasisRef: "untrusted",
      targetSnapshot: null,
    });
    assert.equal(deferred.ok, true);
    assert.equal(retained.ok, true);
    if (!deferred.ok || deferred.value.operation !== "annotation.note.create.v1") return;
    const deferredNoteId = deferred.value.note.noteId;
    const deferredNoteVersion = deferred.value.note.currentVersion;
    const delivered = await bindings.attachments.prepareBeforeWork({
      access,
      attemptId: AttemptIdSchema.parse("attempt.annotations.delivery"),
      recipientActorId: "actor.annotations.recipient",
      targets: [semanticTarget],
      sourceBasisRef: "basis.annotations.1",
      planRevision: "1",
    });
    assert.equal(delivered.ok && delivered.value.instructions.length, 1);
    snapshot = planningSnapshot(false, "partial");
    bindings.sourceObserver.observe({
      projectId,
      contextId,
      reason: "partial fixture cannot prove removal",
      snapshot,
    });
    const afterPartial = openedAnnotations.value.store.read(access, {
      operation: "annotation.note.get.v1",
      projectId,
      contextId,
      noteId: deferredNoteId,
    });
    assert.equal(
      afterPartial.ok && afterPartial.value.operation === "annotation.note.get.v1"
        ? afterPartial.value.note.state
        : "missing",
      "active",
    );
    snapshot = planningSnapshot(false);
    bindings.sourceObserver.observe({
      projectId,
      contextId,
      reason: "authoritative fixture removed task",
      snapshot,
    });
    const notes = openedAnnotations.value.store.read(access, {
      operation: "annotation.note.list.v1",
      projectId,
      contextId,
      includeArchived: true,
    });
    assert.equal(notes.ok, true);
    if (notes.ok && notes.value.operation === "annotation.note.list.v1") {
      const semantic = notes.value.notes.find((note) => note.target.ref === semanticTarget.ref);
      const agentNote = notes.value.notes.find((note) => note.target.ref === agentTarget.ref);
      assert.equal(semantic?.state, "archived");
      assert.equal(agentNote?.state, "active");
    }
    const afterRemoval = await bindings.attachments.prepareBeforeWork({
      access,
      attemptId: AttemptIdSchema.parse("attempt.annotations.after-removal"),
      recipientActorId: "actor.annotations.recipient",
      targets: [semanticTarget],
      sourceBasisRef: "basis.annotations.2",
      planRevision: "2",
    });
    assert.equal(afterRemoval.ok && afterRemoval.value.instructions.length, 0);
    const trash = openedAnnotations.value.store.read(access, {
      operation: "annotation.trash.list.v1",
      projectId,
      contextId,
      limit: 10,
      afterTrashId: null,
    });
    assert.equal(trash.ok, true);
    if (!trash.ok || trash.value.operation !== "annotation.trash.list.v1") return;
    const trashedNote = trash.value.entries.find((entry) =>
      entry.relatedNoteIds.includes(deferredNoteId),
    );
    assert.ok(trashedNote);
    const rejectedRestore = await openedAnnotations.value.service.command(access, {
      operation: "annotation.note.restore.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.restore-missing"),
      projectId,
      contextId,
      trashId: trashedNote.trashId,
      expectedRevision: trashedNote.revision,
    });
    assert.equal(rejectedRestore.ok, false);
    assert.equal(rejectedRestore.ok ? "unexpected" : rejectedRestore.error.code, "conflict");
    snapshot = planningSnapshot(true);
    const relinked = await openedAnnotations.value.service.command(access, {
      operation: "annotation.note.relink.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.relink-restored"),
      projectId,
      contextId,
      noteId: deferredNoteId,
      expectedRevision: deferredNoteVersion,
      target: semanticTarget,
      sourceBasisRef: "untrusted.relink.basis",
      targetSnapshot: null,
    });
    assert.equal(relinked.ok, true);
    const afterRelink = await bindings.attachments.prepareBeforeWork({
      access,
      attemptId: AttemptIdSchema.parse("attempt.annotations.after-relink"),
      recipientActorId: "actor.annotations.recipient",
      targets: [semanticTarget],
      sourceBasisRef: "basis.annotations.3",
      planRevision: "3",
    });
    assert.equal(afterRelink.ok && afterRelink.value.instructions.length, 1);
    const removedTarget = ProjectObjectReferenceSchema.parse({
      projectId,
      contextId,
      domain: "semantic_object",
      ref: "object.annotations.removed",
    });
    const removed = openedAnnotations.value.service.observeSource(access, {
      projectId,
      contextId,
      state: "explicit_remove",
      basisRef: "basis.annotations.removed",
      observedAt: "2026-09-16T00:03:00.000Z",
      presentTargets: [],
      removedTargets: [removedTarget],
      snapshots: [
        {
          target: removedTarget,
          snapshot: {
            basisRef: "basis.annotations.removed",
            capturedAt: "2026-09-16T00:03:00.000Z",
            value: { title: "Removed planning object" },
          },
        },
      ],
      coveredDomains: ["semantic_object"],
    });
    assert.equal(removed.ok, true);
    if (!removed.ok) return;
    const objectTrash = removed.value.find((entry) => entry.entryKind === "object");
    assert.ok(objectTrash);
    const restoreIntent = await openedAnnotations.value.service.command(access, {
      operation: "annotation.object.restore.intent.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.object-restore"),
      projectId,
      contextId,
      trashId: objectTrash.trashId,
      expectedRevision: objectTrash.revision,
    });
    assert.equal(restoreIntent.ok, true);
  } finally {
    openedAnnotations.value.close();
    store.close();
    await rm(root, { recursive: true, force: true });
  }
});
