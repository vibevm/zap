/** Trusted production composition. @scope spec://org.vibevm.zap/lens/PROP-002#shared-client */
import { z } from "zod";
import { resolve } from "node:path";
import { PlanBasisSchema, type PlanBasis, type QuicklensResult } from "../quicklens-model/index.ts";
import { createPrincipalHttpClient, createAgentHttpClient } from "../http/index.ts";
import { CredentialSchema, ConversationIdSchema, WorkspaceIdSchema } from "../protocol/index.ts";
import { createSpecificationWatch, type SpecificationWatch } from "../specification-watch/index.ts";
import { createPlanAuthoring } from "../plan-authoring/index.ts";
import {
  createZapClient,
  ZapIdSchema,
  type ZapClient,
  type ZapHttpExchange,
} from "../zap-client/index.ts";
import { createQuicklensGateway, type QuicklensGateway } from "./gateway.ts";
import { createQuicklensService, type LiveQuicklensDataSource } from "./index.ts";
import { createHumanOwnerWorkflow } from "./owner-workflow.ts";
import { openDurablePlanAuthoring } from "./authoring-journal.ts";
import { openDurableCompositePlanAuthoring } from "./composite-journal.ts";
import { createAgentPlanProposalPort } from "./workflow-agent.ts";
import type { AgentTransportPort } from "../transport/index.ts";
import { createLivePlanWorkflow } from "./workflow.ts";
import { openSqlitePlanWorkflowStore } from "./workflow-store.ts";

const ZapConnectionSchema = z
  .object({
    endpoint: z.url(),
    credentialId: ZapIdSchema,
    bearer: z.string().min(1).max(4_096),
  })
  .strict();
const SpecificationRootSchema = z
  .object({ root: z.string().min(1), include: z.array(z.string().min(1)).min(1).max(100) })
  .strict();
export const AgentPlanRuntimeConfigSchema = z
  .object({
    protocol: z.literal("quicklens/1"),
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    zap: z
      .object({
        reader: ZapConnectionSchema,
        data: ZapConnectionSchema,
        coordinator: ZapConnectionSchema,
      })
      .strict(),
    workflowDatabasePath: z.string().min(1),
    specifications: z.array(SpecificationRootSchema).min(1).max(32),
  })
  .strict();
export const QuicklensSourceRuntimeConfigSchema = AgentPlanRuntimeConfigSchema.extend({
  sourceLabel: z.string().min(1).max(512),
  broker: z.object({ endpoint: z.url(), humanPrincipalToken: CredentialSchema }).strict(),
  zap: z
    .object({
      reader: ZapConnectionSchema,
      data: ZapConnectionSchema,
      coordinator: ZapConnectionSchema,
      owner: ZapConnectionSchema,
    })
    .strict(),
}).strict();
export const QuicklensRuntimeConfigSchema = QuicklensSourceRuntimeConfigSchema.extend({
  gateway: z
    .object({
      namespace: z.string().min(3).max(64),
      pairingToken: z.string().min(24).max(512),
      host: z.enum(["127.0.0.1", "localhost", "::1"]),
      port: z.number().int().min(0).max(65_535),
      allowedHosts: z.array(z.string().min(1)).min(1).max(16),
      allowedOrigins: z.array(z.string().min(1)).min(1).max(16),
    })
    .strict(),
}).strict();
export type QuicklensRuntimeConfig = z.infer<typeof QuicklensRuntimeConfigSchema>;
export type QuicklensSourceRuntimeConfig = z.infer<typeof QuicklensSourceRuntimeConfigSchema>;
export type AgentPlanRuntimeConfig = z.infer<typeof AgentPlanRuntimeConfigSchema>;

export interface QuicklensRuntime {
  readonly source: LiveQuicklensDataSource;
  readonly workflow: ReturnType<typeof createLivePlanWorkflow>;
  readonly gateway: QuicklensGateway;
  readonly address: { readonly host: string; readonly port: number; readonly basePath: string };
  close(): Promise<void>;
}

export interface QuicklensSourceRuntime {
  readonly source: LiveQuicklensDataSource;
  readonly workflow: ReturnType<typeof createLivePlanWorkflow>;
  createAgentPlanning(agent: AgentTransportPort): ReturnType<typeof createAgentPlanProposalPort>;
  close(): void;
}

