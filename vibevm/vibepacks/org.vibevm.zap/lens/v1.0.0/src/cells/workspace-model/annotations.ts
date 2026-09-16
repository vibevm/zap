/** Durable notes, deferred instructions and recoverable Trash wire contracts. @scope spec://org.vibevm.zap/lens/PROP-011#anchored-notes */
import { z } from "zod";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
  JsonValueSchema,
  PrincipalIdSchema,
} from "../protocol/index.ts";
import { ProjectObjectReferenceSchema } from "./object-reference.ts";
import { ClientIdSchema, ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";

export const AnnotationKindSchema = z.enum(["passive", "deferred"]);
export type AnnotationKind = z.infer<typeof AnnotationKindSchema>;
export const AnnotationStateSchema = z.enum(["active", "archived", "needs_relink"]);
export type AnnotationState = z.infer<typeof AnnotationStateSchema>;
export const AnnotationDeliveryStateSchema = z.enum([
  "offered",
  "read",
  "acknowledged",
  "resolved",
  "uncertain",
  "suppressed",
]);
export type AnnotationDeliveryState = z.infer<typeof AnnotationDeliveryStateSchema>;

export const AnnotationAuthorSchema = z
  .object({
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
  })
  .strict();
export type AnnotationAuthor = z.infer<typeof AnnotationAuthorSchema>;

export const AnnotationTargetSnapshotSchema = z
  .object({
    basisRef: z.string().min(1).max(512),
    capturedAt: z.iso.datetime(),
    value: JsonValueSchema,
  })
  .strict();
export type AnnotationTargetSnapshot = z.infer<typeof AnnotationTargetSnapshotSchema>;

export const AnnotationNoteVersionSchema = z
  .object({
    noteId: z.string().min(3).max(160),
    version: DecimalSchema,
    kind: AnnotationKindSchema,
    title: z.string().min(1).max(512),
    bodyMarkdown: z.string().min(1).max(100_000),
    target: ProjectObjectReferenceSchema,
    sourceBasisRef: z.string().min(1).max(512),
    targetSnapshot: AnnotationTargetSnapshotSchema.nullable(),
    author: AnnotationAuthorSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type AnnotationNoteVersion = z.infer<typeof AnnotationNoteVersionSchema>;

export const AnnotationNoteSchema = z
  .object({
    noteId: z.string().min(3).max(160),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    target: ProjectObjectReferenceSchema,
    kind: AnnotationKindSchema,
    state: AnnotationStateSchema,
    currentVersion: DecimalSchema,
    title: z.string().min(1).max(512),
    bodyMarkdown: z.string().min(1).max(100_000),
    sourceBasisRef: z.string().min(1).max(512),
    targetSnapshot: AnnotationTargetSnapshotSchema.nullable(),
    author: AnnotationAuthorSchema,
    createdAt: z.iso.datetime(),
    updatedAt: z.iso.datetime(),
    archivedAt: z.iso.datetime().nullable(),
    archivedReason: z.string().max(8_000).nullable(),
  })
  .strict();
export type AnnotationNote = z.infer<typeof AnnotationNoteSchema>;

export const AnnotationTrashEntrySchema = z
  .object({
    trashId: z.string().min(3).max(160),
    revision: DecimalSchema,
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    entryKind: z.enum(["object", "note"]),
    target: ProjectObjectReferenceSchema,
    formerIdentity: z.string().min(1).max(1_152),
    sourceBasisRef: z.string().min(1).max(512),
    snapshot: AnnotationTargetSnapshotSchema.nullable(),
    relatedNoteIds: z.array(z.string().min(3).max(160)).max(256),
    removalReason: z.string().min(1).max(8_000),
    removedBy: AnnotationAuthorSchema,
    removedAt: z.iso.datetime(),
    state: z.enum(["trashed", "restored"]),
    restoredAt: z.iso.datetime().nullable(),
    restoredBy: AnnotationAuthorSchema.nullable(),
  })
  .strict();
export type AnnotationTrashEntry = z.infer<typeof AnnotationTrashEntrySchema>;

export const AnnotationDeliverySchema = z
  .object({
    deliveryId: z.string().min(3).max(160),
    noteId: z.string().min(3).max(160),
    noteVersion: DecimalSchema,
    attemptId: z.string().min(3).max(160),
    recipientActorId: z.string().min(3).max(160),
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    state: AnnotationDeliveryStateSchema,
    offeredAt: z.iso.datetime(),
    acknowledgedAt: z.iso.datetime().nullable(),
    resolvedAt: z.iso.datetime().nullable(),
    messageId: z.string().min(3).max(160).nullable(),
  })
  .strict();
export type AnnotationDelivery = z.infer<typeof AnnotationDeliverySchema>;

export const AnnotationSourceObservationSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    state: z.enum(["authoritative_full", "explicit_remove", "partial", "unavailable"]),
    basisRef: z.string().min(1).max(512),
    observedAt: z.iso.datetime(),
    presentTargets: z.array(ProjectObjectReferenceSchema).max(100_000),
    removedTargets: z.array(ProjectObjectReferenceSchema).max(100_000),
    snapshots: z
      .array(
        z
          .object({
            target: ProjectObjectReferenceSchema,
            snapshot: AnnotationTargetSnapshotSchema,
          })
          .strict(),
      )
      .max(100_000),
    coveredDomains: z.array(ProjectObjectReferenceSchema.shape.domain).min(1).max(5),
  })
  .strict();
export type AnnotationSourceObservation = z.infer<typeof AnnotationSourceObservationSchema>;

const ScopeSchema = z.object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema });
export const AnnotationReadRequestSchemas = [
  ScopeSchema.extend({
    operation: z.literal("annotation.note.list.v1"),
    includeArchived: z.boolean(),
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("annotation.note.get.v1"),
    noteId: z.string().min(3).max(160),
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("annotation.trash.list.v1"),
    limit: z.number().int().min(1).max(256),
    afterTrashId: z.string().min(3).max(160).nullable(),
  }).strict(),
] as const;

