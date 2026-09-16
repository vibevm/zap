/** Public execution catalog wire schemas. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { z } from "zod";
import {
  ExecutionCatalogErrorSchema,
  ExecutionBindingChoiceSchema,
  ExecutionCatalogPreferencesSchema,
  ExecutionCatalogSnapshotSchema,
  ExecutionConfigurationRecordSchema,
  ExecutionConnectionRecordSchema,
  ExecutionModelReferenceViewSchema,
  ExecutionSelectionRequestSchema,
  ExecutionSelectionSchema,
} from "../execution-catalog/index.ts";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
  PrincipalIdSchema,
} from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
} from "./ids.ts";

export const ExecutionCatalogChangeViewSchema = z
  .object({
    changeId: z.string().min(3).max(160),
    hostId: z.string().min(3).max(160),
    operation: z.enum([
      "connection_upsert",
      "configuration_upsert",
      "preferences_update",
      "usage_record",
    ]),
    subjectId: z.string().min(1).max(160),
    sourceEventId: z.string().min(1).max(512),
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
    fromCatalogRevision: DecimalSchema,
    toCatalogRevision: DecimalSchema,
    fromPreferencesRevision: DecimalSchema,
    toPreferencesRevision: DecimalSchema,
    changedAt: z.iso.datetime(),
  })
  .strict();

export const StoredExecutionSelectionViewSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    selection: ExecutionSelectionSchema,
    sourceEventId: z.string().min(1).max(512),
    actorId: ActorIdSchema.nullable(),
    storedAt: z.iso.datetime(),
  })
  .strict();

const MutationIdentitySchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    sourceEventId: z.string().min(1).max(512),
    expectedCatalogRevision: DecimalSchema,
    expectedPreferencesRevision: DecimalSchema,
  })
  .strict();
const CatalogScopeSchema = z
  .object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema })
  .strict();

export const ExecutionCatalogReadRequestSchemas = [
  CatalogScopeSchema.extend({ operation: z.literal("execution-catalog.get.v1") }).strict(),
  z
    .object({
      operation: z.literal("execution-catalog.preview.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      request: ExecutionSelectionRequestSchema.omit({ requestedAt: true }),
    })
    .strict(),
  CatalogScopeSchema.extend({ operation: z.literal("execution-catalog.history.v1") }).strict(),
  z
    .object({
      operation: z.literal("execution-catalog.selection.get.v1"),
      projectId: ProjectIdSchema,
      contextId: WorkContextIdSchema,
      runId: RunIdSchema,
      attemptId: AttemptIdSchema,
    })
    .strict(),
] as const;

const SelectionResultSchema = z.union([
  z.object({ ok: z.literal(true), value: ExecutionSelectionSchema }).strict(),
  z.object({ ok: z.literal(false), error: ExecutionCatalogErrorSchema }).strict(),
]);

export const ExecutionCatalogReadResponseSchemas = [
  z
    .object({
      operation: z.literal("execution-catalog.get.v1"),
      administrator: z.boolean(),
      snapshot: ExecutionCatalogSnapshotSchema,
      availableBindings: z.array(ExecutionBindingChoiceSchema),
      modelReferences: z.array(ExecutionModelReferenceViewSchema),
    })
    .strict(),
  z
    .object({ operation: z.literal("execution-catalog.preview.v1"), result: SelectionResultSchema })
    .strict(),
  z
    .object({
      operation: z.literal("execution-catalog.history.v1"),
      changes: z.array(ExecutionCatalogChangeViewSchema),
    })
    .strict(),
  z
    .object({
      operation: z.literal("execution-catalog.selection.get.v1"),
      selection: StoredExecutionSelectionViewSchema,
    })
    .strict(),
] as const;

const CatalogMutationSchema = CatalogScopeSchema.extend(MutationIdentitySchema.shape);

export const ExecutionCatalogCommandRequestSchemas = [
  CatalogMutationSchema.extend({
    operation: z.literal("execution-catalog.connection.create.v1"),
    bindingId: ExecutionBindingChoiceSchema.shape.bindingId,
    displayName: z.string().trim().min(1).max(160).nullable(),
  }).strict(),
  CatalogMutationSchema.extend({
    operation: z.literal("execution-catalog.connection.upsert.v1"),
    connection: ExecutionConnectionRecordSchema,
  }).strict(),
  CatalogMutationSchema.extend({
    operation: z.literal("execution-catalog.configuration.create.v1"),
    connectionId: ExecutionConnectionRecordSchema.shape.connectionId,
    referenceId: ExecutionModelReferenceViewSchema.shape.referenceId,
    displayName: z.string().trim().min(1).max(200).nullable(),
  }).strict(),
  CatalogMutationSchema.extend({
    operation: z.literal("execution-catalog.configuration.upsert.v1"),
    configuration: ExecutionConfigurationRecordSchema,
  }).strict(),
  CatalogMutationSchema.extend({
    operation: z.literal("execution-catalog.preferences.update.v1"),
    preferences: ExecutionCatalogPreferencesSchema,
  }).strict(),
  CatalogMutationSchema.extend({
    operation: z.literal("execution-catalog.usage.refresh.v1"),
    connectionId: ExecutionConnectionRecordSchema.shape.connectionId,
  }).strict(),
] as const;

const CatalogMutationResponseSchema = z.object({
  snapshot: ExecutionCatalogSnapshotSchema,
  change: ExecutionCatalogChangeViewSchema,
});
export const ExecutionCatalogCommandResponseSchemas = [
  CatalogMutationResponseSchema.extend({
    operation: z.literal("execution-catalog.connection.create.v1"),
  }).strict(),
  CatalogMutationResponseSchema.extend({
    operation: z.literal("execution-catalog.connection.upsert.v1"),
  }).strict(),
  CatalogMutationResponseSchema.extend({
    operation: z.literal("execution-catalog.configuration.create.v1"),
  }).strict(),
  CatalogMutationResponseSchema.extend({
    operation: z.literal("execution-catalog.configuration.upsert.v1"),
  }).strict(),
  CatalogMutationResponseSchema.extend({
    operation: z.literal("execution-catalog.preferences.update.v1"),
  }).strict(),
  CatalogMutationResponseSchema.extend({
    operation: z.literal("execution-catalog.usage.refresh.v1"),
  }).strict(),
] as const;

export type ExecutionCatalogChangeView = z.infer<typeof ExecutionCatalogChangeViewSchema>;
export type StoredExecutionSelectionView = z.infer<typeof StoredExecutionSelectionViewSchema>;