export interface AgentPlanRuntime {
  readonly port: ReturnType<typeof createAgentPlanProposalPort>;
  close(): void;
}

export async function startQuicklensRuntime(
  raw: unknown,
  location: { readonly configDirectory: string },
): Promise<QuicklensResult<QuicklensRuntime>> {
  const config = QuicklensRuntimeConfigSchema.safeParse(raw);
  if (!config.success) return failure("Quicklens runtime configuration is invalid");
  const normalized = resolvePaths(config.data, location.configDirectory);
  const source = await openSource(normalized);
  if (!source.ok) return source;
  const gateway = createQuicklensGateway({
    source: source.value.source,
    namespace: normalized.gateway.namespace,
    pairingToken: normalized.gateway.pairingToken,
    allowedHosts: normalized.gateway.allowedHosts,
    allowedOrigins: normalized.gateway.allowedOrigins,
  });
  if (!gateway.ok) {
    source.value.close();
    return gateway;
  }
  const address = await gateway.value.start({
    host: normalized.gateway.host,
    port: normalized.gateway.port,
  });
  if (!address.ok) {
    source.value.close();
    return address;
  }
  return {
    ok: true,
    value: {
      source: source.value.source,
      workflow: source.value.workflow,
      gateway: gateway.value,
      address: address.value,
      close: async () => {
        await gateway.value.close();
        source.value.close();
      },
    },
  };
}

export async function openQuicklensSourceRuntime(
  raw: unknown,
  location: { readonly configDirectory: string },
): Promise<QuicklensResult<QuicklensSourceRuntime>> {
  const config = QuicklensSourceRuntimeConfigSchema.safeParse(raw);
  if (!config.success) return failure("Quicklens source runtime configuration is invalid");
  return openSource(resolvePaths(config.data, location.configDirectory));
}

async function openSource(
  config: QuicklensSourceRuntimeConfig,
): Promise<QuicklensResult<QuicklensSourceRuntime>> {
  const common = await openCommon(config, config.zap.owner);
  if (!common.ok) return common;
  const reader = zap(config.zap.reader);
  if (!reader.ok) {
    common.value.close();
    return reader;
  }
  const source = createQuicklensService({
    zap: reader.value,
    broker: createPrincipalHttpClient({
      baseUrl: new URL(config.broker.endpoint),
      principalToken: config.broker.humanPrincipalToken,
    }),
    workflow: common.value.workflow,
    specifications: common.value.specifications,
    workspaceId: config.workspaceId,
    conversationId: config.conversationId,
    sourceLabel: config.sourceLabel,
  });
  if (!source.ok) {
    common.value.close();
    return source;
  }
  return {
    ok: true,
    value: {
      source: source.value,
      workflow: common.value.workflow,
      createAgentPlanning: (agent) =>
        createAgentPlanProposalPort(
          common.value.workflow,
          agent,
          common.value.authoring,
          common.value.journal,
          common.value.compositeJournal,
        ),
      close: () => {
        source.value.close();
        common.value.close();
      },
    },
  };
}

export async function openAgentPlanRuntime(
  raw: unknown,
  input: {
    readonly brokerUrl: URL;
    readonly principalToken: z.infer<typeof CredentialSchema>;
    readonly configDirectory: string;
  },
): Promise<QuicklensResult<AgentPlanRuntime>> {
  const config = AgentPlanRuntimeConfigSchema.safeParse(raw);
  if (!config.success) return failure("Quicklens runtime configuration is invalid");
  const common = await openCommon(resolvePaths(config.data, input.configDirectory));
  if (!common.ok) return common;
  const agent = createAgentHttpClient({
    baseUrl: input.brokerUrl,
    principalToken: input.principalToken,
  });
  return {
    ok: true,
    value: {
      port: createAgentPlanProposalPort(
        common.value.workflow,
        agent,
        common.value.authoring,
        common.value.journal,
        common.value.compositeJournal,
      ),
      close: common.value.close,
    },
  };
}

function resolvePaths<T extends AgentPlanRuntimeConfig>(config: T, directory: string): T {
  return {
    ...config,
    workflowDatabasePath: resolve(directory, config.workflowDatabasePath),
    specifications: config.specifications.map((entry) => ({
      ...entry,
      root: resolve(directory, entry.root),
    })),
  };
}

