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
  type ManagedWorkAgentPort,
  type NativeWorkAgentPort,
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

export function registerWorkTools(
  server: McpServer,
  options: {
    readonly managedWork?: ManagedWorkAgentPort;
    readonly nativeWork?: NativeWorkAgentPort;
  },
): void {
  const managed = options.managedWork;
  if (managed !== undefined) registerManagedTools(server, managed);
  const native = options.nativeWork;
  if (native !== undefined) registerNativeTools(server, native);
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
