/** Composed non-Codex normal-start proof. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { z } from "zod";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createProtectedEnvironmentResolver } from "../product-app/index.ts";
import { createWorkspaceHttpConnection } from "../workspace-client/index.ts";
import { createWayfinderRuntime } from "./index.ts";

test("normal Qwen profile receives protected environment and generated MCP before spawn", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-qwen-runtime-"));
  const projectDirectory = await mkdtemp(join(root, "project-"));
  const capturePath = join(root, "qwen-launch.json");
  const scriptPath = join(root, "qwen-fixture.mjs");
  await writeFile(
    scriptPath,
    [
      "import { writeFileSync } from 'node:fs';",
      "writeFileSync(process.env.CAPTURE_PATH, JSON.stringify({args:process.argv.slice(2),url:process.env.CODLENS_URL,credential:process.env.CODLENS_CREDENTIAL_FILE,marker:process.env.SYNTHETIC_PROVIDER_MARKER}));",
      "process.stdin.setEncoding('utf8');",
      "process.stdin.once('data',()=>console.log(JSON.stringify({type:'system',session_id:'qwen-session-fixture'})));",
      "setInterval(()=>undefined,60000);",
    ].join("\n"),
  );
  const environmentPath = join(root, "qwen-environment.json");
  await writeFile(
    environmentPath,
    JSON.stringify({
      CAPTURE_PATH: capturePath,
      SYNTHETIC_PROVIDER_MARKER: "protected-environment-observed",
    }),
  );
  const origin = "http://127.0.0.1:4174";
  const runtime = createWayfinderRuntime(
    {
      version: 1,
      state: { databasePath: join(root, "workspace.sqlite") },
      gateway: {
        host: "127.0.0.1",
        port: 0,
        namespace: "qwenruntime",
        pairingToken: "pairing.qwen.runtime.synthetic.0001",
        allowedHosts: ["127.0.0.1"],
        allowedOrigins: [origin],
      },
      agentGateway: {
        databasePath: join(root, "agent.sqlite"),
        host: "127.0.0.1",
        port: 0,
        allowedHosts: ["127.0.0.1"],
        allowedOrigins: [],
        statusToken: "status.qwen.runtime.synthetic.0001",
        scopes: [],
      },
      profiles: [],
      providerCoordinatorProfiles: [
        {
          profileId: "profile.qwen.runtime",
          provider: "qwen_code",
          executablePath: process.execPath,
          argumentPrefix: [scriptPath],
          cwd: root,
          modelId: "synthetic-qwen-model",
          effort: null,
          endpoint: null,
          environmentRef: "environment.qwen.runtime",
          mcpConfigPath: join(root, "mcp", "qwen.json"),
          proxy: { mode: "direct" },
        },
      ],
      productProviders: [
        {
          profileId: "profile.qwen.runtime",
          provider: "qwen_code",
          displayName: "Qwen fixture",
          modelId: "synthetic-qwen-model",
          effort: null,
          interactionKind: "structured",
          installed: true,
          configured: true,
          authenticated: "not_observed",
          launchable: true,
          evidence: ["Local no-model stream fixture"],
        },
      ],
      projects: [],
      modelPolicies: [],
      proxy: { mode: "direct" },
    },
    {
      managedEnvironment: createProtectedEnvironmentResolver({
        "environment.qwen.runtime": environmentPath,
      }),
    },
  );
  assert.equal(runtime.ok, true);
  if (!runtime.ok) return;
  t.after(() => runtime.value.close());
  const started = await runtime.value.start();
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const ticket = runtime.value.issuePairingTicket();
  assert.equal(ticket.ok, true);
  if (!ticket.ok) return;
  const client = createWorkspaceHttpConnection({
    baseUrl: `http://${started.value.host}:${String(started.value.port)}${started.value.basePath}`,
    origin,
    pairingToken: ticket.value.ticket,
  });
  assert.notEqual(client, null);
  if (client === null) return;
  const registered = await client.product.request({
    operation: "product.project.register.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.qwen.runtime.project"),
    directoryPath: projectDirectory,
    displayName: "Qwen runtime project",
    profileId: "profile.qwen.runtime",
  });
  assert.equal(registered.ok, true);
  if (!registered.ok || registered.value.operation !== "product.project.register.v1") return;
  const launched = await client.workspace.command({
    operation: "session.start.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.qwen.runtime.start"),
    projectId: registered.value.project.projectId,
    contextId: registered.value.project.contextId,
    profileId: "profile.qwen.runtime",
    interactionKind: "structured",
  });
  assert.equal(launched.ok, true, launched.ok ? undefined : launched.error.message);
  const capture: unknown = JSON.parse(await readFile(capturePath, "utf8"));
  const record = captureRecord(capture);
  assert.equal(record.marker, "protected-environment-observed");
  assert.match(record.url, /^http:\/\/127\.0\.0\.1:/);
  assert.match(record.credential, /credential\.json$/);
  assert.equal(record.args.includes("--mcp-config"), true);
  assert.equal(record.args.includes("--bare"), true);
  assert.deepEqual(
    record.args.slice(
      record.args.indexOf("--approval-mode"),
      record.args.indexOf("--approval-mode") + 2,
    ),
    ["--approval-mode", "default"],
  );
  assert.equal(record.args.includes("--allowed-mcp-server-names"), true);
  assert.equal(record.args.includes("mcp__zap-wayfinder__codlens_assigned_context"), true);
  assert.equal(record.args.includes("mcp__zap-wayfinder__codlens_inbox_wait"), true);
  assert.equal(record.args.includes("mcp__zap-wayfinder__codlens_managed_work_start"), true);
  assert.equal(record.args.includes("mcp__zap-wayfinder__codlens_native_work_before"), true);
  assert.equal(record.args.includes("mcp__zap-wayfinder__codlens_plan_apply"), false);
});

function captureRecord(value: unknown): {
  readonly args: readonly string[];
  readonly url: string;
  readonly credential: string;
  readonly marker: string;
} {
  return z
    .object({
      args: z.array(z.string()),
      url: z.string(),
      credential: z.string(),
      marker: z.string(),
    })
    .strict()
    .parse(value);
}
