/** Synthetic worker MCP client over its generated local Zap server. @scope spec://org.vibevm.zap/lens/PROP-013#agent */
import { createHash } from "node:crypto";
import { isAbsolute } from "node:path";
import { readFileSync } from "node:fs";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { z } from "zod";
import { InboxPageSchema } from "../protocol/index.ts";
import type { ZapMockEffect } from "../mock-model/index.ts";

const ServerSchema = z
  .object({
    command: z.string().min(1).refine(isAbsolute),
    args: z.array(z.string().max(32_000)).max(64),
    env: z.record(z.string(), z.string()),
  })
  .strict();
const ConfigSchema = z
  .object({ mcpServers: z.object({ "zap-wayfinder": ServerSchema }).strict() })
  .strict();
const EnvelopeSchema = z.discriminatedUnion("ok", [
  z.object({ protocol: z.literal("lens/1"), ok: z.literal(true), value: z.unknown() }).strict(),
  z
    .object({
      protocol: z.literal("lens/1"),
      ok: z.literal(false),
      error: z.object({ code: z.string(), message: z.string() }).loose(),
    })
    .strict(),
]);
const AssignedSchema = z
  .object({ adapterSessionId: z.string().min(3), connection: z.unknown() })
  .strict();
const QuestionSchema = z.looseObject({ questionGroupId: z.string().min(3) });

export type ZapMockMcpResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly message: string };

export interface ZapMockMcpSession {
  readonly adapterSessionId: string;
  ask(effect: Extract<ZapMockEffect, { kind: "question" }>): Promise<ZapMockMcpResult<string>>;
  inbox(afterSequence: string): Promise<ZapMockMcpResult<z.infer<typeof InboxPageSchema>>>;
  acknowledge(deliveryIds: readonly string[]): Promise<ZapMockMcpResult<null>>;
  readRun(runId: string): Promise<ZapMockMcpResult<{ readonly revision: string }>>;
  report(input: {
    readonly runId: string;
    readonly expectedRevision: string;
    readonly summaryMarkdown: string;
  }): Promise<ZapMockMcpResult<null>>;
  close(): Promise<void>;
}

