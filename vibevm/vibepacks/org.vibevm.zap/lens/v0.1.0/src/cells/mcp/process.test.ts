import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { z } from "zod";
import { runCli } from "../../cli.ts";
import { openBroker, type LensBroker } from "../broker/index.ts";
import { createLensHttpGateway, type LensHttpGateway } from "../http/index.ts";
import {
  CredentialSchema,
  EnrollPrincipalInputSchema,
  type Credential,
} from "../protocol/index.ts";
import {
  openSqliteAdapterSessionVault,
  type SqliteAdapterSessionVault,
} from "../session-vault/index.ts";

const mcpEntry = fileURLToPath(new URL("../../mcp.ts", import.meta.url));

test("independent MCP processes share delegated handles across broker restart", async () => {
  const databasePath = join(mkdtempSync(join(tmpdir(), "codlens-mcp-")), "lens.sqlite");
  const first = openService(databasePath);
  assert.equal(first.ok, true);
  if (!first.ok) return;
  const enrollment = first.broker.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: ["workspace.process"],
      conversationIds: ["conversation.process"],
      capabilities: [
        "message:emit",
        "question:ask",
        "inbox:read",
        "inbox:ack",
        "inbox:forward",
        "actor:delegate",
        "actor:expire",
      ],
    }),
  );
  assert.equal(enrollment.ok, true);
  if (!enrollment.ok) return;
  const principalToken = enrollment.value.principalToken;
  const started = await first.gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const brokerUrl = `http://127.0.0.1:${started.value.port}`;

  const parent = await processClient(brokerUrl, principalToken);
  const parentHandle = publicResult(
    await parent.callTool({
      name: "codlens_connect",
      arguments: connectInput("request.process.parent", "parent"),
    }),
  );
  const childHandle = publicResult(
    await parent.callTool({
      name: "codlens_delegate",
      arguments: delegateInput(parentHandle, "child", "request.process.child"),
    }),
  );
  const duplicateChildHandle = publicResult(
    await parent.callTool({
      name: "codlens_delegate",
      arguments: delegateInput(parentHandle, "child", "request.process.child"),
    }),
  );
  assert.equal(duplicateChildHandle, childHandle);

  const child = await processClient(brokerUrl, principalToken);
  const childQuestion = questionResult(
    await child.callTool({
      name: "codlens_ask",
      arguments: askInput(childHandle, "request.process.child.ask"),
    }),
  );
  const nestedHandle = publicResult(
    await child.callTool({
      name: "codlens_delegate",
      arguments: delegateInput(childHandle, "nested", "request.process.nested"),
    }),
  );

  const nested = await processClient(brokerUrl, principalToken);
  const nestedQuestion = questionResult(
    await nested.callTool({
      name: "codlens_ask",
      arguments: askInput(nestedHandle, "request.process.nested.ask"),
    }),
  );
  assert.notEqual(childQuestion.originActorId, nestedQuestion.originActorId);
  await Promise.all([parent.close(), child.close(), nested.close()]);
  await first.gateway.close();
  first.vault.close();
  first.broker.close();

  const second = openService(databasePath);
  assert.equal(second.ok, true);
  if (!second.ok) return;
  const restarted = await second.gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(restarted.ok, true);
  if (!restarted.ok) return;
  const resumedClient = await processClient(
    `http://127.0.0.1:${restarted.value.port}`,
    principalToken,
  );
  const afterRestart = questionResult(
    await resumedClient.callTool({
      name: "codlens_ask",
      arguments: askInput(nestedHandle, "request.process.after-restart"),
    }),
  );
  assert.equal(afterRestart.originActorId, nestedQuestion.originActorId);
  const finished = await resumedClient.callTool({
    name: "codlens_finish",
    arguments: {
      adapterSessionId: nestedHandle,
      clientRequestId: "request.process.expire",
    },
  });
  assert.equal(finished.isError, false);
  await resumedClient.close();
  await second.gateway.close();
  second.vault.close();
  second.broker.close();

  const third = openService(databasePath);
  assert.equal(third.ok, true);
  if (!third.ok) return;
  const afterExpiryStart = await third.gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(afterExpiryStart.ok, true);
  if (!afterExpiryStart.ok) return;
  const afterExpiry = await processClient(
    `http://127.0.0.1:${afterExpiryStart.value.port}`,
    principalToken,
  );
  const rejected = await afterExpiry.callTool({
    name: "codlens_ask",
    arguments: askInput(nestedHandle, "request.process.expired.ask"),
  });
  assert.equal(rejected.isError, true);
  await afterExpiry.close();
  await third.gateway.close();
  third.vault.close();
  third.broker.close();
});

