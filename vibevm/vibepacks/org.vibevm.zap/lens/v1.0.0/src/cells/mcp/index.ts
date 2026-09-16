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
  WaitInboxInputSchema,
  type BrokerError,
  type JsonValue,
  type PublicConnection,
  type Result,
} from "../protocol/index.ts";
import {
  AdapterSessionIdSchema,
  type AdapterSessionId,
  type AgentTransportPort,
} from "../transport/index.ts";
import { AgentQuestionInputSchema } from "../workspace-interaction/index.ts";
import type {
  ManagedWorkAgentPort,
  NativeWorkAgentPort,
  RepositoryWorkspaceAgentPort,
} from "../managed-work/index.ts";
import { registerWorkTools } from "./work-tools.ts";

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
const RichQuestionToolInputSchema = AgentQuestionInputSchema.extend({
  adapterSessionId: AdapterSessionIdSchema,
}).strict();
const InboxToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: InboxInputSchema,
  })
  .strict();
const WaitInboxToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    input: WaitInboxInputSchema,
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
const ContextToolInputSchema = z.object({ adapterSessionId: AdapterSessionIdSchema }).strict();
const AssignedContextToolInputSchema = z.object({}).strict();
const PlanProposalToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    proposal: z.record(z.string(), z.unknown()),
  })
  .strict();
const PlanWorkflowToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    request: z.record(z.string(), z.unknown()),
  })
  .strict();
const PlanPreparationToolInputSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    kind: z.enum(["bundle", "comparison", "projected_record"]),
    request: z.record(z.string(), z.unknown()),
  })
  .strict();
export type {
  ManagedWorkAgentPort,
  NativeWorkAgentPort,
  RepositoryWorkspaceAgentPort,
} from "../managed-work/index.ts";

