/** Authenticated execution catalog application-service contracts. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { z } from "zod";
import {
  ExecutionBindingChoiceSchema,
  ExecutionCatalogPreferencesSchema,
  ExecutionConfigurationRecordSchema,
  ExecutionConnectionRecordSchema,
  ExecutionModelReferenceViewSchema,
  ExecutionSelectionRequestSchema,
  UsageObservationSchema,
  type ExecutionCatalogResult,
  type ExecutionCatalogSnapshot,
  type ExecutionConfigurationRecord,
  type ExecutionConnectionRecord,
  type ExecutionBindingChoice,
  type ExecutionModelReferenceView,
  type ExecutionSelection,
  type ExecutionSelectionRequest,
  type TrustedExecutionCatalogContext,
  type UsageObservation,
} from "../execution-catalog/index.ts";
import type {
  ExecutionCatalogChange,
  ExecutionCatalogStore,
  ExecutionCatalogStoreAccess,
  StoredExecutionSelection,
} from "../execution-catalog-store/index.ts";
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
} from "../workspace-model/index.ts";

const CatalogScopeSchema = z
  .object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema })
  .strict();
const MutationIdentitySchema = z
  .object({
    clientRequestId: ClientRequestIdSchema,
    sourceEventId: z.string().min(1).max(512),
    expectedCatalogRevision: DecimalSchema,
    expectedPreferencesRevision: DecimalSchema,
  })
  .strict();

export const ExecutionCatalogGetRequestSchema = z.object({}).strict();
export type ExecutionCatalogGetRequest = z.infer<typeof ExecutionCatalogGetRequestSchema>;

export const ExecutionCatalogPreviewRequestSchema = CatalogScopeSchema.extend({
  request: ExecutionSelectionRequestSchema.omit({ requestedAt: true }),
}).strict();
export type ExecutionCatalogPreviewRequest = z.infer<typeof ExecutionCatalogPreviewRequestSchema>;

export const ExecutionCatalogHistoryRequestSchema = z.object({}).strict();
export type ExecutionCatalogHistoryRequest = z.infer<typeof ExecutionCatalogHistoryRequestSchema>;

export const ExecutionCatalogSelectionGetRequestSchema = CatalogScopeSchema.extend({
  runId: RunIdSchema,
  attemptId: AttemptIdSchema,
}).strict();
export type ExecutionCatalogSelectionGetRequest = z.infer<
  typeof ExecutionCatalogSelectionGetRequestSchema
>;

export const ExecutionConnectionUpsertRequestSchema = MutationIdentitySchema.extend({
  connection: ExecutionConnectionRecordSchema,
}).strict();
export type ExecutionConnectionUpsertRequest = z.infer<
  typeof ExecutionConnectionUpsertRequestSchema
>;

export const ExecutionConnectionCreateRequestSchema = MutationIdentitySchema.extend({
  bindingId: ExecutionBindingChoiceSchema.shape.bindingId,
  displayName: z.string().trim().min(1).max(160).nullable(),
}).strict();
export type ExecutionConnectionCreateRequest = z.infer<
  typeof ExecutionConnectionCreateRequestSchema
>;

export const ExecutionConfigurationUpsertRequestSchema = MutationIdentitySchema.extend({
  configuration: ExecutionConfigurationRecordSchema,
}).strict();
export type ExecutionConfigurationUpsertRequest = z.infer<
  typeof ExecutionConfigurationUpsertRequestSchema
>;

export const ExecutionConfigurationCreateRequestSchema = MutationIdentitySchema.extend({
  connectionId: ExecutionConnectionRecordSchema.shape.connectionId,
  referenceId: ExecutionModelReferenceViewSchema.shape.referenceId,
  displayName: z.string().trim().min(1).max(200).nullable(),
}).strict();
export type ExecutionConfigurationCreateRequest = z.infer<
  typeof ExecutionConfigurationCreateRequestSchema
>;

export const ExecutionCatalogPreferencesUpdateRequestSchema = MutationIdentitySchema.extend({
  preferences: ExecutionCatalogPreferencesSchema,
}).strict();
export type ExecutionCatalogPreferencesUpdateRequest = z.infer<
  typeof ExecutionCatalogPreferencesUpdateRequestSchema
>;

export const ExecutionCatalogUsageRecordRequestSchema = MutationIdentitySchema.extend({
  usage: z.array(UsageObservationSchema).max(10_000),
}).strict();
export type ExecutionCatalogUsageRecordRequest = z.infer<
  typeof ExecutionCatalogUsageRecordRequestSchema
>;
export const ExecutionCatalogUsageRefreshRequestSchema = MutationIdentitySchema.extend({
  connectionId: ExecutionConnectionRecordSchema.shape.connectionId,
}).strict();
export type ExecutionCatalogUsageRefreshRequest = z.infer<
  typeof ExecutionCatalogUsageRefreshRequestSchema
>;

export interface ExecutionCatalogView {
  readonly feature: "execution-catalog.get.v1";
  readonly administrator: boolean;
  readonly snapshot: ExecutionCatalogSnapshot;
  readonly availableBindings: readonly ExecutionBindingChoice[];
  readonly modelReferences: readonly ExecutionModelReferenceView[];
}
export interface ExecutionCatalogPreview {
  readonly feature: "execution-catalog.preview.v1";
  readonly result: ExecutionCatalogResult<ExecutionSelection>;
}
export interface ExecutionCatalogHistory {
  readonly feature: "execution-catalog.history.v1";
  readonly changes: readonly ExecutionCatalogChange[];
}
export interface ExecutionCatalogSelectionView {
  readonly feature: "execution-catalog.selection.get.v1";
  readonly selection: StoredExecutionSelection;
}
export interface ExecutionCatalogMutation {
  readonly snapshot: ExecutionCatalogSnapshot;
  readonly change: ExecutionCatalogChange;
}

export const ExecutionCatalogCallerSchema = z
  .object({
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
    authorizedProjectIds: z.array(ProjectIdSchema).max(256),
    catalogAdministrator: z.boolean().optional(),
  })
  .strict();
export type ExecutionCatalogCaller = z.infer<typeof ExecutionCatalogCallerSchema>;

export interface ExecutionCatalogAccessPort {
  resolve(access: ExecutionCatalogCaller): ExecutionCatalogResult<ExecutionCatalogStoreAccess>;
}

export interface ExecutionCatalogAuthorityPort {
  availableBindings(
    access: ExecutionCatalogStoreAccess,
  ):
    | Promise<ExecutionCatalogResult<readonly ExecutionBindingChoice[]>>
    | ExecutionCatalogResult<readonly ExecutionBindingChoice[]>;
  modelReferences(
    access: ExecutionCatalogStoreAccess,
  ):
    | Promise<ExecutionCatalogResult<readonly ExecutionModelReferenceView[]>>
    | ExecutionCatalogResult<readonly ExecutionModelReferenceView[]>;
  createConnection(input: {
    readonly access: ExecutionCatalogStoreAccess;
    readonly bindingId: string;
    readonly connectionId: string;
    readonly displayName: string;
    readonly now: string;
  }):
    | Promise<ExecutionCatalogResult<ExecutionConnectionRecord>>
    | ExecutionCatalogResult<ExecutionConnectionRecord>;
  createConfiguration(input: {
    readonly access: ExecutionCatalogStoreAccess;
    readonly connection: ExecutionConnectionRecord;
    readonly referenceId: string;
    readonly configurationId: string;
    readonly displayName: string;
    readonly now: string;
  }):
    | Promise<ExecutionCatalogResult<ExecutionConfigurationRecord>>
    | ExecutionCatalogResult<ExecutionConfigurationRecord>;
  validateConnection(input: {
    readonly access: ExecutionCatalogStoreAccess;
    readonly connection: ExecutionConnectionRecord;
  }):
    | Promise<ExecutionCatalogResult<ExecutionConnectionRecord>>
    | ExecutionCatalogResult<ExecutionConnectionRecord>;
  validateConfiguration(input: {
    readonly access: ExecutionCatalogStoreAccess;
    readonly connection: ExecutionConnectionRecord;
    readonly configuration: ExecutionConfigurationRecord;
  }):
    | Promise<ExecutionCatalogResult<ExecutionConfigurationRecord>>
    | ExecutionCatalogResult<ExecutionConfigurationRecord>;
  trustedContext(input: {
    readonly access: ExecutionCatalogStoreAccess;
    readonly projectId: string;
    readonly contextId: string;
    readonly request: ExecutionSelectionRequest;
    readonly snapshot: ExecutionCatalogSnapshot;
    readonly parentSelection: TrustedExecutionCatalogContext["parentSelection"];
  }):
    | Promise<ExecutionCatalogResult<TrustedExecutionCatalogContext>>
    | ExecutionCatalogResult<TrustedExecutionCatalogContext>;
}

export interface ExecutionCatalogUsageRefreshPort {
  refresh(input: {
    readonly connection: ExecutionConnectionRecord;
    readonly configurations: readonly ExecutionConfigurationRecord[];
  }): Promise<ExecutionCatalogResult<readonly UsageObservation[]>>;
}

export interface ExecutionCatalogService {
  get(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogGetRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogView>>;
  preview(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogPreviewRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogPreview>>;
  history(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogHistoryRequest,
  ): ExecutionCatalogResult<ExecutionCatalogHistory>;
  selection(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogSelectionGetRequest,
  ): ExecutionCatalogResult<ExecutionCatalogSelectionView>;
  upsertConnection(
    access: ExecutionCatalogCaller,
    request: ExecutionConnectionUpsertRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogMutation>>;
  createConnection(
    access: ExecutionCatalogCaller,
    request: ExecutionConnectionCreateRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogMutation>>;
  upsertConfiguration(
    access: ExecutionCatalogCaller,
    request: ExecutionConfigurationUpsertRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogMutation>>;
  createConfiguration(
    access: ExecutionCatalogCaller,
    request: ExecutionConfigurationCreateRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogMutation>>;
  updatePreferences(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogPreferencesUpdateRequest,
  ): ExecutionCatalogResult<ExecutionCatalogMutation>;
  recordUsage(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogUsageRecordRequest,
  ): ExecutionCatalogResult<ExecutionCatalogMutation>;
  refreshUsage(
    access: ExecutionCatalogCaller,
    request: ExecutionCatalogUsageRefreshRequest,
  ): Promise<ExecutionCatalogResult<ExecutionCatalogMutation>>;
  selectAndPin(
    access: ExecutionCatalogCaller,
    input: {
      readonly projectId: z.infer<typeof ProjectIdSchema>;
      readonly contextId: z.infer<typeof WorkContextIdSchema>;
      readonly runId: z.infer<typeof RunIdSchema>;
      readonly attemptId: z.infer<typeof AttemptIdSchema>;
      readonly clientRequestId: z.infer<typeof ClientRequestIdSchema>;
      readonly sourceEventId: string;
      readonly request: Omit<ExecutionSelectionRequest, "requestedAt">;
      readonly parentSelection?: TrustedExecutionCatalogContext["parentSelection"];
    },
  ): Promise<ExecutionCatalogResult<StoredExecutionSelection>>;
}

export interface ExecutionCatalogServiceOptions {
  readonly store: ExecutionCatalogStore;
  readonly access: ExecutionCatalogAccessPort;
  readonly authority: ExecutionCatalogAuthorityPort;
  readonly usage?: ExecutionCatalogUsageRefreshPort;
  readonly clock?: () => Date;
  readonly idFactory?: (kind: "connection" | "configuration") => string;
}

export type { UsageObservation };
