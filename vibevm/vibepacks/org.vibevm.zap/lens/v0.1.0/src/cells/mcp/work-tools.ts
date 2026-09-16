/** Typed managed and native work MCP tools. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  ManagedAgentAttachmentAckInputSchema,
  ManagedAgentCreateInputSchema,
  ManagedAgentReadInputSchema,
  ManagedAgentReportInputSchema,
  ManagedAgentStartInputSchema,
  NativeWorkAttachmentAckInputSchema,
  NativeWorkBeforeInputSchema,
  NativeWorkReadInputSchema,
  RepositoryIntegrationDiffAgentInputSchema,
  RepositoryIntegrationGetAgentInputSchema,
  RepositoryIntegrationListAgentInputSchema,
  RepositoryIntegrationPrepareAgentInputSchema,
  RepositoryIntegrationTestAgentInputSchema,
  RepositoryPlanListAgentInputSchema,
  RepositoryWorktreeGetAgentInputSchema,
  RepositoryWorktreeListAgentInputSchema,
  type ManagedWorkAgentPort,
  type NativeWorkAgentPort,
  type RepositoryWorkspaceAgentPort,
} from "../managed-work/index.ts";
import type { Result } from "../protocol/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";

const sessionInput = <T extends z.ZodType>(input: T) =>
  z.object({ adapterSessionId: AdapterSessionIdSchema, input }).strict();
const ManagedProfilesToolInputSchema = z
  .object({ adapterSessionId: AdapterSessionIdSchema })
  .strict();
const ManagedCreateToolInputSchema = sessionInput(ManagedAgentCreateInputSchema);
const ManagedStartToolInputSchema = sessionInput(ManagedAgentStartInputSchema);
const ManagedReadToolInputSchema = sessionInput(ManagedAgentReadInputSchema);
const ManagedReportToolInputSchema = sessionInput(ManagedAgentReportInputSchema);
const ManagedAckToolInputSchema = sessionInput(ManagedAgentAttachmentAckInputSchema);
const NativeBeforeToolInputSchema = sessionInput(NativeWorkBeforeInputSchema);
const NativeReadToolInputSchema = sessionInput(NativeWorkReadInputSchema);
const NativeAckToolInputSchema = sessionInput(NativeWorkAttachmentAckInputSchema);
const RepositoryPlanListToolInputSchema = sessionInput(RepositoryPlanListAgentInputSchema);
const RepositoryWorktreeListToolInputSchema = sessionInput(RepositoryWorktreeListAgentInputSchema);
const RepositoryWorktreeGetToolInputSchema = sessionInput(RepositoryWorktreeGetAgentInputSchema);
const RepositoryIntegrationListToolInputSchema = sessionInput(
  RepositoryIntegrationListAgentInputSchema,
);
const RepositoryIntegrationGetToolInputSchema = sessionInput(
  RepositoryIntegrationGetAgentInputSchema,
);
const RepositoryIntegrationDiffToolInputSchema = sessionInput(
  RepositoryIntegrationDiffAgentInputSchema,
);
const RepositoryIntegrationPrepareToolInputSchema = sessionInput(
  RepositoryIntegrationPrepareAgentInputSchema,
);
const RepositoryIntegrationTestToolInputSchema = sessionInput(
  RepositoryIntegrationTestAgentInputSchema,
);

export function registerWorkTools(
  server: McpServer,
  options: {
    readonly managedWork?: ManagedWorkAgentPort;
    readonly nativeWork?: NativeWorkAgentPort;
    readonly repositoryWork?: RepositoryWorkspaceAgentPort;
  },
): void {
  const managed = options.managedWork;
  if (managed !== undefined) registerManagedTools(server, managed);
  const native = options.nativeWork;
  if (native !== undefined) registerNativeTools(server, native);
  const repository = options.repositoryWork;
  if (repository !== undefined) registerRepositoryTools(server, repository);
}

function registerRepositoryTools(
  server: McpServer,
  repository: RepositoryWorkspaceAgentPort,
): void {
  const readOnly = { readOnlyHint: true };
  server.registerTool(
    "codlens_repository_plan_list",
    {
      description: "List plans in this exact coordinator project.",
      inputSchema: RepositoryPlanListToolInputSchema,
      annotations: readOnly,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.planList(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_worktree_list",
    {
      description: "List worktrees for one plan in this coordinator context.",
      inputSchema: RepositoryWorktreeListToolInputSchema,
      annotations: readOnly,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.worktreeList(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_worktree_get",
    {
      description: "Read one worktree in this coordinator context.",
      inputSchema: RepositoryWorktreeGetToolInputSchema,
      annotations: readOnly,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.worktreeGet(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_integration_list",
    {
      description: "List integrations for one plan in this coordinator context.",
      inputSchema: RepositoryIntegrationListToolInputSchema,
      annotations: readOnly,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.integrationList(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_integration_get",
    {
      description: "Read one integration in this coordinator context.",
      inputSchema: RepositoryIntegrationGetToolInputSchema,
      annotations: readOnly,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.integrationGet(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_integration_diff",
    {
      description: "Read a bounded untrusted-text diff for one exact integration candidate.",
      inputSchema: RepositoryIntegrationDiffToolInputSchema,
      annotations: readOnly,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.integrationDiff(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_integration_prepare",
    {
      description: "Prepare an integration from exact source and target commits.",
      inputSchema: RepositoryIntegrationPrepareToolInputSchema,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.integrationPrepare(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_repository_integration_test",
    {
      description: "Run one registered test profile for an exact integration revision.",
      inputSchema: RepositoryIntegrationTestToolInputSchema,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await repository.integrationTest(adapterSessionId, input)),
  );
}

function registerManagedTools(server: McpServer, managed: ManagedWorkAgentPort): void {
  server.registerTool(
    "codlens_managed_work_profiles",
    {
      description: "List trusted managed profiles for this assigned actor project and context.",
      inputSchema: ManagedProfilesToolInputSchema,
      annotations: { readOnlyHint: true },
    },
    async ({ adapterSessionId }) => toolResult(await managed.profiles(adapterSessionId)),
  );
  server.registerTool(
    "codlens_managed_work_create",
    {
      description: "Create child work using explicit targets and server-derived scope and parent.",
      inputSchema: ManagedCreateToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await managed.create(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_managed_work_start",
    { description: "Start an exact prepared child run.", inputSchema: ManagedStartToolInputSchema },
    async ({ adapterSessionId, input }) => toolResult(await managed.start(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_managed_work_read",
    {
      description: "Read one own or directly supervised managed run.",
      inputSchema: ManagedReadToolInputSchema,
      annotations: { readOnlyHint: true },
    },
    async ({ adapterSessionId, input }) => toolResult(await managed.read(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_managed_work_report",
    {
      description: "Submit this assigned worker run's typed report.",
      inputSchema: ManagedReportToolInputSchema,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await managed.report(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_managed_work_attachment_ack",
    {
      description: "Acknowledge an exact attachment/version for this assigned attempt.",
      inputSchema: ManagedAckToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await managed.acknowledgeAttachment(adapterSessionId, input)),
  );
}

function registerNativeTools(server: McpServer, native: NativeWorkAgentPort): void {
  server.registerTool(
    "codlens_native_work_before",
    {
      description:
        "Prepare deferred instructions for an exact native attempt and explicit targetRefs. Prompt text is never a target.",
      inputSchema: NativeBeforeToolInputSchema,
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await native.beforeWork(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_native_work_read",
    {
      description: "Read the retained before-work receipt for an exact native attempt.",
      inputSchema: NativeReadToolInputSchema,
      annotations: { readOnlyHint: true },
    },
    async ({ adapterSessionId, input }) => toolResult(await native.read(adapterSessionId, input)),
  );
  server.registerTool(
    "codlens_native_work_attachment_ack",
    {
      description: "Acknowledge an exact attachment/version offered to this native attempt.",
      inputSchema: NativeAckToolInputSchema,
      annotations: { idempotentHint: true },
    },
    async ({ adapterSessionId, input }) =>
      toolResult(await native.acknowledgeAttachment(adapterSessionId, input)),
  );
}

function toolResult<T>(result: Result<T>) {
  const value = result.ok
    ? { protocol: "lens/1", ok: true, value: result.value }
    : { protocol: "lens/1", ok: false, error: result.error };
  return {
    content: [{ type: "text" as const, text: JSON.stringify(value) }],
    structuredContent: value,
    isError: !result.ok,
  };
}
