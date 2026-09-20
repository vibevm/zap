/** Authenticated catalog read/admin/selection service. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import {
  ExecutionCatalogSnapshotSchema,
  ExecutionSelectionRequestSchema,
  generateExecutionConfigurationName,
  type ExecutionCatalogResult,
} from "../execution-catalog/index.ts";
import type {
  ExecutionCatalogStoreAccess,
  ReplaceCatalogSnapshotRequest,
} from "../execution-catalog-store/index.ts";
import type { ProjectId } from "../workspace-model/index.ts";
import {
  ExecutionCatalogGetRequestSchema,
  ExecutionCatalogHistoryRequestSchema,
  ExecutionCatalogPreferencesUpdateRequestSchema,
  ExecutionCatalogPreviewRequestSchema,
  ExecutionCatalogSelectionGetRequestSchema,
  ExecutionCatalogUsageRecordRequestSchema,
  ExecutionCatalogUsageRefreshRequestSchema,
  ExecutionConfigurationUpsertRequestSchema,
  ExecutionConfigurationCreateRequestSchema,
  ExecutionConnectionUpsertRequestSchema,
  ExecutionConnectionCreateRequestSchema,
  type ExecutionCatalogGetRequest,
  type ExecutionCatalogHistoryRequest,
  type ExecutionCatalogCaller,
  type ExecutionCatalogPreferencesUpdateRequest,
  type ExecutionCatalogPreviewRequest,
  type ExecutionCatalogSelectionGetRequest,
  type ExecutionCatalogService,
  type ExecutionCatalogServiceOptions,
  type ExecutionCatalogUsageRecordRequest,
  type ExecutionCatalogUsageRefreshRequest,
  type ExecutionConfigurationUpsertRequest,
  type ExecutionConfigurationCreateRequest,
  type ExecutionConnectionUpsertRequest,
  type ExecutionConnectionCreateRequest,
} from "./types.ts";
import {
  digest,
  failure,
  increment,
  mutation,
  nameConflict,
  nextCatalog,
  stableId,
  upsert,
} from "./helpers.ts";

export class AuthenticatedExecutionCatalogService implements ExecutionCatalogService {
  readonly #options: ExecutionCatalogServiceOptions;
  readonly #clock: () => Date;

  constructor(options: ExecutionCatalogServiceOptions) {
    this.#options = options;
    this.#clock = options.clock ?? (() => new Date());
  }

  async get(rawAccess: ExecutionCatalogCaller, rawRequest: ExecutionCatalogGetRequest) {
    const request = ExecutionCatalogGetRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog get request is malformed");
    const access = this.#globalAccess(rawAccess, false);
    if (!access.ok) return access;
    const snapshot = this.#options.store.readSnapshot(access.value, null);
    if (!snapshot.ok) return snapshot;
    const bindings = await this.#options.authority.availableBindings(access.value);
    if (!bindings.ok) return bindings;
    const references = await this.#options.authority.modelReferences(access.value);
    return references.ok
      ? {
          ok: true as const,
          value: {
            feature: "execution-catalog.get.v1" as const,
            administrator: access.value.catalogAdministrator,
            snapshot: snapshot.value,
            availableBindings: bindings.value,
            modelReferences: references.value,
          },
        }
      : references;
  }

  async preview(rawAccess: ExecutionCatalogCaller, rawRequest: ExecutionCatalogPreviewRequest) {
    const request = ExecutionCatalogPreviewRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog preview request is malformed");
    const access = this.#access(rawAccess, request.data.projectId, false);
    if (!access.ok) return access;
    const selectionRequest = ExecutionSelectionRequestSchema.parse({
      ...request.data.request,
      requestedAt: this.#clock().toISOString(),
    });
    const snapshot = this.#options.store.readSnapshot(access.value, request.data.projectId);
    if (!snapshot.ok) return snapshot;
    const trusted = await this.#options.authority.trustedContext({
      access: access.value,
      projectId: request.data.projectId,
      contextId: request.data.contextId,
      request: selectionRequest,
      snapshot: snapshot.value,
      parentSelection: null,
    });
    if (!trusted.ok) return trusted;
    const result = this.#options.store.resolvePreview(access.value, {
      projectId: request.data.projectId,
      contextId: request.data.contextId,
      request: selectionRequest,
      trustedContext: trusted.value,
    });
    return {
      ok: true as const,
      value: { feature: "execution-catalog.preview.v1" as const, result },
    };
  }

  history(rawAccess: ExecutionCatalogCaller, rawRequest: ExecutionCatalogHistoryRequest) {
    const request = ExecutionCatalogHistoryRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog history request is malformed");
    const access = this.#globalAccess(rawAccess, false);
    if (!access.ok) return access;
    const changes = this.#options.store.listChanges(access.value, null);
    return changes.ok
      ? {
          ok: true as const,
          value: { feature: "execution-catalog.history.v1" as const, changes: changes.value },
        }
      : changes;
  }

  selection(rawAccess: ExecutionCatalogCaller, rawRequest: ExecutionCatalogSelectionGetRequest) {
    const request = ExecutionCatalogSelectionGetRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "execution selection request is malformed");
    const access = this.#access(rawAccess, request.data.projectId, false);
    if (!access.ok) return access;
    const selection = this.#options.store.readSelection(
      access.value,
      request.data.projectId,
      request.data.contextId,
      request.data.runId,
      request.data.attemptId,
    );
    return selection.ok
      ? {
          ok: true as const,
          value: {
            feature: "execution-catalog.selection.get.v1" as const,
            selection: selection.value,
          },
        }
      : selection;
  }

  async upsertConnection(
    rawAccess: ExecutionCatalogCaller,
    rawRequest: ExecutionConnectionUpsertRequest,
  ) {
    const request = ExecutionConnectionUpsertRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "connection upsert request is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "connection_upsert");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const validated = await this.#options.authority.validateConnection({
      access: access.value,
      connection: request.data.connection,
    });
    if (!validated.ok) return validated;
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const duplicateBinding = current.value.connections.find(
      (candidate) =>
        candidate.launchBindingId === validated.value.launchBindingId &&
        candidate.connectionId !== validated.value.connectionId,
    );
    if (duplicateBinding !== undefined)
      return failure("conflict", "protected launch binding already belongs to another connection");
    if (
      nameConflict(
        current.value.connections,
        validated.value.connectionId,
        validated.value.displayName,
      )
    )
      return failure("conflict", "connection display name is already in use");
    const snapshot = nextCatalog(current.value, this.#clock, {
      connections: upsert(current.value.connections, validated.value, "connectionId"),
    });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "connection_upsert", validated.value.connectionId, snapshot),
    );
  }

  async createConnection(
    rawAccess: ExecutionCatalogCaller,
    rawRequest: ExecutionConnectionCreateRequest,
  ) {
    const request = ExecutionConnectionCreateRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "connection create request is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "connection_upsert");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const bindings = await this.#options.authority.availableBindings(access.value);
    if (!bindings.ok) return bindings;
    const binding = bindings.value.find(
      (candidate) => candidate.bindingId === request.data.bindingId,
    );
    if (binding === undefined)
      return failure("not_found", "protected execution binding is unavailable");
    const existing = current.value.connections.find(
      (candidate) => candidate.launchBindingId === binding.bindingId,
    );
    if (existing !== undefined) {
      if (existing.enabled) return failure("conflict", "this account connection is already active");
      const restored = await this.#options.authority.validateConnection({
        access: access.value,
        connection: {
          ...existing,
          displayName: request.data.displayName ?? existing.displayName,
          enabled: true,
        },
      });
      if (!restored.ok) return restored;
      if (
        nameConflict(
          current.value.connections,
          restored.value.connectionId,
          restored.value.displayName,
        )
      )
        return failure("conflict", "connection display name is already in use");
      const snapshot = nextCatalog(current.value, this.#clock, {
        connections: upsert(current.value.connections, restored.value, "connectionId"),
      });
      return this.#options.store.replaceSnapshot(
        access.value,
        mutation(request.data, "connection_upsert", restored.value.connectionId, snapshot),
      );
    }
    const connectionId = stableId(
      "connection",
      access.value.principalId,
      request.data.clientRequestId,
    );
    const displayName = request.data.displayName ?? binding.displayName;
    const validated = await this.#options.authority.createConnection({
      access: access.value,
      bindingId: binding.bindingId,
      connectionId,
      displayName,
      now: this.#clock().toISOString(),
    });
    if (!validated.ok) return validated;
    const duplicate = current.value.connections.find(
      (candidate) =>
        candidate.launchBindingId === validated.value.launchBindingId &&
        candidate.connectionId !== connectionId,
    );
    if (duplicate !== undefined)
      return failure("conflict", "protected launch binding already belongs to another connection");
    if (nameConflict(current.value.connections, connectionId, validated.value.displayName))
      return failure("conflict", "connection display name is already in use");
    const snapshot = nextCatalog(current.value, this.#clock, {
      connections: upsert(current.value.connections, validated.value, "connectionId"),
    });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "connection_upsert", connectionId, snapshot),
    );
  }

  async upsertConfiguration(
    rawAccess: ExecutionCatalogCaller,
    rawRequest: ExecutionConfigurationUpsertRequest,
  ) {
    const request = ExecutionConfigurationUpsertRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "configuration upsert request is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "configuration_upsert");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const connection = current.value.connections.find(
      (candidate) => candidate.connectionId === request.data.configuration.connectionId,
    );
    if (connection === undefined)
      return failure("not_found", "configuration connection does not exist");
    const validated = await this.#options.authority.validateConfiguration({
      access: access.value,
      connection,
      configuration: request.data.configuration,
    });
    if (!validated.ok) return validated;
    if (
      nameConflict(
        current.value.configurations,
        validated.value.configurationId,
        validated.value.displayName,
      )
    )
      return failure("conflict", "configuration display name is already in use");
    const snapshot = nextCatalog(current.value, this.#clock, {
      configurations: upsert(current.value.configurations, validated.value, "configurationId"),
    });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "configuration_upsert", validated.value.configurationId, snapshot),
    );
  }

  async createConfiguration(
    rawAccess: ExecutionCatalogCaller,
    rawRequest: ExecutionConfigurationCreateRequest,
  ) {
    const request = ExecutionConfigurationCreateRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "configuration create request is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "configuration_upsert");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const connection = current.value.connections.find(
      (candidate) => candidate.connectionId === request.data.connectionId,
    );
    if (connection === undefined)
      return failure("not_found", "configuration connection does not exist");
    const references = await this.#options.authority.modelReferences(access.value);
    if (!references.ok) return references;
    const reference = references.value.find(
      (candidate) => candidate.referenceId === request.data.referenceId,
    );
    if (reference === undefined)
      return failure("not_found", "execution model reference is unavailable");
    const existing = current.value.configurations.find(
      (candidate) =>
        candidate.connectionId === connection.connectionId &&
        candidate.modelId === reference.modelId,
    );
    if (existing !== undefined) {
      if (existing.enabled)
        return failure("conflict", "this named model configuration is already active");
      const normalized = await this.#options.authority.validateConfiguration({
        access: access.value,
        connection,
        configuration: { ...existing, enabled: false },
      });
      if (!normalized.ok) return normalized;
      const restored = await this.#options.authority.validateConfiguration({
        access: access.value,
        connection,
        configuration: { ...normalized.value, enabled: true },
      });
      if (!restored.ok) return restored;
      const snapshot = nextCatalog(current.value, this.#clock, {
        configurations: upsert(current.value.configurations, restored.value, "configurationId"),
      });
      return this.#options.store.replaceSnapshot(
        access.value,
        mutation(request.data, "configuration_upsert", restored.value.configurationId, snapshot),
      );
    }
    const configurationId = stableId(
      "configuration",
      access.value.principalId,
      request.data.clientRequestId,
    );
    const displayName =
      request.data.displayName ??
      generateExecutionConfigurationName(
        {
          agentProductName: connection.agentProduct,
          modelName: reference.modelId,
          accountName: connection.displayName,
          qualifier: null,
          configurationId,
        },
        current.value.configurations.map((candidate) => candidate.displayName),
      );
    const validated = await this.#options.authority.createConfiguration({
      access: access.value,
      connection,
      referenceId: reference.referenceId,
      configurationId,
      displayName,
      now: this.#clock().toISOString(),
    });
    if (!validated.ok) return validated;
    if (nameConflict(current.value.configurations, configurationId, validated.value.displayName))
      return failure("conflict", "configuration display name is already in use");
    const snapshot = nextCatalog(current.value, this.#clock, {
      configurations: upsert(current.value.configurations, validated.value, "configurationId"),
    });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "configuration_upsert", configurationId, snapshot),
    );
  }

  updatePreferences(
    rawAccess: ExecutionCatalogCaller,
    rawRequest: ExecutionCatalogPreferencesUpdateRequest,
  ) {
    const request = ExecutionCatalogPreferencesUpdateRequestSchema.safeParse(rawRequest);
    if (!request.success)
      return failure("invalid_input", "catalog preferences request is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "preferences_update");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const snapshot = ExecutionCatalogSnapshotSchema.parse({
      ...current.value,
      preferencesRevision: increment(current.value.preferencesRevision),
      preferences: request.data.preferences,
      updatedAt: this.#clock().toISOString(),
    });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "preferences_update", "preferences", snapshot),
    );
  }

  recordUsage(rawAccess: ExecutionCatalogCaller, rawRequest: ExecutionCatalogUsageRecordRequest) {
    const request = ExecutionCatalogUsageRecordRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog usage request is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "usage_record");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const snapshot = nextCatalog(current.value, this.#clock, { usage: request.data.usage });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "usage_record", "usage", snapshot),
    );
  }

  async refreshUsage(
    rawAccess: ExecutionCatalogCaller,
    rawRequest: ExecutionCatalogUsageRefreshRequest,
  ) {
    const request = ExecutionCatalogUsageRefreshRequestSchema.safeParse(rawRequest);
    if (!request.success) return failure("invalid_input", "catalog usage refresh is malformed");
    const access = this.#globalAccess(rawAccess, true);
    if (!access.ok) return access;
    const replay = this.#replayMutation(access.value, request.data, "usage_record");
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    if (this.#options.usage === undefined)
      return failure("not_found", "execution usage observation is not configured");
    const current = this.#options.store.readSnapshot(access.value, null);
    if (!current.ok) return current;
    const connection = current.value.connections.find(
      (candidate) => candidate.connectionId === request.data.connectionId,
    );
    if (connection === undefined) return failure("not_found", "execution connection is missing");
    const observed = await this.#options.usage.refresh({
      connection,
      configurations: current.value.configurations.filter(
        (configuration) => configuration.connectionId === connection.connectionId,
      ),
    });
    if (!observed.ok) return observed;
    const usage = [
      ...current.value.usage.filter(
        (observation) => observation.connectionId !== connection.connectionId,
      ),
      ...observed.value,
    ];
    const snapshot = nextCatalog(current.value, this.#clock, { usage });
    return this.#options.store.replaceSnapshot(
      access.value,
      mutation(request.data, "usage_record", connection.connectionId, snapshot),
    );
  }

  async selectAndPin(
    rawAccess: ExecutionCatalogCaller,
    input: Parameters<ExecutionCatalogService["selectAndPin"]>[1],
  ) {
    const access = this.#access(rawAccess, input.projectId, false);
    if (!access.ok) return access;
    const requestDigest = digest({
      projectId: input.projectId,
      contextId: input.contextId,
      runId: input.runId,
      attemptId: input.attemptId,
      request: input.request,
    });
    const replay = this.#options.store.replaySelection(access.value, {
      clientRequestId: input.clientRequestId,
      requestDigest,
    });
    if (!replay.ok) return replay;
    if (replay.value !== null) return { ok: true as const, value: replay.value };
    const request = ExecutionSelectionRequestSchema.safeParse({
      ...input.request,
      requestedAt: this.#clock().toISOString(),
    });
    if (!request.success)
      return failure("invalid_input", "execution selection request is malformed");
    const snapshot = this.#options.store.readSnapshot(access.value, input.projectId);
    if (!snapshot.ok) return snapshot;
    const trusted = await this.#options.authority.trustedContext({
      access: access.value,
      projectId: input.projectId,
      contextId: input.contextId,
      request: request.data,
      snapshot: snapshot.value,
      parentSelection: input.parentSelection ?? null,
    });
    if (!trusted.ok) return trusted;
    const selected = this.#options.store.resolvePreview(access.value, {
      projectId: input.projectId,
      contextId: input.contextId,
      request: request.data,
      trustedContext: trusted.value,
    });
    if (!selected.ok) return selected;
    return this.#options.store.storeSelection(access.value, {
      projectId: input.projectId,
      contextId: input.contextId,
      runId: input.runId,
      attemptId: input.attemptId,
      clientRequestId: input.clientRequestId,
      requestDigest,
      sourceEventId: input.sourceEventId,
      selection: selected.value,
    });
  }

  #access(
    raw: ExecutionCatalogCaller,
    projectId: ProjectId,
    admin: boolean,
  ): ExecutionCatalogResult<ExecutionCatalogStoreAccess> {
    const resolved = this.#options.access.resolve(raw);
    if (!resolved.ok) return resolved;
    if (!resolved.value.authorizedProjectIds.includes(projectId))
      return failure("forbidden", "project is outside authorized catalog scope");
    if (admin && !resolved.value.catalogAdministrator)
      return failure("forbidden", "catalog administration authority is required");
    return resolved;
  }

  #globalAccess(
    raw: ExecutionCatalogCaller,
    admin: boolean,
  ): ExecutionCatalogResult<ExecutionCatalogStoreAccess> {
    const resolved = this.#options.access.resolve(raw);
    if (!resolved.ok) return resolved;
    return admin && !resolved.value.catalogAdministrator
      ? failure("forbidden", "catalog administration authority is required")
      : resolved;
  }

  #replayMutation(
    access: ExecutionCatalogStoreAccess,
    request: {
      readonly clientRequestId: string;
      readonly expectedCatalogRevision: string;
      readonly expectedPreferencesRevision: string;
      readonly sourceEventId: string;
    },
    operation: ReplaceCatalogSnapshotRequest["operation"],
  ) {
    return this.#options.store.replaySnapshotMutation(access, {
      clientRequestId: request.clientRequestId,
      operation,
      requestDigest: digest(request),
    });
  }
}

export function createExecutionCatalogService(
  options: ExecutionCatalogServiceOptions,
): ExecutionCatalogService {
  return new AuthenticatedExecutionCatalogService(options);
}
