/** Local MCP fixture for the runnable ZapMockAgent process test. */
import { appendFileSync } from "node:fs";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const server = new McpServer({ name: "zap-mock-process-fixture", version: "1" });
const SessionSchema = z.object({ adapterSessionId: z.string() }).loose();
const logPath = process.env["CODLENS_CREDENTIAL_FILE"];

server.registerTool(
  "codlens_assigned_context",
  { inputSchema: z.object({}).strict() },
  async () => {
    await Promise.resolve();
    return result({
      adapterSessionId: "adapter.mock.fixture",
      connection: { actorId: "actor.mock.fixture" },
    });
  },
);
server.registerTool(
  "codlens_ask_user_question",
  { inputSchema: z.object({ adapterSessionId: z.string() }).loose() },
  async (input) => {
    await Promise.resolve();
    record("ask", input);
    return result({ questionGroupId: "question-group.mock.fixture" });
  },
);
server.registerTool("codlens_inbox", { inputSchema: SessionSchema }, async (input) => {
  await Promise.resolve();
  record("inbox", input);
  return result({
    deliveries: [
      {
        deliveryId: "delivery.mock.answer",
        message: {
          protocol: "lens/1",
          messageId: "message.mock.answer",
          workspaceId: "workspace.mock.fixture",
          conversationId: "conversation.mock.fixture",
          fromActorId: null,
          toActorId: "actor.mock.fixture",
          kind: "question.answered",
          correlationId: "question-group.mock.fixture",
          causationId: null,
          sequence: "1",
          payload: { answerVersionId: "answer-version.mock.fixture", choice: "first" },
          createdAt: "2026-09-16T00:00:00.000Z",
        },
        logicalRecipientActorId: "actor.mock.fixture",
        recipientActorId: "actor.mock.fixture",
        forwardedFromDeliveryId: null,
        acknowledgedAt: null,
        observations: ["persisted"],
      },
    ],
    observationCursor: "1",
    hasMore: false,
  });
});
server.registerTool("codlens_ack", { inputSchema: SessionSchema }, async (input) => {
  await Promise.resolve();
  record("ack", input);
  return result({ acknowledgedDeliveryIds: ["delivery.mock.answer"] });
});
server.registerTool("codlens_managed_work_read", { inputSchema: SessionSchema }, async (input) => {
  await Promise.resolve();
  record("read", input);
  return result({ revision: "7" });
});
server.registerTool(
  "codlens_managed_work_report",
  { inputSchema: SessionSchema },
  async (input) => {
    await Promise.resolve();
    record("report", input);
    return result({ state: "reported" });
  },
);

await server.connect(new StdioServerTransport());

function result(value: unknown) {
  const structuredContent = { protocol: "lens/1", ok: true, value };
  return {
    content: [{ type: "text" as const, text: JSON.stringify(structuredContent) }],
    structuredContent,
  };
}

function record(kind: string, value: unknown): void {
  if (logPath !== undefined)
    appendFileSync(logPath, `${JSON.stringify({ kind, value })}\n`, "utf8");
}
