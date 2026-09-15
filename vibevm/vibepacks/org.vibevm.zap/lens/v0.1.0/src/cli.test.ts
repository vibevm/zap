import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { z } from "zod";
import { createAgentHttpClient } from "./cells/http/index.ts";
import { AdapterSessionIdSchema } from "./cells/transport/index.ts";
import { ClientRequestIdSchema, CredentialSchema } from "./cells/protocol/index.ts";
import { runCli } from "./cli.ts";

test("setup creates protected credentials without printing their values", async () => {
  const root = mkdtempSync(join(tmpdir(), "codlens-setup-"));
  const databasePath = join(root, "lens.sqlite");
  const credentialPath = join(root, "credentials.json");
  const output: string[] = [];
  const status = await runCli(
    [
      "setup",
      JSON.stringify({
        workspaceId: "workspace.setup",
        conversationId: "conversation.setup",
      }),
    ],
    {
      CODLENS_DATABASE_PATH: databasePath,
      CODLENS_CREDENTIAL_FILE: credentialPath,
    },
    (line) => output.push(line),
  );
  assert.equal(status, 0);
  assert.equal(existsSync(databasePath), true);
  assert.equal(existsSync(credentialPath), true);
  const storedText = readFileSync(credentialPath, "utf8");
  const stored = z
    .object({
      statusToken: z.string(),
      agent: z.object({ principalToken: z.string() }).passthrough(),
      humanResponder: z.object({ principalToken: z.string() }).passthrough(),
    })
    .passthrough()
    .parse(JSON.parse(storedText));
  const rendered = output.join("\n");
  assert.equal(rendered.includes(stored.statusToken), false);
  assert.equal(rendered.includes(stored.agent.principalToken), false);
  assert.equal(rendered.includes(stored.humanResponder.principalToken), false);

  if (process.platform === "win32") {
    const acl = spawnSync("icacls.exe", [credentialPath], {
      encoding: "utf8",
      windowsHide: true,
    });
    assert.equal(acl.status, 0);
    assert.match(acl.stdout, new RegExp(escapePattern(userInfo().username), "i"));
  }
});