const CommandScopeSchema = ScopeSchema.extend({ clientRequestId: ClientRequestIdSchema });
export const AnnotationCommandRequestSchemas = [
  CommandScopeSchema.extend({
    operation: z.literal("annotation.note.create.v1"),
    target: ProjectObjectReferenceSchema,
    kind: AnnotationKindSchema,
    title: z.string().min(1).max(512),
    bodyMarkdown: z.string().min(1).max(100_000),
    sourceBasisRef: z.string().min(1).max(512),
    targetSnapshot: AnnotationTargetSnapshotSchema.nullable(),
  }).strict(),
  CommandScopeSchema.extend({
    operation: z.literal("annotation.note.update.v1"),
    noteId: z.string().min(3).max(160),
    expectedRevision: DecimalSchema,
    title: z.string().min(1).max(512),
    bodyMarkdown: z.string().min(1).max(100_000),
    sourceBasisRef: z.string().min(1).max(512),
    targetSnapshot: AnnotationTargetSnapshotSchema.nullable(),
  }).strict(),
  CommandScopeSchema.extend({
    operation: z.literal("annotation.note.archive.v1"),
    noteId: z.string().min(3).max(160),
    expectedRevision: DecimalSchema,
    reasonMarkdown: z.string().min(1).max(8_000),
  }).strict(),
  CommandScopeSchema.extend({
    operation: z.literal("annotation.note.restore.v1"),
    trashId: z.string().min(3).max(160),
    expectedRevision: DecimalSchema,
  }).strict(),
  CommandScopeSchema.extend({
    operation: z.literal("annotation.note.relink.v1"),
    noteId: z.string().min(3).max(160),
    expectedRevision: DecimalSchema,
    target: ProjectObjectReferenceSchema,
    sourceBasisRef: z.string().min(1).max(512),
    targetSnapshot: AnnotationTargetSnapshotSchema.nullable(),
  }).strict(),
  CommandScopeSchema.extend({
    operation: z.literal("annotation.note.send.v1"),
    noteId: z.string().min(3).max(160),
    expectedRevision: DecimalSchema,
  }).strict(),
  CommandScopeSchema.extend({
    operation: z.literal("annotation.object.restore.intent.v1"),
    trashId: z.string().min(3).max(160),
    expectedRevision: DecimalSchema,
  }).strict(),
] as const;

export type AnnotationReadRequest = z.infer<(typeof AnnotationReadRequestSchemas)[number]>;
export type AnnotationCommandRequest = z.infer<(typeof AnnotationCommandRequestSchemas)[number]>;

export const AnnotationReadResponseSchemas = [
  z
    .object({
      operation: z.literal("annotation.note.list.v1"),
      notes: z.array(AnnotationNoteSchema),
    })
    .strict(),
  z
    .object({
      operation: z.literal("annotation.note.get.v1"),
      note: AnnotationNoteSchema,
      versions: z.array(AnnotationNoteVersionSchema),
    })
    .strict(),
  z
    .object({
      operation: z.literal("annotation.trash.list.v1"),
      entries: z.array(AnnotationTrashEntrySchema),
      nextTrashId: z.string().min(3).max(160).nullable(),
      total: z.number().int().min(0),
    })
    .strict(),
] as const;
export const AnnotationCommandResponseSchemas = [
  z
    .object({ operation: z.literal("annotation.note.create.v1"), note: AnnotationNoteSchema })
    .strict(),
  z
    .object({ operation: z.literal("annotation.note.update.v1"), note: AnnotationNoteSchema })
    .strict(),
  z
    .object({
      operation: z.literal("annotation.note.archive.v1"),
      note: AnnotationNoteSchema,
      trash: AnnotationTrashEntrySchema,
    })
    .strict(),
  z
    .object({ operation: z.literal("annotation.note.restore.v1"), note: AnnotationNoteSchema })
    .strict(),
  z
    .object({ operation: z.literal("annotation.note.relink.v1"), note: AnnotationNoteSchema })
    .strict(),
  z
    .object({
      operation: z.literal("annotation.note.send.v1"),
      observation: z.enum(["offered", "queued"]),
      messageId: z.string().min(3).max(160).nullable(),
    })
    .strict(),
  z
    .object({
      operation: z.literal("annotation.object.restore.intent.v1"),
      intentRef: z.string().min(3).max(160),
    })
    .strict(),
] as const;
export type AnnotationReadResponse = z.infer<(typeof AnnotationReadResponseSchemas)[number]>;
export type AnnotationCommandResponse = z.infer<(typeof AnnotationCommandResponseSchemas)[number]>;
