/**
 * Official-SDK MCP bridge for the bounded lens/1 agent tools.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-001#transport
 * @example
 * const bridge = createCodlensMcpServer({ broker, principalToken });
 * await bridge.connect(transport);
 */
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  AckInputSchema,
  AskInputSchema,
  ConnectInputSchema,
  ClientRequestIdSchema,
  DelegateInputSchema,
  EmitInputSchema,
  HostBindingSchema,
  ForwardInboxInputSchema,
  InboxInputSchema,
  type BrokerError,
  type Result,
} from "../protocol/index.ts";
import { AdapterSessionIdSchema, type AgentTransportPort } from "../transport/index.ts";

export const MCP_PROTOCOL_REVISION = "2025-11-25";

const ModelHostBindingSchema = HostBindingSchema.extend({
  provenance: z.enum(["explicit_handle", "unverified"]),
}).strict();
const ConnectToolInputSchema = ConnectInputSchema.omit({
  principalToken: true,
})
  .extend({ host: ModelHostBindingSchema })
  .strict();
const EmitToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: EmitInputSchema,
  })
  .strict();
const AskToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: AskInputSchema,
  })
  .strict();
const InboxToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: InboxInputSchema,
  })
  .strict();
const AckToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: AckInputSchema,
  })
  .strict();
const DelegateToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: DelegateInputSchema.extend({ host: ModelHostBindingSchema }).strict(),
  })
  .strict();
const FinishToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    clientRequestId: ClientRequestIdSchema,
  })
  .strict();
const ForwardToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: ForwardInboxInputSchema,
  })
  .strict();

export interface CodlensMcpOptions {
  readonly agent: AgentTransportPort;
}

/**
 * Creates an MCP 2025-11-25 server. Generic MCP supplies tools and durable
 * pull; it does not claim unsolicited host wake or model consumption.
 */
export function createCodlensMcpServer(options: CodlensMcpOptions): McpServer {
  const server = new McpServer(
    { name: "codlens", version: "0.1.0" },
    {
      instructions:
        "lens/1 durable tools over MCP 2025-11-25. Questions return immediately; call codlens_inbox at a later safe point for answers.",
    },
  );

  server.registerTool(
    "codlens_connect",
    {
      description:
        "Create one broker actor using adapter-held credentials; returns no broker secret.",
      inputSchema: ConnectToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async (input) => toolResult(await options.agent.connect(input)),
  );

  server.registerTool(
    "codlens_emit",
    {
      description: "Persist one addressed lens/1 notice and return promptly.",
      inputSchema: EmitToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.emit(adapterSessionId, input)),
  );

  server.registerTool(
    "codlens_ask",
    {
      description:
        "Persist a human question and return its durable ID immediately; never waits for an answer.",
      inputSchema: AskToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.ask(adapterSessionId, input)),
  );

  server.registerTool(
    "codlens_inbox",
    {
      description: "Read one bounded durable inbox page without waiting or waking a model turn.",
      inputSchema: InboxToolInputSchema,
      annotations: { readOnlyHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.inbox(adapterSessionId, input)),
  );

  server.registerTool(
    "codlens_ack",
    {
      description: "Acknowledge explicit delivery IDs after actor consumption.",
      inputSchema: AckToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.ack(adapterSessionId, input)),
  );

  server.registerTool(
    "codlens_delegate",
    {
      description:
        "Create a capability-reduced child actor and adapter session; returns no broker credential.",
      inputSchema: DelegateToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.delegate(adapterSessionId, input)),
  );

  server.registerTool(
    "codlens_finish",
    {
      description:
        "Expire this exact actor after completion so stale bindings cannot consume later replies.",
      inputSchema: FinishToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, clientRequestId }) =>
      toolResult(await options.agent.finish(adapterSessionId, clientRequestId)),
  );

  server.registerTool(
    "codlens_forward",
    {
      description:
        "Forward pending child deliveries only along its declared forward_parent policy.",
      inputSchema: ForwardToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.forward(adapterSessionId, input)),
  );

  return server;
}

function toolResult<T>(result: Result<T>) {
  const value = result.ok
    ? { protocol: "lens/1", ok: true, value: result.value }
    : { protocol: "lens/1", ok: false, error: publicError(result.error) };
  return {
    content: [{ type: "text" as const, text: JSON.stringify(value) }],
    structuredContent: value,
    isError: !result.ok,
  };
}

function publicError(error: BrokerError): BrokerError {
  return error;
}