test("headless CLI setup, connect, publish, inbox and answer use one broker", async () => {
  const root = mkdtempSync(join(tmpdir(), "codlens-cli-"));
  const databasePath = join(root, "lens.sqlite");
  const credentialPath = join(root, "credentials.json");
  const setupOutput: string[] = [];
  assert.equal(
    await runCli(
      [
        "setup",
        JSON.stringify({ workspaceId: "workspace.cli", conversationId: "conversation.cli" }),
      ],
      { CODLENS_DATABASE_PATH: databasePath, CODLENS_CREDENTIAL_FILE: credentialPath },
      (line) => setupOutput.push(line),
    ),
    0,
  );
  const port = await availablePort();
  const entry = fileURLToPath(new URL("./cli.ts", import.meta.url));
  const service = spawn(process.execPath, [entry, "start"], {
    env: {
      ...process.env,
      CODLENS_DATABASE_PATH: databasePath,
      CODLENS_CREDENTIAL_FILE: credentialPath,
      CODLENS_PORT: String(port),
      CODLENS_ALLOWED_HOSTS: "127.0.0.1",
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (service.stdout === null) {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: service stdout is missing; fix surface: spawn the fixture with a piped stdout",
    );
  }
  await once(service.stdout, "data");
  const common = {
    CODLENS_URL: `http://127.0.0.1:${port}`,
    CODLENS_CREDENTIAL_FILE: credentialPath,
  };
  const connectedOutput: string[] = [];
  assert.equal(
    await runCli(
      [
        "connect",
        JSON.stringify({
          clientRequestId: "request.cli.connect",
          workspaceId: "workspace.cli",
          conversationId: "conversation.cli",
          capabilities: [
            "message:emit",
            "question:ask",
            "inbox:read",
            "inbox:ack",
            "actor:delegate",
          ],
          host: {
            kind: "codex",
            sessionId: "native.parent",
            provenance: "explicit_handle",
          },
          replyPolicy: { kind: "retain" },
        }),
      ],
      common,
      (line) => connectedOutput.push(line),
    ),
    0,
  );
  const connected = z
    .object({
      ok: z.literal(true),
      value: z
        .object({
          adapterSessionId: z.string(),
          connection: z
            .object({ actor: z.object({ actorId: z.string() }).passthrough() })
            .passthrough(),
        })
        .passthrough(),
    })
    .passthrough()
    .parse(JSON.parse(connectedOutput[0] ?? ""));
  const sessionEnvironment = {
    ...common,
    CODLENS_ADAPTER_SESSION_ID: connected.value.adapterSessionId,
  };
  const published: string[] = [];
  assert.equal(
    await runCli(
      [
        "publish",
        JSON.stringify({
          clientRequestId: "request.cli.publish",
          toActorId: connected.value.connection.actor.actorId,
          payload: { text: "hello" },
        }),
      ],
      sessionEnvironment,
      (line) => published.push(line),
    ),
    0,
  );
  const inbox: string[] = [];
  assert.equal(await runCli(["inbox", "{}"], sessionEnvironment, (line) => inbox.push(line)), 0);
  assert.match(inbox[0] ?? "", /hello/);

  const client = createAgentHttpClient({
    baseUrl: new URL(common.CODLENS_URL),
    principalToken: CredentialSchema.parse(storedAgentToken(credentialPath)),
  });
  const parentSession = AdapterSessionIdSchema.parse(connected.value.adapterSessionId);
  const childA = await client.delegate(parentSession, {
    clientRequestId: "request.cli.child-a",
    capabilities: ["inbox:read"],
    host: {
      kind: "codex",
      sessionId: "native.parent",
      subagentId: "native.child-a",
      provenance: "explicit_handle",
    },
    replyPolicy: { kind: "forward_parent" },
  });
  const childB = await client.delegate(parentSession, {
    clientRequestId: "request.cli.child-b",
    capabilities: ["inbox:read"],
    host: {
      kind: "codex",
      sessionId: "native.parent",
      subagentId: "native.child-b",
      provenance: "explicit_handle",
    },
    replyPolicy: { kind: "forward_parent" },
  });
  assert.equal(childA.ok && childB.ok, true);
  if (!childA.ok || !childB.ok) return;
  for (let index = 0; index < 12; index += 1) {
    const emitted = await client.emit(parentSession, {
      clientRequestId: ClientRequestIdSchema.parse(`request.cli.child-a.${String(index)}`),
      toActorId: childA.value.connection.actor.actorId,
      payload: { text: `child-a-${String(index)}` },
    });
    assert.equal(emitted.ok, true);
  }
  assert.equal(
    (
      await client.emit(parentSession, {
        clientRequestId: ClientRequestIdSchema.parse("request.cli.child-b.message"),
        toActorId: childB.value.connection.actor.actorId,
        payload: { text: "child-b-only" },
      })
    ).ok,
    true,
  );
  const childAHook = JSON.stringify({
    session_id: "native.parent",
    hook_event_name: "PostToolUse",
    agent_id: "native.child-a",
  });
  const firstOffer: string[] = [];
  assert.equal(
    await runCli(["host-hook", "codex", childAHook], common, (line) => firstOffer.push(line)),
    0,
  );
  assert.match(firstOffer[0] ?? "", /PostToolUse/);
  assert.doesNotMatch(firstOffer[0] ?? "", /child-b-only/);
  const secondOffer: string[] = [];
  assert.equal(
    await runCli(["host-hook", "codex", childAHook], common, (line) => secondOffer.push(line)),
    0,
  );
  assert.match(secondOffer[0] ?? "", /child-a-11/);
  const childBHook = JSON.stringify({
    session_id: "native.parent",
    hook_event_name: "SubagentStop",
    agent_id: "native.child-b",
  });
  const childBOffer: string[] = [];
  assert.equal(
    await runCli(["host-hook", "codex", childBHook], common, (line) => childBOffer.push(line)),
    0,
  );
  assert.match(childBOffer[0] ?? "", /child-b-only/);
  assert.match(childBOffer[0] ?? "", /SubagentStop/);

  const asked = await fetch(`${common.CODLENS_URL}/v1/ask`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${storedAgentToken(credentialPath)}`,
      "X-Codlens-Adapter-Session": connected.value.adapterSessionId,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      clientRequestId: "request.cli.ask",
      prompt: "Continue?",
      answerMode: "free_text",
    }),
  });
  const question = z
    .object({
      ok: z.literal(true),
      value: z.object({ questionId: z.string(), revision: z.string() }).passthrough(),
    })
    .passthrough()
    .parse(JSON.parse(await asked.text()));
  const answered: string[] = [];
  assert.equal(
    await runCli(
      [
        "answer",
        JSON.stringify({
          clientRequestId: "request.cli.answer",
          workspaceId: "workspace.cli",
          conversationId: "conversation.cli",
          questionId: question.value.questionId,
          expectedRevision: question.value.revision,
          answer: "yes",
        }),
      ],
      common,
      (line) => answered.push(line),
    ),
    0,
  );
  assert.match(answered[0] ?? "", /answered/);
  service.kill();
  await once(service, "exit");
  const restartedService = spawn(process.execPath, [entry, "start"], {
    env: {
      ...process.env,
      CODLENS_DATABASE_PATH: databasePath,
      CODLENS_CREDENTIAL_FILE: credentialPath,
      CODLENS_PORT: String(port),
      CODLENS_ALLOWED_HOSTS: "127.0.0.1",
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (restartedService.stdout === null) {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: restarted stdout is missing; fix surface: spawn with piped stdout",
    );
  }
  await once(restartedService.stdout, "data");
  const afterRestartOffer: string[] = [];
  assert.equal(
    await runCli(["host-hook", "codex", childAHook], common, (line) => {
      afterRestartOffer.push(line);
    }),
    0,
  );
  assert.match(afterRestartOffer[0] ?? "", /child-a-/);
  assert.doesNotMatch(afterRestartOffer[0] ?? "", /child-b-only/);
  restartedService.kill();
  await once(restartedService, "exit");
});

function escapePattern(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function storedAgentToken(path: string): string {
  return z
    .object({
      agent: z.object({ principalToken: z.string() }).passthrough(),
    })
    .passthrough()
    .parse(JSON.parse(readFileSync(path, "utf8"))).agent.principalToken;
}

async function availablePort(): Promise<number> {
  const server = createServer();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: fixture port is missing; fix surface: bind a TCP loopback socket",
    );
  }
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error === undefined ? resolve() : reject(error))),
  );
  return address.port;
}