export interface AgentPlanProposalPort {
  register(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
  submit(
    actor: PublicConnection,
    session: AdapterSessionId,
    proposal: unknown,
  ): Promise<Result<JsonValue>>;
  preview(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
  apply(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
  reconcile(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
  prepare(
    actor: PublicConnection,
    session: AdapterSessionId,
    kind: "bundle" | "comparison" | "projected_record",
    input: unknown,
  ): Promise<Result<JsonValue>>;
  discover(actor: PublicConnection, session: AdapterSessionId): Promise<Result<JsonValue>>;
  author(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
  authorComposite(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
  prepareComposite(
    actor: PublicConnection,
    session: AdapterSessionId,
    input: unknown,
  ): Promise<Result<JsonValue>>;
}

export interface CodlensMcpOptions {
  readonly agent: AgentTransportPort;
  readonly assignedSession?: AdapterSessionId;
  readonly planProposal?: AgentPlanProposalPort;
  readonly managedWork?: ManagedWorkAgentPort;
  readonly nativeWork?: NativeWorkAgentPort;
  readonly repositoryWork?: RepositoryWorkspaceAgentPort;
}

/**
 * Creates an MCP 2025-11-25 server. Generic MCP supplies tools and durable
 * pull; it does not claim unsolicited host wake or model consumption.
 */
export function createCodlensMcpServer(options: CodlensMcpOptions): McpServer {
  const server = new McpServer(
    { name: "codlens", version: "1.0.0" },
    {
      instructions:
        "lens/1 durable tools over MCP 2025-11-25. Publish user clarification through /ZapAskUserQuestion (codlens_ask_user_question), then post a short ordinary-text notice. The tool returns immediately; call codlens_inbox only at a later safe point.",
    },
  );

  if (options.assignedSession !== undefined) {
    const assignedSession = options.assignedSession;
    server.registerTool(
      "codlens_assigned_context",
      {
        description:
          "Discover this process's server-assigned adapter session and exact actor scope without exposing credentials.",
        inputSchema: AssignedContextToolInputSchema,
        annotations: { readOnlyHint: true },
      },
      async () => {
        const context = await options.agent.context(assignedSession);
        return toolResult(
          context.ok
            ? {
                ok: true,
                value: { adapterSessionId: assignedSession, connection: context.value },
              }
            : context,
        );
      },
    );
  }

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
    "codlens_context",
    {
      description:
        "Read this authenticated actor handle, workspace, conversation and capabilities without exposing broker credentials.",
      inputSchema: ContextToolInputSchema,
      annotations: { readOnlyHint: true },
    },
    async ({ adapterSessionId }) => toolResult(await options.agent.context(adapterSessionId)),
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

  const planProposal = options.planProposal;
  if (planProposal !== undefined) {
    server.registerTool(
      "codlens_plan_discover",
      {
        description:
          "Read the exact current ZAP, specification, policy and baseline authoring context.",
        inputSchema: ContextToolInputSchema,
        annotations: { readOnlyHint: true },
      },
      async ({ adapterSessionId }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok ? await planProposal.discover(actor.value, adapterSessionId) : actor,
        );
      },
    );
    server.registerTool(
      "codlens_plan_author",
      {
        description:
          "Submit typed successor-plan metadata through data authority and return one immutable prepared successor for ordered execution.",
        inputSchema: PlanWorkflowToolInputSchema,
      },
      async ({ adapterSessionId, request }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok ? await planProposal.author(actor.value, adapterSessionId, request) : actor,
        );
      },
    );
    server.registerTool(
      "codlens_plan_prepare_composite",
      {
        description:
          "Prepare and durably bind milestone create/revise identities before authoring successor content that references them.",
        inputSchema: PlanWorkflowToolInputSchema,
        annotations: { idempotentHint: true },
      },
      async ({ adapterSessionId, request }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok
            ? await planProposal.prepareComposite(actor.value, adapterSessionId, request)
            : actor,
        );
      },
    );
    server.registerTool(
      "codlens_plan_author_composite",
      {
        description:
          "Durably prepare and record milestone create/revise precursors plus one successor plan through the shared Wayfinder journal.",
        inputSchema: PlanWorkflowToolInputSchema,
      },
      async ({ adapterSessionId, request }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok
            ? await planProposal.authorComposite(actor.value, adapterSessionId, request)
            : actor,
        );
      },
    );
    server.registerTool(
      "codlens_plan_prepare",
      {
        description:
          "Call a public ZAP preparation route and return backend-derived bases, affected scope and preflight data without applying a mutation.",
        inputSchema: PlanPreparationToolInputSchema,
        annotations: { readOnlyHint: true },
      },
      async ({ adapterSessionId, kind, request }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok
            ? await planProposal.prepare(actor.value, adapterSessionId, kind, request)
            : actor,
        );
      },
    );
    server.registerTool(
      "codlens_plan_intent",
      {
        description:
          "Register an explicit plan intent for this authenticated root actor when ordinary chat begins the workflow without a GUI request.",
        inputSchema: PlanWorkflowToolInputSchema,
      },
      async ({ adapterSessionId, request }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok ? await planProposal.register(actor.value, adapterSessionId, request) : actor,
        );
      },
    );
    server.registerTool(
      "codlens_plan_proposal",
      {
        description:
          "Bind a durably authored operation to the exact queued Quicklens intent; pass preparedOperationId, never copy authority-bearing prepared bytes through model text.",
        inputSchema: PlanProposalToolInputSchema,
      },
      async ({ adapterSessionId, proposal }) => {
        const actor = await options.agent.context(adapterSessionId);
        return toolResult(
          actor.ok ? await planProposal.submit(actor.value, adapterSessionId, proposal) : actor,
        );
      },
    );
    for (const [name, description, call] of [
      [
        "codlens_plan_preview",
        "Read a prepared proposal or report that the correlated agent proposal is still pending.",
        planProposal.preview.bind(planProposal),
      ],
      [
        "codlens_plan_apply",
        "Advance and execute an exact prepared plan through configured Coordinator authority only.",
        planProposal.apply.bind(planProposal),
      ],
      [
        "codlens_plan_reconcile",
        "Reconcile an exact uncertain plan operation without selecting Owner credentials.",
        planProposal.reconcile.bind(planProposal),
      ],
    ] as const) {
      server.registerTool(
        name,
        { description, inputSchema: PlanWorkflowToolInputSchema },
        async ({ adapterSessionId, request }) => {
          const actor = await options.agent.context(adapterSessionId);
          return toolResult(actor.ok ? await call(actor.value, adapterSessionId, request) : actor);
        },
      );
    }
  }

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

  if (options.agent.askUserQuestion !== undefined) {
    const publishRichQuestion = options.agent.askUserQuestion.bind(options.agent);
    server.registerTool(
      "codlens_ask_user_question",
      {
        description:
          "/ZapAskUserQuestion: persist one structured question group for the authenticated actor, return its durable ID immediately, then tell the user in ordinary text that the question was sent.",
        inputSchema: RichQuestionToolInputSchema,
        annotations: { idempotentHint: true },
      },
      async ({ adapterSessionId, clientRequestId, draft }) =>
        toolResult(
          await publishRichQuestion(adapterSessionId, {
            clientRequestId,
            draft,
          }),
        ),
    );
  }

  registerWorkTools(server, options);

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
    "codlens_inbox_wait",
    {
      description:
        "Wait up to 30 seconds for this idle authenticated actor inbox. Returns without acknowledging, approving, or injecting terminal input.",
      inputSchema: WaitInboxToolInputSchema,
      annotations: { readOnlyHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await options.agent.waitInbox(adapterSessionId, input)),
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
