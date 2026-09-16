import assert from "node:assert/strict";
import test from "node:test";
import {
  createClaudeStreamJsonTransportFactory,
  type ClaudeProcessFactory,
  type OwnedLineProcess,
} from "./claude.ts";
import type { ProviderProcessDiagnostic } from "./process-diagnostic.ts";
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
    accountBindingId: "binding.claude.fixture",
    executionHostId: ExecutionHostIdSchema.parse("host.claude.fixture"),
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
            environment: {
              SYNTHETIC_CLAUDE_AUTH: "available",
              CLAUDE_CONFIG_DIR: "C:/fixture/claude-account",
            },
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
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(children[0]?.args.slice(0, 10), [
    "C:/fixture/claude-entry.js",
    "--print",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--setting-sources",
    "",
    "--model",
  ]);
  assert.deepEqual(
    children[0]?.args.slice(
      children[0]?.args.indexOf("--mcp-config"),
      children[0]?.args.indexOf("--mcp-config") + 2,
    ),
    ["--mcp-config", "C:/fixture/claude-mcp-prepared.json"],
  );
  assert.deepEqual(
    children[0]?.args.slice(
      children[0]?.args.indexOf("--setting-sources"),
      children[0]?.args.indexOf("--setting-sources") + 2,
    ),
    ["--setting-sources", ""],
  );
  assert.equal(children[0]?.args.includes("--strict-mcp-config"), true);
  assert.match(
    children[0]?.args[children[0]?.args.indexOf("--allowedTools") + 1] ?? "",
    /mcp__zap-wayfinder__codlens_assigned_context/,
  );
  assert.doesNotMatch(
    children[0]?.args[children[0]?.args.indexOf("--allowedTools") + 1] ?? "",
    /codlens_plan_apply/,
  );
  assert.equal(children[0]?.environment["SYNTHETIC_CLAUDE_AUTH"], "available");
  assert.equal(children[0]?.environment["CLAUDE_CONFIG_DIR"], "C:/fixture/claude-account");
  assert.equal(children[0]?.cwd, "C:/fixture");
  assert.equal(children[0]?.writes.length, 2);
  const events: Array<{ readonly kind?: string; readonly data?: unknown }> = [];
  adapterResult.value.subscribe((event) => events.push(event));
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
  children[0]?.emit({
    type: "stream_event",
    parent_tool_use_id: null,
    event: { type: "message_start" },
  });
  children[0]?.emit({
    type: "stream_event",
    parent_tool_use_id: null,
    event: { type: "message_stop" },
  });
  const paused = await adapterResult.value.pause?.({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    expectedProcessEpoch: started.ok ? started.value.processEpoch : "unavailable",
  });
  assert.equal(paused?.ok && paused.value.observation, "requested");
  children[0]?.emit({
    type: "stream_event",
    parent_tool_use_id: "tool.child.fixture",
    event: { type: "message_start" },
  });
  children[0]?.emit({ type: "result", subtype: "success", is_error: false });
  assert.equal(
    events.some((event) => event.kind === "session_paused"),
    false,
  );
  children[0]?.emit({
    type: "result",
    parent_tool_use_id: "tool.child.fixture",
    subtype: "success",
    is_error: false,
  });
  assert.equal(
    events.some((event) => event.kind === "native_message_observed"),
    true,
  );
  assert.equal(
    events.some((event) => event.kind === "session_paused"),
    true,
  );
  const continued = await adapterResult.value.continueSession?.({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    expectedProcessEpoch: started.ok ? started.value.processEpoch : "unavailable",
  });
  assert.equal(continued?.ok && continued.value.observation, "settled");
  assert.equal(continued?.ok && continued.value.currentProcessEpoch, started.value.processEpoch);
  children[0]?.emit({
    type: "control_request",
    request_id: "permission.fixture",
    request: { subtype: "can_use_tool", tool_use_id: "tool.fixture", tool_name: "Read" },
  });
  const request = events.find((event) => event.kind === "host_request_pending");
  assert.equal(request?.kind, "host_request_pending");
  const responded = await adapterResult.value.respondToRequest({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    requestId: "permission.fixture",
    processEpoch: started.value.processEpoch,
    answer: { behavior: "deny", message: "fixture denial" },
  });
  assert.equal(responded.ok, true);
  assert.match(children[0]?.writes.at(-1) ?? "", /permission\.fixture/);
  const stopped = await adapterResult.value.stop?.({
    coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.fixture"),
    expectedProcessEpoch: started.ok ? started.value.processEpoch : "unavailable",
  });
  assert.equal(stopped?.ok && stopped.value.observation, "settled");
  assert.equal(children[0]?.killed, true);
  adapterResult.value.close();
});