test("setup credentials support child finish, delayed answer and parent forwarding", async () => {
  const root = mkdtempSync(join(tmpdir(), "codlens-lifecycle-"));
  const databasePath = join(root, "lens.sqlite");
  const credentialPath = join(root, "credentials.json");
  const setupOutput: string[] = [];
  const setup = await runCli(
    [
      "setup",
      JSON.stringify({
        workspaceId: "workspace.lifecycle",
        conversationId: "conversation.lifecycle",
      }),
    ],
    {
      CODLENS_DATABASE_PATH: databasePath,
      CODLENS_CREDENTIAL_FILE: credentialPath,
    },
    (line) => {
      setupOutput.push(line);
    },
  );
  assert.equal(setup, 0);
  const credentials = z
    .object({
      agent: z.object({ principalToken: CredentialSchema }),
    })
    .passthrough()
    .parse(JSON.parse(readFileSync(credentialPath, "utf8")));
  const service = openService(databasePath);
  assert.equal(service.ok, true);
  if (!service.ok) return;
  const started = await service.gateway.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const brokerUrl = `http://127.0.0.1:${String(started.value.port)}`;
  const parent = await processClient(brokerUrl, credentials.agent.principalToken);
  const parentConnection = publicConnectionResult(
    await parent.callTool({
      name: "codlens_connect",
      arguments: lifecycleConnectInput("request.lifecycle.parent"),
    }),
  );
  const childConnection = publicConnectionResult(
    await parent.callTool({
      name: "codlens_delegate",
      arguments: {
        adapterSessionId: parentConnection.adapterSessionId,
        input: {
          clientRequestId: "request.lifecycle.child",
          capabilities: ["question:ask", "inbox:read", "actor:expire"],
          host: {
            kind: "test",
            subagentId: "lifecycle.child",
            provenance: "explicit_handle",
          },
          replyPolicy: { kind: "forward_parent" },
        },
      },
    }),
  );
  const child = await processClient(brokerUrl, credentials.agent.principalToken);
  const question = questionResult(
    await child.callTool({
      name: "codlens_ask",
      arguments: askInput(childConnection.adapterSessionId, "request.lifecycle.ask"),
    }),
  );
  const finished = await child.callTool({
    name: "codlens_finish",
    arguments: {
      adapterSessionId: childConnection.adapterSessionId,
      clientRequestId: "request.lifecycle.finish",
    },
  });
  assert.equal(finished.isError, false);
  const answerOutput: string[] = [];
  assert.equal(
    await runCli(
      [
        "answer",
        JSON.stringify({
          clientRequestId: "request.lifecycle.answer",
          workspaceId: "workspace.lifecycle",
          conversationId: "conversation.lifecycle",
          questionId: question.questionId,
          expectedRevision: "1",
          answer: "continue through parent",
        }),
      ],
      { CODLENS_URL: brokerUrl, CODLENS_CREDENTIAL_FILE: credentialPath },
      (line) => {
        answerOutput.push(line);
      },
    ),
    0,
  );
  assert.match(answerOutput[0] ?? "", /answered/);
  const forwarded = await parent.callTool({
    name: "codlens_forward",
    arguments: {
      adapterSessionId: parentConnection.adapterSessionId,
      input: {
        clientRequestId: "request.lifecycle.forward",
        fromActorId: childConnection.actorId,
        toActorId: parentConnection.actorId,
      },
    },
  });
  assert.equal(forwarded.isError, false);
  const inbox = await parent.callTool({
    name: "codlens_inbox",
    arguments: {
      adapterSessionId: parentConnection.adapterSessionId,
      input: { afterSequence: "0", limit: 50 },
    },
  });
  assert.match(JSON.stringify(inbox.structuredContent), /continue through parent/);
  await Promise.all([parent.close(), child.close()]);
  await service.gateway.close();
  service.vault.close();
  service.broker.close();
});