async function openCommon(
  config: AgentPlanRuntimeConfig,
  ownerConnection?: z.infer<typeof ZapConnectionSchema>,
) {
  const coordinator = zap(config.zap.coordinator);
  if (!coordinator.ok) return coordinator;
  const reader = zap(config.zap.reader);
  if (!reader.ok) return reader;
  const data = zap(config.zap.data);
  if (!data.ok) return data;
  const specifications = await createSpecificationWatch({ roots: config.specifications });
  if (!specifications.ok) return failure(specifications.error.message);
  const store = openSqlitePlanWorkflowStore(config.workflowDatabasePath, {
    workspaceId: config.workspaceId,
    conversationId: config.conversationId,
  });
  if (!store.ok) {
    specifications.value.close();
    return store;
  }
  const validateBasis = basisValidator(reader.value, specifications.value);
  const validateExecution = executionValidator(reader.value, specifications.value);
  const validateAdmissionRetry = admissionRetryValidator(reader.value, specifications.value);
  const authoring = createPlanAuthoring({
    reader: reader.value,
    data: data.value,
    coordinator: coordinator.value,
    specifications: specifications.value,
  });
  const journal = openDurablePlanAuthoring(
    config.workflowDatabasePath,
    { workspaceId: config.workspaceId, conversationId: config.conversationId },
    authoring,
  );
  if (!journal.ok) {
    specifications.value.close();
    store.value.close();
    return failure(journal.error.message);
  }
  const compositeJournal = openDurableCompositePlanAuthoring(
    config.workflowDatabasePath,
    { workspaceId: config.workspaceId, conversationId: config.conversationId },
    authoring,
  );
  if (!compositeJournal.ok) {
    specifications.value.close();
    store.value.close();
    journal.value.close();
    return failure(compositeJournal.error.message);
  }
  const owner = ownerConnection === undefined ? undefined : zap(ownerConnection);
  if (owner !== undefined && !owner.ok) {
    specifications.value.close();
    store.value.close();
    journal.value.close();
    compositeJournal.value.close();
    return owner;
  }
  const workflow = createLivePlanWorkflow({
    reader: reader.value,
    coordinator: coordinator.value,
    store: store.value,
    workspaceId: config.workspaceId,
    conversationId: config.conversationId,
    validateBasis,
    validateExecution,
    validateAdmissionRetry,
    authoring,
    ...(owner?.ok
      ? {
          owner: createHumanOwnerWorkflow({
            ownerZap: owner.value,
            readerZap: reader.value,
            store: store.value,
            validateExecution,
          }),
        }
      : {}),
  });
  return {
    ok: true as const,
    value: {
      workflow,
      coordinator: coordinator.value,
      authoring,
      journal: journal.value,
      compositeJournal: compositeJournal.value,
      specifications: specifications.value,
      close: () => {
        specifications.value.close();
        store.value.close();
        journal.value.close();
        compositeJournal.value.close();
      },
    },
  };
}

function admissionRetryValidator(client: ZapClient, specifications: SpecificationWatch) {
  return async (
    basis: PlanBasis,
    expectedRevision: string,
  ): Promise<QuicklensResult<PlanBasis>> => {
    const parsed = PlanBasisSchema.safeParse(basis);
    if (!parsed.success || !/^(0|[1-9][0-9]*)$/.test(expectedRevision)) {
      return stale("Admission retry basis is malformed");
    }
    const active = await client.activeContext();
    const context = active.ok ? active.value.items[0] : undefined;
    if (
      !active.ok ||
      context === undefined ||
      parsed.data.storeRef !== `store:${context.snapshot.store_id}` ||
      parsed.data.baseRef !== `base:${context.snapshot.base_id}`
    ) {
      return stale("Admission retry store or base changed");
    }
    const actual = BigInt(active.value.revision);
    const expected = BigInt(expectedRevision);
    if (actual > expected) return { ok: true, value: parsed.data };
    if (actual < expected) return stale("Admission retry revision is ahead of ZAP state");
    const sourceDigest = parsed.data.sourceBasisRef.startsWith("source-basis:")
      ? parsed.data.sourceBasisRef.slice(13)
      : "";
    const source = await specifications.verify(sourceDigest);
    return source.ok && source.value.digest === sourceDigest
      ? { ok: true, value: parsed.data }
      : stale("Project specifications changed before an uncommitted admission retry");
  };
}