test("Claude stream initialization fails immediately when the owned process exits", async () => {
  const child = new ScriptedChild([], {}, "C:/fixture", false);
  const opened = await createClaudeStreamJsonTransportFactory({
    processFactory: { spawn: () => child },
    timeoutMs: 1_000,
  }).open({
    profileId: "profile.claude.exit",
    provider: "claude_code",
    executablePath: "C:/fixture/claude.exe",
    cwd: "C:/fixture",
    modelId: "fixture-small",
    effort: null,
    endpoint: null,
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const starting = opened.value.start({
    scope: {
      coordinatorSessionId: AgentSessionIdSchema.parse("session.claude.exit"),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse("host.claude.fixture"),
      profileId: "profile.claude.exit",
      cwd: "C:/fixture",
      bootstrapText: "",
      bootstrapBasis: "fixture",
    },
  });
  await new Promise((resolve) => setImmediate(resolve));
  child.exit(23);
  const result = await starting;
  assert.equal(result.ok, false);
  assert.equal(result.ok ? "unexpected" : result.error.message, "Provider control channel closed");
  opened.value.close();
});

class ScriptedChild implements OwnedLineProcess {
  readonly pid = 9001;
  readonly args: readonly string[];
  readonly environment: Readonly<Record<string, string>>;
  readonly cwd: string;
  readonly writes: string[] = [];
  killed = false;
  readonly #respondToControl: boolean;
  #lines = new Set<(line: string) => void>();
  #diagnostics = new Set<(diagnostic: ProviderProcessDiagnostic) => void>();
  #exits = new Set<(code: number | null) => void>();

  constructor(
    args: readonly string[],
    environment: Readonly<Record<string, string>>,
    cwd: string,
    respondToControl = true,
  ) {
    this.args = args;
    this.environment = environment;
    this.cwd = cwd;
    this.#respondToControl = respondToControl;
  }

  write(input: string): void {
    this.writes.push(input);
    const parsed: unknown = JSON.parse(input);
    const record = typeof parsed === "object" && parsed !== null ? parsed : {};
    if (!("type" in record)) return;
    if (record.type === "control_request" && "request_id" in record && this.#respondToControl) {
      const request =
        "request" in record && typeof record.request === "object" ? record.request : {};
      const subtype = request !== null && "subtype" in request ? request.subtype : "unknown";
      setImmediate(() =>
        this.emit({
          type: "control_response",
          response: { subtype: "success", request_id: record.request_id, response: { subtype } },
        }),
      );
      return;
    }
    if (record.type === "user" && this.writes.length === 2)
      setImmediate(() => this.emit({ type: "result", subtype: "success", is_error: false }));
  }

  emit(raw: unknown): void {
    for (const listener of this.#lines) listener(JSON.stringify(raw));
  }

  onLine(listener: (line: string) => void): () => void {
    this.#lines.add(listener);
    return () => this.#lines.delete(listener);
  }

  onDiagnostic(listener: (diagnostic: ProviderProcessDiagnostic) => void): () => void {
    this.#diagnostics.add(listener);
    return () => this.#diagnostics.delete(listener);
  }

  onExit(listener: (code: number | null) => void): () => void {
    this.#exits.add(listener);
    return () => this.#exits.delete(listener);
  }

  kill(): void {
    this.killed = true;
    this.exit(0);
  }

  exit(code: number): void {
    for (const listener of this.#exits) listener(code);
  }
}
