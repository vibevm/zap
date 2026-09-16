import assert from "node:assert/strict";
import test from "node:test";
import {
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ConversationIdSchema } from "../protocol/index.ts";
import type { OwnedLineProcess } from "./claude.ts";
import { createQwenStreamJsonTransportFactory, type QwenProcessFactory } from "./qwen.ts";

test("Qwen owned stream transport uses selected model, CLI proxy, real output and correlation", async () => {
  const children: ScriptedQwen[] = [];
  const processFactory: QwenProcessFactory = {
    spawn(input) {
      const child = new ScriptedQwen(input.args, input.environment);
      children.push(child);
      return child;
    },
  };
  const opened = await createQwenStreamJsonTransportFactory({
    processFactory,
    proxyPolicy: { mode: "explicit", httpsProxy: "http://proxy.fixture:8080" },
    prepareLaunch: {
      prepare: () =>
        Promise.resolve({
          ok: true,
          value: {
            environment: {
              OPENAI_API_KEY: "synthetic-not-a-real-key",
              OPENAI_BASE_URL: "https://provider.fixture/v1",
            },
            mcpConfigPath: "C:/fixture/qwen-mcp-prepared.json",
          },
        }),
    },
  }).open({
    profileId: "profile.qwen.fixture",
    provider: "qwen_code",
    executablePath: "C:/fixture/qwen.cmd",
    cwd: "C:/fixture",
    modelId: "profile-model",
    effort: null,
    endpoint: null,
    argumentPrefix: ["C:/fixture/qwen-cli-entry.js"],
    environmentRef: "environment.qwen.fixture",
    mcpConfigPath: "C:/fixture/qwen-mcp-base.json",
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const events: unknown[] = [];
  opened.value.subscribe((event) => events.push(event));
  const started = await opened.value.start({
    scope: {
      coordinatorSessionId: AgentSessionIdSchema.parse("session.qwen.fixture"),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse("host.qwen.fixture"),
      profileId: "profile.qwen.fixture",
      modelId: "selected-model",
      reasoningEffort: null,
      cwd: "C:/fixture",
      bootstrapText: "bootstrap fixture",
      bootstrapBasis: "fixture",
    },
  });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  assert.equal(children[0]?.args.includes("selected-model"), true);
  assert.equal(children[0]?.args.includes("profile-model"), false);
  assert.equal(children[0]?.args[0], "C:/fixture/qwen-cli-entry.js");
  assert.deepEqual(
    children[0]?.args.slice(
      children[0]?.args.indexOf("--proxy"),
      children[0]?.args.indexOf("--proxy") + 2,
    ),
    ["--proxy", "http://proxy.fixture:8080"],
  );
  assert.deepEqual(
    children[0]?.args.slice(
      children[0]?.args.indexOf("--mcp-config"),
      children[0]?.args.indexOf("--mcp-config") + 2,
    ),
    ["--mcp-config", "C:/fixture/qwen-mcp-prepared.json"],
  );
  assert.deepEqual(
    children[0]?.args.slice(
      children[0]?.args.indexOf("--approval-mode"),
      children[0]?.args.indexOf("--approval-mode") + 2,
    ),
    ["--approval-mode", "default"],
  );
  assert.equal(children[0]?.args.includes("--allowed-mcp-server-names"), true);
  assert.equal(children[0]?.args.includes("mcp__zap-wayfinder__codlens_assigned_context"), true);
  assert.equal(children[0]?.args.includes("mcp__zap-wayfinder__codlens_inbox_wait"), true);
  assert.equal(children[0]?.args.includes("mcp__zap-wayfinder__codlens_plan_apply"), false);
  assert.equal(children[0]?.environment["OPENAI_API_KEY"], "synthetic-not-a-real-key");
  const sent = await opened.value.send({
    session: started.value,
    text: "second fixture turn",
    clientMessageId: "message.qwen.fixture",
  });
  assert.equal(sent.ok, true);
  assert.equal(sent.ok ? sent.value.nativeTurnId : "failed", null);
  assert.equal(
    sent.ok ? sent.value.transportCorrelation?.clientMessageId : "failed",
    "message.qwen.fixture",
  );
  assert.equal(JSON.stringify(events).includes("visible fixture output"), true);
  assert.equal(JSON.stringify(events).includes("secret-thinking"), false);
  opened.value.close();
});

class ScriptedQwen implements OwnedLineProcess {
  readonly pid = 9101;
  readonly writes: string[] = [];
  readonly args: readonly string[];
  readonly environment: Readonly<Record<string, string>>;
  #lines = new Set<(line: string) => void>();
  #exits = new Set<(code: number | null) => void>();

  constructor(args: readonly string[], environment: Readonly<Record<string, string>>) {
    this.args = args;
    this.environment = environment;
  }

  write(input: string): void {
    this.writes.push(input);
    if (this.writes.length !== 1) return;
    setImmediate(() => {
      for (const listener of this.#lines) {
        listener(
          JSON.stringify({
            type: "assistant",
            session_id: "qwen-session.fixture",
            message: {
              role: "assistant",
              content: [
                { type: "thinking", thinking: "secret-thinking" },
                { type: "text", text: "visible fixture output" },
              ],
            },
          }),
        );
      }
    });
  }

  onLine(listener: (line: string) => void): () => void {
    this.#lines.add(listener);
    return () => this.#lines.delete(listener);
  }

  onExit(listener: (code: number | null) => void): () => void {
    this.#exits.add(listener);
    return () => this.#exits.delete(listener);
  }

  kill(): void {
    for (const listener of this.#exits) listener(0);
  }
}