export async function openZapMockMcpSession(
  configPath: string,
): Promise<ZapMockMcpResult<ZapMockMcpSession>> {
  let raw: unknown;
  try {
    raw = JSON.parse(readFileSync(configPath, "utf8"));
  } catch {
    return failure("generated Zap MCP configuration is unreadable");
  }
  const config = ConfigSchema.safeParse(raw);
  if (!config.success) return failure("generated Zap MCP configuration is invalid");
  const server = config.data.mcpServers["zap-wayfinder"];
  if (!localZapEnvironment(server.env))
    return failure("generated Zap MCP configuration is outside the local synthetic boundary");
  const client = new Client({ name: "zap-mock-agent", version: "0.1.0" });
  try {
    await client.connect(
      new StdioClientTransport({
        command: server.command,
        args: [...server.args],
        env: server.env,
        cwd: process.cwd(),
        stderr: "pipe",
      }),
    );
    const assigned = await call(client, "codlens_assigned_context", {});
    if (!assigned.ok) {
      await client.close();
      return assigned;
    }
    const context = AssignedSchema.safeParse(assigned.value);
    if (!context.success) {
      await client.close();
      return failure("assigned Zap MCP context is invalid");
    }
    const adapterSessionId = context.data.adapterSessionId;
    return {
      ok: true,
      value: {
        adapterSessionId,
        async ask(effect) {
          const key = digest(effect.questionId);
          const options = effect.options.map((label, index) => ({
            optionId: `option.mock.${key}.${String(index + 1)}`,
            label,
            description: `Synthetic option ${String(index + 1)}`,
            previewMarkdown: null,
            artifactRefs: [],
          }));
          const asked = await call(client, "codlens_ask_user_question", {
            adapterSessionId,
            clientRequestId: `request.mock-question.${key}`,
            draft: {
              title: "ZapMockAgent question",
              introductionMarkdown:
                "Deterministic synthetic question; no model inference was used.",
              items: [
                {
                  questionItemId: `question-item.mock.${key}`,
                  header: "Mock decision",
                  promptMarkdown: effect.prompt,
                  contextMarkdown: null,
                  artifactRefs: [],
                  required: true,
                  answerMode: "single_choice",
                  options,
                  customAnswer: {
                    allowed: true,
                    label: "Other",
                    multiline: false,
                    maximumLength: 4_000,
                  },
                  recommendation: null,
                },
              ],
              independentWorkAvailable: true,
              deadlineAt: null,
            },
          });
          if (!asked.ok) return asked;
          const question = QuestionSchema.safeParse(asked.value);
          return question.success
            ? { ok: true, value: question.data.questionGroupId }
            : failure("Zap MCP question response is invalid");
        },
        async inbox(afterSequence) {
          const received = await call(client, "codlens_inbox", {
            adapterSessionId,
            input: { afterSequence, limit: 50 },
          });
          if (!received.ok) return received;
          const page = InboxPageSchema.safeParse(received.value);
          return page.success
            ? { ok: true, value: page.data }
            : failure("Zap MCP inbox response is invalid");
        },
        async acknowledge(deliveryIds) {
          const acknowledged = await call(client, "codlens_ack", {
            adapterSessionId,
            input: {
              clientRequestId: `request.mock-ack.${digest([...deliveryIds].join("\u0000"))}`,
              deliveryIds: [...deliveryIds],
            },
          });
          return acknowledged.ok ? { ok: true, value: null } : acknowledged;
        },
        async readRun(runId) {
          const read = await call(client, "codlens_managed_work_read", {
            adapterSessionId,
            input: { runId },
          });
          if (!read.ok) return read;
          const claim = z
            .looseObject({ revision: z.string().regex(/^(0|[1-9][0-9]*)$/) })
            .safeParse(read.value);
          return claim.success
            ? { ok: true, value: claim.data }
            : failure("managed run response is invalid");
        },
        async report(input) {
          const reported = await call(client, "codlens_managed_work_report", {
            adapterSessionId,
            input: { ...input, artifactRefs: [] },
          });
          return reported.ok ? { ok: true, value: null } : reported;
        },
        close: () => client.close(),
      },
    };
  } catch {
    await client.close().catch(() => undefined);
    return failure("local Zap MCP process could not be opened");
  }
}

async function call(
  client: Client,
  name: string,
  args: Record<string, unknown>,
): Promise<ZapMockMcpResult<unknown>> {
  try {
    const result = await client.callTool({ name, arguments: args });
    const parsed = EnvelopeSchema.safeParse(result.structuredContent);
    if (!parsed.success) return failure(`Zap MCP ${name} returned invalid structured content`);
    return parsed.data.ok
      ? { ok: true, value: parsed.data.value }
      : failure(`Zap MCP ${name} refused the synthetic request: ${parsed.data.error.code}`);
  } catch {
    return failure(`Zap MCP ${name} transport failed`);
  }
}

function localZapEnvironment(environment: Readonly<Record<string, string>>): boolean {
  const allowed = new Set([
    "CODLENS_URL",
    "CODLENS_CREDENTIAL_FILE",
    "CODLENS_ADAPTER_SESSION_ID",
    "CODLENS_WORKSPACE_ID",
    "CODLENS_CONVERSATION_ID",
  ]);
  if (Object.keys(environment).some((name) => !allowed.has(name))) return false;
  const raw = environment["CODLENS_URL"];
  if (raw === undefined) return false;
  try {
    const url = new URL(raw);
    return (
      url.protocol === "http:" &&
      (url.hostname === "127.0.0.1" || url.hostname === "localhost" || url.hostname === "[::1]")
    );
  } catch {
    return false;
  }
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
}

function failure(message: string): ZapMockMcpResult<never> {
  return { ok: false, message };
}
