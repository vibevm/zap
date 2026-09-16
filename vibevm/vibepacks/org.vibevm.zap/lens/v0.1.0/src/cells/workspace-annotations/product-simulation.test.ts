/** Public-service annotation product scenario. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { AnnotationsProductSimulationSchema } from "../mock-simulation/index.ts";
import { ActorIdSchema, ClientRequestIdSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  ProjectObjectReferenceSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import {
  createAnnotationService,
  createAnnotationWorkAttachmentPort,
  openAnnotationStore,
} from "./index.ts";

const scenario = AnnotationsProductSimulationSchema.parse(
  JSON.parse(
    readFileSync(new URL("./deferred-trash-restore.simulation.json", import.meta.url), "utf8"),
  ),
);

test("deferred note delivery, acknowledgement, Trash and restore use public services", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const seed = process.env["ZAP_MOCK_SIMULATION_SEED"] ?? scenario.seed;
  const root = await mkdtemp(join(tmpdir(), "zap-annotations-product-"));
  const databasePath = join(root, "annotations.sqlite");
  try {
    let nextId = 0;
    const opened = openAnnotationStore({
      databasePath,
      clock: () => new Date("2026-09-16T12:00:00.000Z"),
      idFactory: (kind) => `${kind}.product-${String(++nextId)}`,
    });
    assert.equal(opened.ok, true);
    if (!opened.ok) return;
    const input = scenario.inputs;
    const projectId = ProjectIdSchema.parse(input.projectId);
    const contextId = WorkContextIdSchema.parse(input.contextId);
    const target = ProjectObjectReferenceSchema.parse({
      projectId,
      contextId,
      domain: input.targetDomain,
      ref: input.targetRef,
    });
    const snapshot = {
      basisRef: input.sourceBasisRef,
      capturedAt: "2026-09-16T12:00:00.000Z",
      value: { title: input.title },
    };
    const resolver = {
      resolve: async () => ({ state: "present" as const, snapshot }),
    };
    const service = createAnnotationService({ store: opened.value, resolver });
    const access = WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.annotations.product"),
      actorId: null,
      clientId: ClientIdSchema.parse("client.annotations.product"),
      authorizedProjectIds: [projectId],
    });
    const created = await service.command(access, {
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.create"),
      projectId,
      contextId,
      target,
      kind: "deferred",
      title: input.title,
      bodyMarkdown: input.bodyMarkdown,
      sourceBasisRef: input.sourceBasisRef,
      targetSnapshot: snapshot,
    });
    assert.equal(created.ok, true);
    if (!created.ok || created.value.operation !== "annotation.note.create.v1") return;
    const attachments = createAnnotationWorkAttachmentPort({
      store: opened.value,
      resolver,
      clock: () => new Date("2026-09-16T12:01:00.000Z"),
      idFactory: (kind) => `${kind}.product-${String(++nextId)}`,
    });
    const firstAttemptId = AttemptIdSchema.parse(input.firstAttemptId);
    const first = await attachments.prepareBeforeWork({
      access,
      attemptId: firstAttemptId,
      recipientActorId: input.recipientActorId,
      targets: [target],
      sourceBasisRef: input.sourceBasisRef,
      planRevision: "1",
    });
    assert.equal(first.ok, true);
    if (!first.ok) return;
    assert.equal(first.value.instructions.length, scenario.expected.initialInstructionCount);
    const instruction = first.value.instructions[0];
    assert.notEqual(instruction, undefined);
    if (instruction === undefined) return;
    const wrongActor = await attachments.acknowledge({
      access: WorkspaceAccessContextSchema.parse({
        ...access,
        actorId: ActorIdSchema.parse("actor.annotations.other"),
      }),
      attemptId: firstAttemptId,
      attachmentId: instruction.attachmentId,
      version: instruction.version,
    });
    assert.equal(!wrongActor.ok, scenario.expected.wrongActorAckRefused);
    const acknowledged = await attachments.acknowledge({
      access: WorkspaceAccessContextSchema.parse({
        ...access,
        actorId: ActorIdSchema.parse(input.recipientActorId),
      }),
      attemptId: firstAttemptId,
      attachmentId: instruction.attachmentId,
      version: instruction.version,
    });
    assert.equal(acknowledged.ok, true);
    const archived = await service.command(access, {
      operation: "annotation.note.archive.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.archive"),
      projectId,
      contextId,
      noteId: created.value.note.noteId,
      expectedRevision: created.value.note.currentVersion,
      reasonMarkdown: "The work item was retired.",
    });
    assert.equal(archived.ok, true);
    if (!archived.ok || archived.value.operation !== "annotation.note.archive.v1") return;
    const trash = service.read(access, {
      operation: "annotation.trash.list.v1",
      projectId,
      contextId,
      limit: 20,
      afterTrashId: null,
    });
    assert.equal(trash.ok, true);
    if (!trash.ok || trash.value.operation !== "annotation.trash.list.v1") return;
    assert.equal(trash.value.entries.length, scenario.expected.trashEntryCount);
    const whileArchived = await attachments.prepareBeforeWork({
      access,
      attemptId: AttemptIdSchema.parse("attempt.annotations.archived"),
      recipientActorId: input.recipientActorId,
      targets: [target],
      sourceBasisRef: input.sourceBasisRef,
      planRevision: "2",
    });
    assert.equal(whileArchived.ok, true);
    if (whileArchived.ok)
      assert.equal(
        whileArchived.value.instructions.length,
        scenario.expected.archivedInstructionCount,
      );
    const restored = await service.command(access, {
      operation: "annotation.note.restore.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.annotations.restore"),
      projectId,
      contextId,
      trashId: archived.value.trash.trashId,
      expectedRevision: archived.value.trash.revision,
    });
    assert.equal(restored.ok, true);
    if (!restored.ok || restored.value.operation !== "annotation.note.restore.v1") return;
    assert.equal(restored.value.note.currentVersion, scenario.expected.restoredVersion);
    const afterRestore = await attachments.prepareBeforeWork({
      access,
      attemptId: AttemptIdSchema.parse(input.restoredAttemptId),
      recipientActorId: input.recipientActorId,
      targets: [target],
      sourceBasisRef: input.sourceBasisRef,
      planRevision: "3",
    });
    assert.equal(afterRestore.ok, true);
    if (afterRestore.ok)
      assert.equal(
        afterRestore.value.instructions.length,
        scenario.expected.restoredInstructionCount,
      );
    opened.value.close();
    const reopened = openAnnotationStore({ databasePath });
    assert.equal(reopened.ok, true);
    if (reopened.ok) {
      const persisted = reopened.value.read(access, {
        operation: "annotation.note.get.v1",
        projectId,
        contextId,
        noteId: created.value.note.noteId,
      });
      assert.equal(persisted.ok, true);
      if (persisted.ok && persisted.value.operation === "annotation.note.get.v1")
        assert.equal(persisted.value.note.currentVersion, scenario.expected.restoredVersion);
      reopened.value.close();
    }
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: scenario.scenarioId,
        seed,
        passed: true,
        inputPosition: 8,
        zeroLlmInference: scenario.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
