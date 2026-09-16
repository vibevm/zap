import assert from "node:assert/strict";
import test from "node:test";
import { createOpenCodeHttpTransportFactory } from "./http.ts";
import { createOpenCodeOwnedTransportFactory, type OpenCodeDaemonFactory } from "./opencode.ts";
import { createProviderCoordinatorAdapter, type ProviderCoordinatorProfile } from "./index.ts";
import {
  AgentSessionIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  ExecutionHostIdSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, ConversationIdSchema } from "../protocol/index.ts";

test("OpenCode HTTP transport uses official session, prompt_async, message and event routes", async () => {
  const requests: Array<{ path: string; method: string; body: unknown }> = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    const path = new URL(input instanceof Request ? input.url : input.toString()).pathname;
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : null;
    requests.push({ path, method: init?.method ?? "GET", body });
    if (path === "/session") return Response.json({ id: "opencode.session.fixture" });
    if (path === "/event")
      return new Response(
        `data: ${JSON.stringify({
          type: "message.part.updated",
          properties: {
            part: { type: "text", sessionID: "opencode.session.fixture", text: "visible" },
          },
        })}\n\n`,
        { status: 200, headers: { "content-type": "text/event-stream" } },
      );
    if (path.endsWith("/prompt_async")) return new Response(null, { status: 204 });
    if (path === "/session/status")
      return Response.json({ "opencode.session.fixture": { type: "busy" } });
    if (path.endsWith("/abort")) return Response.json(true);
    if (path.endsWith("/message")) return Response.json([]);
    return Response.json({});
  };
  const opened = await createOpenCodeHttpTransportFactory({ fetchImpl }).open({
    profileId: "profile.opencode.fixture",
    provider: "opencode",
    executablePath: "C:/fixture/opencode.exe",
    cwd: "C:/fixture",
    modelId: "fixture/fixture-model",
    effort: null,
    endpoint: "http://127.0.0.1:4170",
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const events: unknown[] = [];
  opened.value.subscribe((event) => events.push(event));
  const started = await opened.value.start({
    scope: {
      coordinatorSessionId: AgentSessionIdSchema.parse("session.opencode.fixture"),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse("host.fixture"),
      profileId: "profile.opencode.fixture",
      modelId: "fixture/selected-model",
      cwd: "C:/fixture",
      bootstrapText: "bootstrap fixture",
      bootstrapBasis: "fixture",
    },
  });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const sent = await opened.value.send({
    session: started.value,
    text: "fixture",
    clientMessageId: "message.fixture",
  });
  assert.equal(sent.ok, true);
  if (sent.ok) {
    assert.equal(sent.value.nativeTurnId, null);
    assert.equal(sent.value.transportCorrelation?.provenance, "transport_correlation");
  }
  const paused = await opened.value.pause({ session: started.value });
  assert.equal(paused.ok && paused.value.observation, "requested");
  assert.equal(
    requests.some((request) => request.path.endsWith("/abort")),
    true,
  );
  const prompt = requests.filter((request) => request.path.endsWith("/prompt_async"));
  assert.equal(prompt.length, 2);
  assert.deepEqual(prompt[1]?.body, {
    model: { providerID: "fixture", modelID: "selected-model" },
    parts: [{ type: "text", text: "fixture" }],
  });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(JSON.stringify(events).includes("visible"), true);
  const resumed = await opened.value.resume({
    scope: {
      coordinatorSessionId: AgentSessionIdSchema.parse("session.opencode.fixture"),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse("host.fixture"),
      profileId: "profile.opencode.fixture",
      modelId: "fixture/selected-model",
      cwd: "C:/fixture",
      nativeThreadId: "opencode.session.fixture",
    },
  });
  assert.equal(resumed.ok, true);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(requests.filter((request) => request.path === "/event").length >= 2, true);
  opened.value.close();
});

test("OpenCode owned factory launches one authenticated pure daemon in the exact project cwd", async () => {
  const launches: Array<{
    args: readonly string[];
    cwd: string;
    environment: Readonly<Record<string, string>>;
  }> = [];
  let killed = false;
  const processFactory: OpenCodeDaemonFactory = {
    spawn(input) {
      launches.push(input);
      return {
        pid: 9201,
        onExit: () => () => undefined,
        kill: () => {
          killed = true;
        },
      };
    },
  };
  const fetchImpl: typeof fetch = async (input, init) => {
    const path = new URL(input instanceof Request ? input.url : input.toString()).pathname;
    assert.match(String(new Headers(init?.headers).get("authorization")), /^Basic /);
    if (path === "/doc") return Response.json({ openapi: "3.1.0" });
    if (path === "/session") return Response.json({ id: "owned.session.fixture" });
    if (path === "/event") return new Response(null, { status: 200 });
    if (path.endsWith("/prompt_async")) return new Response(null, { status: 204 });
    return Response.json({});
  };
  const opened = await createOpenCodeOwnedTransportFactory({
    processFactory,
    fetchImpl,
    reservePort: () => Promise.resolve(42117),
    proxyPolicy: { mode: "explicit", httpsProxy: "http://proxy.fixture:8080" },
    prepareLaunch: {
      prepare: () =>
        Promise.resolve({
          ok: true,
          value: {
            environment: { OPENROUTER_API_KEY: "synthetic-not-a-real-key" },
            mcpConfigPath: "C:/fixture/opencode-mcp-prepared.json",
          },
        }),
    },
  }).open({
    profileId: "profile.opencode.owned",
    provider: "opencode",
    executablePath: "C:/fixture/opencode.exe",
    cwd: "C:/configured-must-not-win",
    modelId: "fixture/fixture-model",
    effort: null,
    endpoint: null,
    argumentPrefix: ["C:/fixture/opencode-prefix"],
    environmentRef: "environment.opencode.fixture",
    mcpConfigPath: "C:/fixture/opencode-mcp-base.json",
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const started = await opened.value.start({
    scope: {
      coordinatorSessionId: AgentSessionIdSchema.parse("session.opencode.owned"),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse("host.fixture"),
      profileId: "profile.opencode.owned",
      cwd: "C:/exact/project",
      bootstrapText: "owned bootstrap",
      bootstrapBasis: "fixture",
    },
  });
  assert.equal(started.ok, true);
  assert.equal(launches[0]?.cwd, "C:/exact/project");
  assert.deepEqual(launches[0]?.args, [
    "C:/fixture/opencode-prefix",
    "serve",
    "--pure",
    "--hostname",
    "127.0.0.1",
    "--port",
    "42117",
  ]);
  assert.equal(launches[0]?.environment["HTTPS_PROXY"], "http://proxy.fixture:8080");
  assert.equal(launches[0]?.environment["OPENCODE_SERVER_USERNAME"], "zap");
  assert.ok(launches[0]?.environment["OPENCODE_SERVER_PASSWORD"]);
  assert.equal(launches[0]?.environment["OPENROUTER_API_KEY"], "synthetic-not-a-real-key");
  assert.equal(
    launches[0]?.environment["OPENCODE_CONFIG"],
    "C:/fixture/opencode-mcp-prepared.json",
  );
  opened.value.close();
  assert.equal(killed, true);
});

test("OpenCode HTTP failures retain safe schema metadata without response text", async () => {
  const rawMessage = "invalid field synthetic-secret-must-not-escape";
  const opened = await createOpenCodeHttpTransportFactory({
    fetchImpl: () =>
      Promise.resolve(
        Response.json(
          {
            _tag: "InvalidRequestError",
            message: rawMessage,
            kind: "validation",
            field: "messageID",
          },
          { status: 400 },
        ),
      ),
  }).open({
    profileId: "profile.opencode.error",
    provider: "opencode",
    executablePath: "C:/fixture/opencode.exe",
    cwd: "C:/fixture",
    modelId: "fixture/fixture-model",
    effort: null,
    endpoint: "http://127.0.0.1:4170",
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const result = await opened.value.start({
    scope: {
      coordinatorSessionId: AgentSessionIdSchema.parse("session.opencode.error"),
      projectId: ProjectIdSchema.parse("project.fixture"),
      contextId: WorkContextIdSchema.parse("context.fixture"),
      conversationId: ConversationIdSchema.parse("conversation.fixture"),
      coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
      hostId: ExecutionHostIdSchema.parse("host.fixture"),
      profileId: "profile.opencode.error",
      cwd: "C:/fixture",
      bootstrapText: "",
      bootstrapBasis: "fixture",
    },
  });
  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.match(result.error.message, /InvalidRequestError\/validation\/messageID/u);
  assert.match(result.error.message, /sha256=[0-9a-f]{64} bytes=/u);
  assert.doesNotMatch(result.error.message, /synthetic-secret/u);
});

test("OpenCode public Stop settles only after the owned daemon exits", async () => {
  let exitListener: ((code: number | null) => void) | undefined;
  let killRequested = false;
  const processFactory: OpenCodeDaemonFactory = {
    spawn: () => ({
      pid: 9202,
      onExit(listener) {
        exitListener = listener;
        return () => {
          if (exitListener === listener) exitListener = undefined;
        };
      },
      kill() {
        killRequested = true;
      },
    }),
  };
  const fetchImpl: typeof fetch = async (input) => {
    const path = new URL(input instanceof Request ? input.url : input.toString()).pathname;
    if (path === "/doc") return Response.json({ openapi: "3.1.0" });
    if (path === "/session") return Response.json({ id: "owned.session.stop" });
    if (path.endsWith("/abort")) return Response.json(true);
    if (path === "/event") return new Response(null, { status: 200 });
    return Response.json({});
  };
  const profile: ProviderCoordinatorProfile = {
    profileId: "profile.opencode.stop",
    provider: "opencode",
    executablePath: "C:/fixture/opencode.exe",
    cwd: "C:/fixture",
    modelId: "fixture/fixture-model",
    effort: null,
    endpoint: null,
  };
  const transport = await createOpenCodeOwnedTransportFactory({
    processFactory,
    fetchImpl,
    reservePort: () => Promise.resolve(42118),
  }).open(profile);
  assert.equal(transport.ok, true);
  if (!transport.ok) return;
  const adapter = createProviderCoordinatorAdapter({
    profile,
    hostId: "host.opencode.stop",
    transport: transport.value,
  });
  assert.equal(adapter.ok, true);
  if (!adapter.ok) return;
  const sessionId = AgentSessionIdSchema.parse("session.opencode.stop");
  const started = await adapter.value.start({
    coordinatorSessionId: sessionId,
    projectId: ProjectIdSchema.parse("project.fixture"),
    contextId: WorkContextIdSchema.parse("context.fixture"),
    conversationId: ConversationIdSchema.parse("conversation.fixture"),
    coordinatorActorId: ActorIdSchema.parse("actor.fixture"),
    hostId: ExecutionHostIdSchema.parse("host.opencode.stop"),
    profileId: profile.profileId,
    cwd: "C:/fixture",
    bootstrapText: "",
    bootstrapBasis: "fixture",
  });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const events: Array<{ readonly kind: string }> = [];
  adapter.value.subscribe((event) => events.push(event));
  const stopped = await adapter.value.stop?.({
    coordinatorSessionId: sessionId,
    expectedProcessEpoch: started.value.processEpoch,
  });
  assert.equal(stopped?.ok && stopped.value.observation, "requested");
  assert.equal(killRequested, true);
  assert.equal(
    events.some((event) => event.kind === "session_stopped"),
    false,
  );
  exitListener?.(0);
  assert.equal(
    events.some((event) => event.kind === "session_stopped"),
    true,
  );
  adapter.value.close();
});