async function processClient(url: string, principalToken: Credential): Promise<Client> {
  const client = new Client({ name: "codlens-process-test", version: "1.0.0" });
  await client.connect(
    new StdioClientTransport({
      command: process.execPath,
      args: [mcpEntry],
      env: {
        CODLENS_URL: url,
        CODLENS_PRINCIPAL_TOKEN: principalToken,
      },
    }),
  );
  return client;
}

function openService(databasePath: string):
  | {
      readonly ok: true;
      readonly broker: LensBroker;
      readonly vault: SqliteAdapterSessionVault;
      readonly gateway: LensHttpGateway;
    }
  | { readonly ok: false } {
  const broker = openBroker({ databasePath });
  const vault = openSqliteAdapterSessionVault(databasePath);
  if (!broker.ok || !vault.ok) return { ok: false };
  const gateway = createLensHttpGateway({
    broker: broker.value,
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: [],
    statusToken: CredentialSchema.parse("status-process-test-0000000001"),
    adapterSessionIdFactory: () => `adapter.${crypto.randomUUID().replaceAll("-", "")}`,
    adapterSessionVault: vault.value,
  });
  return { ok: true, broker: broker.value, vault: vault.value, gateway };
}

function connectInput(clientRequestId: string, sessionId: string) {
  return {
    clientRequestId,
    workspaceId: "workspace.process",
    conversationId: "conversation.process",
    capabilities: [
      "message:emit",
      "question:ask",
      "inbox:read",
      "inbox:ack",
      "actor:delegate",
      "actor:expire",
      "inbox:forward",
    ],
    host: { kind: "test", sessionId, provenance: "explicit_handle" },
    replyPolicy: { kind: "retain" },
  };
}

function lifecycleConnectInput(clientRequestId: string) {
  return {
    clientRequestId,
    workspaceId: "workspace.lifecycle",
    conversationId: "conversation.lifecycle",
    capabilities: [
      "question:ask",
      "inbox:read",
      "inbox:ack",
      "inbox:forward",
      "actor:delegate",
      "actor:expire",
    ],
    host: { kind: "test", sessionId: "lifecycle.parent", provenance: "explicit_handle" },
    replyPolicy: { kind: "retain" },
  };
}

function delegateInput(parent: string, subagentId: string, clientRequestId: string) {
  return {
    adapterSessionId: parent,
    input: {
      clientRequestId,
      capabilities: ["question:ask", "inbox:read", "actor:delegate", "actor:expire"],
      host: { kind: "test", subagentId, provenance: "explicit_handle" },
      replyPolicy: { kind: "forward_parent" },
    },
  };
}

function askInput(adapterSessionId: string, clientRequestId: string) {
  return {
    adapterSessionId,
    input: {
      clientRequestId,
      prompt: "Answer independently",
      answerMode: "free_text",
      choices: [],
      independentWorkAvailable: true,
    },
  };
}

function publicResult(result: Awaited<ReturnType<Client["callTool"]>>): string {
  return z
    .object({
      ok: z.literal(true),
      value: z.object({ adapterSessionId: z.string() }).passthrough(),
    })
    .passthrough()
    .parse(result.structuredContent).value.adapterSessionId;
}

function publicConnectionResult(result: Awaited<ReturnType<Client["callTool"]>>): {
  readonly adapterSessionId: string;
  readonly actorId: string;
} {
  const value = z
    .object({
      ok: z.literal(true),
      value: z
        .object({
          adapterSessionId: z.string(),
          connection: z
            .object({
              actor: z.object({ actorId: z.string() }).passthrough(),
            })
            .passthrough(),
        })
        .passthrough(),
    })
    .passthrough()
    .parse(result.structuredContent).value;
  return {
    adapterSessionId: value.adapterSessionId,
    actorId: value.connection.actor.actorId,
  };
}

function questionResult(result: Awaited<ReturnType<Client["callTool"]>>) {
  return z
    .object({
      ok: z.literal(true),
      value: z.object({ originActorId: z.string(), questionId: z.string() }).passthrough(),
    })
    .passthrough()
    .parse(result.structuredContent).value;
}
