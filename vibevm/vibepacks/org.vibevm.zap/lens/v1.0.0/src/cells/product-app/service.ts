/** Trusted local project setup service. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { createHash } from "node:crypto";
import { basename, isAbsolute } from "node:path";
import { realpath, stat } from "node:fs/promises";
import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import {
  ProductProjectRegistrationRequestSchema,
  ProductProjectSchema,
  ProductProviderProfileSchema,
  ProductSetupRequestSchema,
  type ProductProviderProfile,
  type ProductProjectRegistrationRequest,
  type ProjectId,
  type ProductSetupPort,
  type ProductSetupRequest,
  type ProductSetupResponse,
  type ProductSetupResult,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import {
  type TrustedPlanContextRegistration,
  TrustedProjectRegistrationSchema,
  type TrustedProjectRegistration,
} from "../workspace-store/index.ts";
import type { ProductAppRegistry, ProductPlanContextEntry, ProductProjectEntry } from "./store.ts";
import type {
  ExecutionCatalogCaller,
  ExecutionCatalogService,
} from "../execution-catalog-service/index.ts";
import type { ExecutionCatalogError } from "../execution-catalog/index.ts";

export interface ProductAppService extends ProductSetupPort {
  projectIds(): readonly ProjectId[];
  registrations(): readonly TrustedProjectRegistration[];
  planRegistrations(): readonly TrustedPlanContextRegistration[];
  registerPlanContext(entry: ProductPlanContextEntry): Promise<ProductSetupResult<null>>;
  hydrate(): ProductSetupResult<null>;
}

export interface ProductExecutionConfigurationResolver {
  resolve(input: {
    readonly configurationId: string;
    readonly directoryPath: string;
  }): Promise<ProductSetupResult<ProductProviderProfile>>;
}

export function createProductAppService(options: {
  readonly registry: ProductAppRegistry;
  readonly workspaceStore: WorkspaceStore;
  readonly providers: readonly ProductProviderProfile[];
  readonly additionalProviders?: () => readonly ProductProviderProfile[];
  readonly executionCatalog?: ExecutionCatalogService;
  readonly executionCatalogCaller?: ExecutionCatalogCaller;
  readonly executionConfigurations?: ProductExecutionConfigurationResolver;
  readonly clock?: () => Date;
  readonly prepareRegistration?: (
    registration: TrustedProjectRegistration,
  ) => Promise<ProductSetupResult<null>>;
  readonly preparePlanContext?: (
    registration: TrustedPlanContextRegistration,
  ) => Promise<ProductSetupResult<null>>;
}): ProductSetupResult<ProductAppService> {
  const providers = ProductProviderProfileSchema.array().max(64).safeParse(options.providers);
  if (!providers.success) return fail("invalid_input", "provider catalog is invalid");
  const clock = options.clock ?? (() => new Date());
  const hydrate = (): ProductSetupResult<null> => {
    for (const entry of options.registry.entries()) {
      const registered = options.workspaceStore.registerProject(entry.registration);
      if (!registered.ok) return fail("unavailable", registered.error.message);
      for (const plan of entry.plans) {
        const added = options.workspaceStore.registerPlanContext(plan.registration);
        if (!added.ok) return fail("unavailable", added.error.message);
      }
    }
    return { ok: true, value: null };
  };
  return {
    ok: true,
    value: {
      projectIds: () => options.registry.entries().map((entry) => entry.project.projectId),
      registrations: () => options.registry.entries().map((entry) => entry.registration),
      planRegistrations: () =>
        options.registry.entries().flatMap((entry) => entry.plans.map((plan) => plan.registration)),
      async registerPlanContext(entry) {
        const saved = options.registry.putPlan(entry.registration.projectId, entry);
        if (!saved.ok) return fail("unavailable", saved.message);
        const registered = options.workspaceStore.registerPlanContext(saved.value.registration);
        if (!registered.ok) return fail("unavailable", registered.error.message);
        const prepared = await options.preparePlanContext?.(saved.value.registration);
        return prepared === undefined || prepared.ok ? { ok: true, value: null } : prepared;
      },
      hydrate,
      request: async (raw, authorization) => {
        const request = ProductSetupRequestSchema.safeParse(raw);
        if (!request.success) return fail("invalid_input", "product setup request is invalid");
        if (isProductCatalogRequest(request.data)) {
          if (!authorization?.catalogAdministrator)
            return fail("forbidden", "catalog administration authority is required");
          return catalogRequest(
            options.executionCatalog,
            options.executionCatalogCaller,
            request.data,
          );
        }
        if (request.data.operation === "product.setup.get.v1") {
          const currentProviders = ProductProviderProfileSchema.array()
            .max(64)
            .parse([
              ...providers.data,
              ...(options.additionalProviders?.() ?? []).filter(
                (profile) =>
                  !providers.data.some((candidate) => candidate.profileId === profile.profileId),
              ),
            ]);
          const catalog =
            options.executionCatalog === undefined || options.executionCatalogCaller === undefined
              ? null
              : await options.executionCatalog.get(options.executionCatalogCaller, {});
          const executionConfigurations =
            catalog?.ok === true
              ? catalog.value.snapshot.configurations.filter(
                  (configuration) =>
                    configuration.enabled &&
                    catalog.value.snapshot.connections.some(
                      (connection) =>
                        connection.connectionId === configuration.connectionId &&
                        connection.enabled,
                    ),
                )
              : [];
          return {
            ok: true,
            value: {
              operation: "product.setup.get.v1",
              snapshot: {
                projects: options.registry.entries().map((entry) => entry.project),
                providers: currentProviders,
                executionConfigurations,
                projectLimit: 256,
              },
            },
          };
        }
        return register(
          {
            clientRequestId: request.data.clientRequestId,
            directoryPath: request.data.directoryPath,
            displayName: request.data.displayName,
            profileId: request.data.profileId,
            executionConfigurationId: request.data.executionConfigurationId,
          },
          providers.data,
          options.registry,
          options.workspaceStore,
          clock,
          options.prepareRegistration,
          options.executionConfigurations,
        );
      },
    },
  };
}

type ProductCatalogRequest = Extract<
  ProductSetupRequest,
  { operation: `product.execution-catalog.${string}` }
>;

function isProductCatalogRequest(request: ProductSetupRequest): request is ProductCatalogRequest {
  return request.operation.startsWith("product.execution-catalog.");
}

async function catalogRequest(
  service: ExecutionCatalogService | undefined,
  caller: ExecutionCatalogCaller | undefined,
  request: ProductCatalogRequest,
): Promise<ProductSetupResult<ProductSetupResponse>> {
  if (service === undefined || caller === undefined)
    return fail("unavailable", "execution catalog service is not configured");
  if (request.operation === "product.execution-catalog.get.v1") {
    const result = await service.get(caller, {});
    return result.ok
      ? {
          ok: true,
          value: {
            operation: request.operation,
            administrator: result.value.administrator,
            snapshot: result.value.snapshot,
            availableBindings: [...result.value.availableBindings],
            modelReferences: [...result.value.modelReferences],
          },
        }
      : catalogFail(result.error);
  }
  const identity = {
    clientRequestId: request.clientRequestId,
    sourceEventId: request.sourceEventId,
    expectedCatalogRevision: request.expectedCatalogRevision,
    expectedPreferencesRevision: request.expectedPreferencesRevision,
  };
  const result =
    request.operation === "product.execution-catalog.connection.create.v1"
      ? await service.createConnection(caller, {
          ...identity,
          bindingId: request.bindingId,
          displayName: request.displayName,
        })
      : request.operation === "product.execution-catalog.connection.upsert.v1"
        ? await service.upsertConnection(caller, {
            ...identity,
            connection: request.connection,
          })
        : request.operation === "product.execution-catalog.configuration.create.v1"
          ? await service.createConfiguration(caller, {
              ...identity,
              connectionId: request.connectionId,
              referenceId: request.referenceId,
              displayName: request.displayName,
            })
          : request.operation === "product.execution-catalog.configuration.upsert.v1"
            ? await service.upsertConfiguration(caller, {
                ...identity,
                configuration: request.configuration,
              })
            : request.operation === "product.execution-catalog.preferences.update.v1"
              ? service.updatePreferences(caller, {
                  ...identity,
                  preferences: request.preferences,
                })
              : await service.refreshUsage(caller, {
                  ...identity,
                  connectionId: request.connectionId,
                });
  return result.ok
    ? {
        ok: true,
        value: {
          operation: request.operation,
          snapshot: result.value.snapshot,
          change: result.value.change,
        },
      }
    : catalogFail(result.error);
}

function catalogFail(error: ExecutionCatalogError): ProductSetupResult<never> {
  if (
    error.code === "invalid_input" ||
    error.code === "not_found" ||
    error.code === "conflict" ||
    error.code === "forbidden" ||
    error.code === "stale_revision" ||
    error.code === "idempotency_conflict"
  )
    return fail(error.code, error.message);
  return fail("unavailable", error.message);
}

async function register(
  raw: ProductProjectRegistrationRequest,
  providers: readonly ProductProviderProfile[],
  registry: ProductAppRegistry,
  workspaceStore: WorkspaceStore,
  clock: () => Date,
  prepareRegistration:
    | ((registration: TrustedProjectRegistration) => Promise<ProductSetupResult<null>>)
    | undefined,
  executionConfigurations: ProductExecutionConfigurationResolver | undefined,
): Promise<ProductSetupResult<ProductSetupResponse>> {
  const input = ProductProjectRegistrationRequestSchema.safeParse(raw);
  if (!input.success) return fail("invalid_input", "project registration is invalid");
  const priorRequest = registry.findByRequest(input.data.clientRequestId);
  const requestDigest = digest(input.data);
  if (priorRequest !== null)
    return priorRequest.requestDigest === requestDigest
      ? await finalized(priorRequest, prepareRegistration)
      : fail("conflict", "registration request identity changed content");
  const directory = await canonicalDirectory(input.data.directoryPath);
  if (!directory.ok) return directory;
  const profile = await selectedProfile(
    input.data,
    directory.value,
    providers,
    executionConfigurations,
  );
  if (!profile.ok) return profile;
  const existing = registry.findByDirectory(directory.value);
  if (existing !== null)
    return existing.project.profileId === profile.value.profileId &&
      existing.project.executionConfigurationId === (input.data.executionConfigurationId ?? null)
      ? await finalized(existing, prepareRegistration)
      : fail("conflict", "project directory is already registered with another profile");
  const entry = buildEntry(input.data, directory.value, profile.value, clock());
  const saved = registry.put(entry);
  if (!saved.ok) return fail("unavailable", saved.message);
  const stored = workspaceStore.registerProject(saved.value.registration);
  return stored.ok
    ? finalized(saved.value, prepareRegistration)
    : fail("unavailable", stored.error.message);
}

async function selectedProfile(
  input: ProductProjectRegistrationRequest,
  directoryPath: string,
  providers: readonly ProductProviderProfile[],
  executionConfigurations: ProductExecutionConfigurationResolver | undefined,
): Promise<ProductSetupResult<ProductProviderProfile>> {
  if (input.executionConfigurationId !== undefined) {
    return executionConfigurations === undefined
      ? fail("unavailable", "execution configuration launch is not configured")
      : executionConfigurations.resolve({
          configurationId: input.executionConfigurationId,
          directoryPath,
        });
  }
  const profile = providers.find((candidate) => candidate.profileId === input.profileId);
  if (profile === undefined)
    return fail("not_found", "selected provider profile is not registered");
  return profile.installed && profile.configured && profile.launchable
    ? { ok: true, value: profile }
    : fail("unavailable", "selected provider profile is not available to start");
}

function buildEntry(
  input: ProductProjectRegistrationRequest,
  directoryPath: string,
  profile: ProductProviderProfile,
  now: Date,
): ProductProjectEntry {
  const identity = digest(directoryPath).slice(0, 24);
  const projectId = `project.local.${identity}`;
  const contextId = `context.local.${identity}.main`;
  const displayName = input.displayName ?? basename(directoryPath);
  const registeredAt = now.toISOString();
  const project = ProductProjectSchema.parse({
    projectId,
    contextId,
    displayName,
    directoryPath,
    profileId: profile.profileId,
    executionConfigurationId: input.executionConfigurationId ?? null,
    registeredAt,
  });
  const registration = TrustedProjectRegistrationSchema.parse({
    registrationId: ClientRequestIdSchema.parse(input.clientRequestId),
    projectId,
    displayName,
    repositoryRootRefs: [directoryPath],
    actions: {},
    context: {
      contextId,
      displayName: "Main workspace",
      workspaceRef: `workspace.local.${identity}`,
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable", reason: "No ZAP planning source is configured." },
      coordinatorConversationId: ConversationIdSchema.parse(`conversation.local.${identity}`),
      brokerScope: {
        workspaceId: WorkspaceIdSchema.parse(`workspace.local.${identity}`),
        conversationId: ConversationIdSchema.parse(`conversation.local.${identity}`),
      },
    },
    coordinatorLaunchOptions: [
      {
        profileId: profile.profileId,
        label: profile.displayName,
        interactionKind: profile.interactionKind,
        availability: { state: "available" },
      },
    ],
    protected: { cwd: directoryPath, launchProfileRef: profile.profileId },
  });
  return { project, registration, requestDigest: digest(input), plans: [] };
}

async function canonicalDirectory(path: string): Promise<ProductSetupResult<string>> {
  if (!isAbsolute(path)) return fail("invalid_input", "project directory must be absolute");
  try {
    const canonical = await realpath(path);
    const information = await stat(canonical);
    return information.isDirectory()
      ? { ok: true, value: canonical }
      : fail("invalid_input", "project path is not a directory");
  } catch {
    return fail("not_found", "project directory does not exist or is not readable");
  }
}

function registered(entry: ProductProjectEntry): ProductSetupResult<ProductSetupResponse> {
  return {
    ok: true,
    value: { operation: "product.project.register.v1", project: entry.project },
  };
}

async function finalized(
  entry: ProductProjectEntry,
  prepareRegistration:
    | ((registration: TrustedProjectRegistration) => Promise<ProductSetupResult<null>>)
    | undefined,
): Promise<ProductSetupResult<ProductSetupResponse>> {
  const prepared = await prepareRegistration?.(entry.registration);
  return prepared === undefined || prepared.ok ? registered(entry) : prepared;
}

function digest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function fail(
  code:
    | "invalid_input"
    | "not_found"
    | "conflict"
    | "forbidden"
    | "stale_revision"
    | "idempotency_conflict"
    | "unavailable",
  message: string,
): ProductSetupResult<never> {
  return { ok: false, error: { code, message } };
}