function basisValidator(client: ZapClient, specifications: SpecificationWatch) {
  return async (basis: PlanBasis): Promise<QuicklensResult<PlanBasis>> => {
    const parsed = PlanBasisSchema.safeParse(basis);
    if (!parsed.success) return stale("Plan basis is malformed");
    const sourceDigest = parsed.data.sourceBasisRef.startsWith("source-basis:")
      ? parsed.data.sourceBasisRef.slice(13)
      : "";
    const source = await specifications.verify(sourceDigest);
    if (!source.ok || source.value.digest !== sourceDigest) {
      return stale("Project specifications changed since the proposal basis");
    }
    const active = await client.activeContext();
    const context = active.ok ? active.value.items[0] : undefined;
    if (
      !active.ok ||
      context === undefined ||
      parsed.data.storeRef !== `store:${context.snapshot.store_id}` ||
      parsed.data.baseRef !== `base:${context.snapshot.base_id}` ||
      parsed.data.revision !== String(active.value.revision)
    ) {
      return stale("ZAP active context changed since the proposal basis");
    }
    return { ok: true, value: parsed.data };
  };
}

function executionValidator(client: ZapClient, specifications: SpecificationWatch) {
  return async (
    basis: PlanBasis,
    expectedRevision: string,
  ): Promise<QuicklensResult<PlanBasis>> => {
    const parsed = PlanBasisSchema.safeParse(basis);
    if (!parsed.success || !/^(0|[1-9][0-9]*)$/.test(expectedRevision)) {
      return stale("Execution reconciliation basis is malformed");
    }
    const sourceDigest = parsed.data.sourceBasisRef.startsWith("source-basis:")
      ? parsed.data.sourceBasisRef.slice(13)
      : "";
    const source = await specifications.verify(sourceDigest);
    if (!source.ok || source.value.digest !== sourceDigest) {
      return stale("Project specifications changed before an uncommitted retry");
    }
    const active = await client.activeContext();
    const context = active.ok ? active.value.items[0] : undefined;
    return active.ok &&
      context !== undefined &&
      parsed.data.storeRef === `store:${context.snapshot.store_id}` &&
      parsed.data.baseRef === `base:${context.snapshot.base_id}` &&
      String(active.value.revision) === expectedRevision
      ? { ok: true, value: parsed.data }
      : stale("ZAP state changed before an uncommitted retry");
  };
}

function zap(input: z.infer<typeof ZapConnectionSchema>): QuicklensResult<ZapClient> {
  const client = createZapClient({
    endpoint: new URL(input.endpoint),
    credential: { id: input.credentialId, bearer: input.bearer },
    exchange: fetchExchange,
  });
  return client.ok ? client : failure(`ZAP client configuration failed: ${client.error.kind}`);
}

const fetchExchange: ZapHttpExchange = {
  request: async (input) => {
    const timeout = AbortSignal.timeout(30_000);
    const signal = input.signal === undefined ? timeout : AbortSignal.any([input.signal, timeout]);
    const response = await fetch(input.url, {
      method: input.method,
      headers: input.headers,
      ...(input.body === undefined ? {} : { body: Buffer.from(input.body) }),
      signal,
    });
    const headers: Record<string, string> = {};
    response.headers.forEach((value, key) => {
      headers[key] = value;
    });
    const chunks: Uint8Array[] = [];
    let length = 0;
    if (response.body !== null) {
      const reader = response.body.getReader();
      for (;;) {
        const chunk = await reader.read();
        if (chunk.done) break;
        length += chunk.value.length;
        if (length > 64 * 1024 * 1024) {
          await reader.cancel();
          throw new Error(
            "violates REQ spec://org.vibevm.zap/lens/PROP-001#nonblocking: ZAP response exceeded the runtime transport bound; fix surface: reduce the response or configured query page",
          );
        }
        chunks.push(chunk.value);
      }
    }
    const body = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      body.set(chunk, offset);
      offset += chunk.length;
    }
    return {
      status: response.status,
      headers,
      body,
    };
  },
};

function stale(message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: { code: "stale_basis", message, recovery: "Refresh Quicklens and prepare again." },
  };
}

function failure(message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code: "unavailable",
      message,
      recovery: "Repair the protected Quicklens runtime configuration and retry.",
    },
  };
}
