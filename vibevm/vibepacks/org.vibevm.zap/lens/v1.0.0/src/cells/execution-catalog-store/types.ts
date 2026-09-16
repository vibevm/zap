/** Durable host catalog, history and pinned-selection contracts. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { z } from "zod";
import {
  ExecutionCatalogSnapshotSchema,
  ExecutionSelectionRequestSchema,
  ExecutionSelectionSchema,
  TrustedExecutionCatalogContextSchema,
  type ExecutionCatalogResult,
  type ExecutionCatalogSnapshot,
  type ExecutionSelection,
  type ExecutionSelectionRequest,
  type TrustedExecutionCatalogContext,
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
  type AttemptId,
  type ProjectId,
  type RunId,
  type WorkContextId,
} from "../workspace-model/index.ts";

export const ExecutionCatalogStoreAccessSchema = z
  .object({
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
    hostId: z.string().min(3).max(160),
    authorizedProjectIds: z.array(ProjectIdSchema).max(256),
    catalogAdministrator: z.boolean(),
  })
  .strict();
export type ExecutionCatalogStoreAccess = z.infer<typeof ExecutionCatalogStoreAccessSchema>;

export const ExecutionCatalogChangeSchema = z
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
export type ExecutionCatalogChange = z.infer<typeof ExecutionCatalogChangeSchema>;

export const ReplaceCatalogSnapshotResultSchema = z
  .object({
    snapshot: ExecutionCatalogSnapshotSchema,
    change: ExecutionCatalogChangeSchema,
  })
  .strict();
export type ReplaceCatalogSnapshotResult = z.infer<typeof ReplaceCatalogSnapshotResultSchema>;

export const ReplaceCatalogSnapshotRequestSchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    requestDigest: z.string().regex(/^[a-f0-9]{64}$/),
    sourceEventId: z.string().min(1).max(512),
    expectedCatalogRevision: DecimalSchema,
    expectedPreferencesRevision: DecimalSchema,
    operation: ExecutionCatalogChangeSchema.shape.operation,
    subjectId: z.string().min(1).max(160),
    snapshot: ExecutionCatalogSnapshotSchema,
  })
  .strict();
export type ReplaceCatalogSnapshotRequest = z.infer<typeof ReplaceCatalogSnapshotRequestSchema>;

export const ResolveExecutionPreviewRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    request: ExecutionSelectionRequestSchema,
    trustedContext: TrustedExecutionCatalogContextSchema,
  })
  .strict();
export type ResolveExecutionPreviewRequest = z.infer<typeof ResolveExecutionPreviewRequestSchema>;

export const StoredExecutionSelectionSchema = z
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
export type StoredExecutionSelection = z.infer<typeof StoredExecutionSelectionSchema>;

export const StoreExecutionSelectionRequestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    runId: RunIdSchema,
    attemptId: AttemptIdSchema,
    clientRequestId: ClientRequestIdSchema,
    requestDigest: z.string().regex(/^[a-f0-9]{64}$/),
    sourceEventId: z.string().min(1).max(512),
    selection: ExecutionSelectionSchema,
  })
  .strict();
export type StoreExecutionSelectionRequest = z.infer<typeof StoreExecutionSelectionRequestSchema>;

export interface OpenExecutionCatalogStoreOptions {
  readonly databasePath: string;
  readonly hostId: string;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: string) => string;
}

export interface ExecutionCatalogStore {
  readSnapshot(
    access: ExecutionCatalogStoreAccess,
    projectId: ProjectId | null,
  ): ExecutionCatalogResult<ExecutionCatalogSnapshot>;
  replaySnapshotMutation(
    access: ExecutionCatalogStoreAccess,
    input: {
      readonly clientRequestId: string;
      readonly operation: ReplaceCatalogSnapshotRequest["operation"];
      readonly requestDigest: string;
    },
  ): ExecutionCatalogResult<ReplaceCatalogSnapshotResult | null>;
  replaceSnapshot(
    access: ExecutionCatalogStoreAccess,
    request: ReplaceCatalogSnapshotRequest,
  ): ExecutionCatalogResult<ReplaceCatalogSnapshotResult>;
  listChanges(
    access: ExecutionCatalogStoreAccess,
    projectId: ProjectId | null,
  ): ExecutionCatalogResult<readonly ExecutionCatalogChange[]>;
  resolvePreview(
    access: ExecutionCatalogStoreAccess,
    request: ResolveExecutionPreviewRequest,
  ): ExecutionCatalogResult<ExecutionSelection>;
  replaySelection(
    access: ExecutionCatalogStoreAccess,
    input: { readonly clientRequestId: string; readonly requestDigest: string },
  ): ExecutionCatalogResult<StoredExecutionSelection | null>;
  storeSelection(
    access: ExecutionCatalogStoreAccess,
    request: StoreExecutionSelectionRequest,
  ): ExecutionCatalogResult<StoredExecutionSelection>;
  readSelection(
    access: ExecutionCatalogStoreAccess,
    projectId: ProjectId,
    contextId: WorkContextId,
    runId: RunId,
    attemptId: AttemptId,
  ): ExecutionCatalogResult<StoredExecutionSelection>;
  close(): ExecutionCatalogResult<null>;
}

export type {
  ExecutionCatalogResult,
  ExecutionCatalogSnapshot,
  ExecutionSelection,
  ExecutionSelectionRequest,
  TrustedExecutionCatalogContext,
  ProjectId,
  WorkContextId,
  RunId,
  AttemptId,
};
