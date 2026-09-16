import assert from "node:assert/strict";
import test from "node:test";
import {
  createClaudeStreamJsonTransportFactory,
  type ClaudeProcessFactory,
  type OwnedLineProcess,
} from "./claude.ts";
import { createProviderCoordinatorAdapter, type ProviderCoordinatorProfile } from "./index.ts";
import {
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ConversationIdSchema } from "../protocol/index.ts";

test("Claude stream transport owns a scripted child, decodes init, and fences lifecycle", async () => {
  const children: ScriptedChild[] = [];
  const processFactory: ClaudeProcessFactory = {
    spawn(input) {
      const child = new ScriptedChild(input.args, input.environment, input.cwd);
      children.push(child);
      return child;
    },
  };
  const profile: ProviderCoordinatorProfile = {
    profileId: "profile.claude.fixture",
    provider: "claude_code",
    executablePath: "C:/fixture/claude.exe",
    cwd: "C:/fixture",
    modelId: "fixture-small",
    effort: "low",
    endpoint: null,
    argumentPrefix: ["C:/fixture/claude-entry.js"],
    environmentRef: "environment.claude.fixture",
    mcpConfigPath: "C:/fixture/claude-mcp-base.json",
  };
  const opened = await createClaudeStreamJsonTransportFactory({
    processFactory,
    prepareLaunch: {
      prepare: () =>
        Promise.resolve({
          ok: true,
          value: {
            environment: { SYNTHETIC_CLAUDE_AUTH: "available" },
            mcpConfigPath: "C:/fixture/claude-mcp-prepared.json",
          },
        }),
    },
  }).open(profile);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const adapterResult = createProviderCoordinatorAdapter({
    profile,
    hostId: "host.claude.fixture",
    transport: opened.value,
  });
  assert.equal(adapterResult.ok, true);
  if (!adapterResult.ok) return;
  const started = await adapterResult.value.start({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    projectId: ProjectIdSchema.parse("project.fixture"),
    contextId: WorkContextIdSchema.parse("context.fixture"),
    conversationId: ConversationIdSchema.parse("conversation.fixture"),
    coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
    hostId: ExecutionHostIdSchema.parse("host.claude.fixture"),
    profileId: profile.profileId,
    cwd: profile.cwd,
    bootstrapText: "fixture bootstrap",
    bootstrapBasis: "fixture",
  });
  assert.equal(started.ok, true);
  assert.deepEqual(children[0]?.args.slice(0, 8), [
    "C:/fixture/claude-entry.js",
    "--print",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--model",
  ]);
  assert.deepEqual(
    children[0]?.args.slice(
      children[0]?.args.indexOf("--mcp-config"),
      children[0]?.args.indexOf("--mcp-config") + 2,
    ),
    ["--mcp-config", "C:/fixture/claude-mcp-prepared.json"],
  );
  assert.match(
    children[0]?.args[children[0]?.args.indexOf("--allowedTools") + 1] ?? "",
    /mcp__zap-wayfinder__codlens_assigned_context/,
  );
  assert.doesNotMatch(
    children[0]?.args[children[0]?.args.indexOf("--allowedTools") + 1] ?? "",
    /codlens_plan_apply/,
  );
  assert.equal(children[0]?.environment["SYNTHETIC_CLAUDE_AUTH"], "available");
  assert.equal(children[0]?.cwd, "C:/fixture");
  assert.equal(children[0]?.writes.length, 1);
  const sent = await adapterResult.value.send({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    text: "fixture turn",
    clientMessageId: "message.claude.fixture",
  });
  assert.equal(sent.ok, true);
  assert.equal(sent.ok ? sent.value.nativeTurnId : "failed", null);
  assert.equal(
    sent.ok ? sent.value.transportCorrelation?.clientMessageId : "failed",
    "message.claude.fixture",
  );
  const stopped = await adapterResult.value.stop?.({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    expectedProcessEpoch: started.ok ? started.value.processEpoch : "unavailable",
  });
  assert.equal(stopped?.ok, true);
  assert.equal(children[0]?.killed, true);
  adapterResult.value.close();
});

class ScriptedChild implements OwnedLineProcess {
  readonly pid = 9001;
  readonly args: readonly string[];
  readonly environment: Readonly<Record<string, string>>;
  readonly cwd: string;
  readonly writes: string[] = [];
  killed = false;
  #lines = new Set<(line: string) => void>();
  #exits = new Set<(code: number | null) => void>();

  constructor(args: readonly string[], environment: Readonly<Record<string, string>>, cwd: string) {
    this.args = args;
    this.environment = environment;
    this.cwd = cwd;
  }

  write(input: string): void {
    this.writes.push(input);
    if (this.writes.length === 1)
      setImmediate(() => {
        for (const listener of this.#lines)
          listener(
            JSON.stringify({
              type: "system",
              subtype: "init",
              session_id: "claude-session.fixture",
            }),
          );
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
    this.killed = true;
    for (const listener of this.#exits) listener(0);
  }
}
