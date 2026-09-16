#!/usr/bin/env node
/**
 * Headless lens fixture entry point. Ambient configuration ends here.
 * @scope spec://org.vibevm.zap/lens/PROP-001#transport
 */
import { randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import { chmodSync, readFileSync, writeFileSync } from "node:fs";
import { userInfo } from "node:os";
import { pathToFileURL } from "node:url";
import { z } from "zod";
import { openBroker } from "./cells/broker/index.ts";
import { createLensHttpGateway } from "./cells/http/index.ts";
import { openSqliteAdapterSessionVault } from "./cells/session-vault/index.ts";
import {
  CredentialSchema,
  EnrollPrincipalInputSchema,
  PrincipalEnrollmentSchema,
} from "./cells/protocol/index.ts";

const usage = "codlens setup|start|connect|status|publish|inbox|answer [json]";

const SetupInputSchema = z
  .object({
    workspaceId: z.string(),
    conversationId: z.string(),
  })
  .strict();
const CredentialFileSchema = z
  .object({
    protocol: z.literal("lens/1"),
    statusToken: CredentialSchema,
    agent: PrincipalEnrollmentSchema,
    humanResponder: PrincipalEnrollmentSchema,
  })
  .strict();

export async function runCli(
  argv: readonly string[],
  environment: NodeJS.ProcessEnv,
  write: (line: string) => void,
): Promise<number> {
  const command = argv[0];
  if (command === "mcp" && argv[1] === "serve") {
    const { runMcp } = await import("./mcp.ts");
    return runMcp(environment);
  }
  if (command === "host-hook") return hostHook(argv[1], argv[2], environment, write);
  if (command === "setup") return setup(environment, argv[1], write);
  if (command === "start") return start(environment, write);
  if (command === "connect")
    return request(environment, "POST", "/v1/connect", argv[1], "principal", write);
  if (command === "status")
    return request(environment, "GET", "/v1/status", undefined, "status", write);
  if (command === "publish")
    return request(environment, "POST", "/v1/emit", argv[1], "session", write);
  if (command === "inbox")
    return request(environment, "POST", "/v1/inbox", argv[1] ?? "{}", "session", write);
  if (command === "answer")
    return request(environment, "POST", "/v1/answer", argv[1], "principal", write);
  write(JSON.stringify({ protocol: "lens/1", ok: false, error: usage }));
  return 2;
}

async function hostHook(
  host: string | undefined,
  input: string | undefined,
  environment: NodeJS.ProcessEnv,
  write: (line: string) => void,
): Promise<number> {
  if (!new Set(["codex", "claude_code", "qwen_code"]).has(host ?? "")) return 0;
  const brokerUrl = environment["CODLENS_URL"];
  const principal = credentialValue(environment, "principal");
  if (brokerUrl === undefined || input === undefined || !principal.ok) return 0;
  let baseUrl: URL;
  try {
    baseUrl = new URL(brokerUrl);
  } catch {
    return 0;
  }
  const adapterSession = environment["CODLENS_ADAPTER_SESSION_ID"];
  try {
    const response = await fetch(new URL(`/v1/host-hook/${host ?? ""}`, baseUrl), {
      method: "POST",
      headers: {
        Authorization: `Bearer ${principal.value}`,
        "Content-Type": "application/json",
        ...(adapterSession === undefined ? {} : { "X-Codlens-Adapter-Session": adapterSession }),
      },
      body: input,
    });
    const output = await response.text();
    if (response.ok && output.length > 0) write(output);
    return response.ok ? 0 : 1;
  } catch {
    return 1;
  }
}

async function start(
  environment: NodeJS.ProcessEnv,
  write: (line: string) => void,
): Promise<number> {
  const databasePath = environment["CODLENS_DATABASE_PATH"];
  const statusToken = credentialValue(environment, "status");
  if (databasePath === undefined || !statusToken.ok) {
    write(
      configurationError("start requires CODLENS_DATABASE_PATH and a valid CODLENS_STATUS_TOKEN"),
    );
    return 2;
  }
  const opened = openBroker({ databasePath });
  if (!opened.ok) {
    write(JSON.stringify({ protocol: "lens/1", ...opened }));
    return 1;
  }
  const vault = openSqliteAdapterSessionVault(databasePath);
  if (!vault.ok) {
    opened.value.close();
    write(JSON.stringify({ protocol: "lens/1", ...vault }));
    return 1;
  }
  const host = environment["CODLENS_BIND_HOST"] ?? "127.0.0.1";
  const port = Number(environment["CODLENS_PORT"] ?? "32191");
  const gateway = createLensHttpGateway({
    broker: opened.value,
    allowedHosts: split(environment["CODLENS_ALLOWED_HOSTS"] ?? `${host}:${String(port)}`),
    allowedOrigins: split(environment["CODLENS_ALLOWED_ORIGINS"] ?? "http://127.0.0.1"),
    statusToken: statusToken.value,
    adapterSessionIdFactory: () => `adapter.${randomUUID().replaceAll("-", "")}`,
    adapterSessionVault: vault.value,
  });
  const started = await gateway.start({ host, port });
  if (!started.ok) {
    opened.value.close();
    vault.value.close();
    write(JSON.stringify({ protocol: "lens/1", ...started }));
    return 1;
  }
  write(JSON.stringify({ protocol: "lens/1", ok: true, address: started.value }));
  return 0;
}

function setup(
  environment: NodeJS.ProcessEnv,
  rawInput: string | undefined,
  write: (line: string) => void,
): number {
  const databasePath = environment["CODLENS_DATABASE_PATH"];
  const credentialPath = environment["CODLENS_CREDENTIAL_FILE"];
  const input = jsonSchema(SetupInputSchema, rawInput);
  if (databasePath === undefined || credentialPath === undefined || !input.ok) {
    write(configurationError("setup requires database path, credential file and valid scope JSON"));
    return 2;
  }
  const opened = openBroker({ databasePath });
  if (!opened.ok) {
    write(JSON.stringify({ protocol: "lens/1", ...opened }));
    return 1;
  }
  const agentInput = EnrollPrincipalInputSchema.parse({
    kind: "agent",
    workspaceIds: [input.value.workspaceId],
    conversationIds: [input.value.conversationId],
    capabilities: [
      "message:emit",
      "question:ask",
      "inbox:read",
      "inbox:ack",
      "inbox:forward",
      "actor:delegate",
      "actor:expire",
      "plan:propose",
    ],
  });
  const humanInput = EnrollPrincipalInputSchema.parse({
    kind: "human_responder",
    workspaceIds: [input.value.workspaceId],
    conversationIds: [input.value.conversationId],
    capabilities: ["message:emit", "question:answer", "question:amend", "events:read"],
  });
  const agent = opened.value.enrollPrincipal(agentInput);
  const human = opened.value.enrollPrincipal(humanInput);
  opened.value.close();
  try {
    protectCredentialFile(databasePath);
  } catch {
    write(configurationError("broker database could not be restricted to the current user"));
    return 1;
  }
  if (!agent.ok || !human.ok) {
    const failed = agent.ok ? human : agent;
    write(JSON.stringify({ protocol: "lens/1", ...failed }));
    return 1;
  }
  const credentials = CredentialFileSchema.parse({
    protocol: "lens/1",
    statusToken: `status.${randomUUID().replaceAll("-", "")}`,
    agent: agent.value,
    humanResponder: human.value,
  });
  try {
    writeFileSync(credentialPath, JSON.stringify(credentials), {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    });
    protectCredentialFile(credentialPath);
  } catch {
    write(configurationError("credential file could not be created with exclusive user access"));
    return 1;
  }
  write(
    JSON.stringify({
      protocol: "lens/1",
      ok: true,
      credentialFile: credentialPath,
      agentPrincipalId: agent.value.principalId,
      humanResponderPrincipalId: human.value.principalId,
    }),
  );
  return 0;
}

async function request(
  environment: NodeJS.ProcessEnv,
  method: "GET" | "POST",
  path: string,
  body: string | undefined,
  credentialKind: "status" | "session" | "principal",
  write: (line: string) => void,
): Promise<number> {
  const baseUrl = environment["CODLENS_URL"] ?? "http://127.0.0.1:32191";
  const variable = credentialKind === "status" ? "CODLENS_STATUS_TOKEN" : "CODLENS_PRINCIPAL_TOKEN";
  const adapterSession =
    credentialKind === "session" ? environment["CODLENS_ADAPTER_SESSION_ID"] : undefined;
  const stored = credentialValue(
    environment,
    credentialKind === "principal" && path === "/v1/answer"
      ? "human"
      : credentialKind === "status"
        ? "status"
        : "principal",
  );
  const token = environment[variable] ?? (stored.ok ? stored.value : undefined);
  if (
    token === undefined ||
    (credentialKind === "session" && adapterSession === undefined) ||
    (method === "POST" && body === undefined)
  ) {
    write(configurationError(`${variable} and command JSON are required`));
    return 2;
  }
  try {
    const response = await fetch(new URL(path, baseUrl), {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        ...(adapterSession === undefined ? {} : { "X-Codlens-Adapter-Session": adapterSession }),
        ...(method === "POST" ? { "Content-Type": "application/json" } : {}),
      },
      ...(body === undefined ? {} : { body }),
    });
    write(await response.text());
    return response.ok ? 0 : 1;
  } catch {
    write(configurationError("HTTP command could not reach the configured local broker"));
    return 1;
  }
}

function credential(value: string | undefined) {
  const parsed = CredentialSchema.safeParse(value);
  return parsed.success ? { ok: true as const, value: parsed.data } : { ok: false as const };
}

function credentialValue(
  environment: NodeJS.ProcessEnv,
  kind: "status" | "session" | "principal" | "human",
) {
  if (kind === "session") return credential(environment["CODLENS_ADAPTER_SESSION_ID"]);
  const direct =
    kind === "status"
      ? environment["CODLENS_STATUS_TOKEN"]
      : environment[kind === "human" ? "CODLENS_HUMAN_PRINCIPAL_TOKEN" : "CODLENS_PRINCIPAL_TOKEN"];
  if (direct !== undefined) return credential(direct);
  const path = environment["CODLENS_CREDENTIAL_FILE"];
  if (path === undefined) return credential(undefined);
  try {
    const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
    const parsed = CredentialFileSchema.safeParse(raw);
    if (!parsed.success) return credential(undefined);
    if (kind === "status") return { ok: true as const, value: parsed.data.statusToken };
    return {
      ok: true as const,
      value:
        kind === "human"
          ? parsed.data.humanResponder.principalToken
          : parsed.data.agent.principalToken,
    };
  } catch {
    return credential(undefined);
  }
}

function jsonSchema<T>(schema: z.ZodType<T>, value: string | undefined) {
  if (value === undefined) return { ok: false as const };
  try {
    const raw: unknown = JSON.parse(value);
    const parsed = schema.safeParse(raw);
    return parsed.success ? { ok: true as const, value: parsed.data } : { ok: false as const };
  } catch {
    return { ok: false as const };
  }
}

function protectCredentialFile(path: string): void {
  chmodSync(path, 0o600);
  if (process.platform !== "win32") return;
  const acl = spawnSync(
    "icacls.exe",
    [path, "/inheritance:r", "/grant:r", `${userInfo().username}:(R,W)`],
    { windowsHide: true, stdio: "ignore" },
  );
  if (acl.status !== 0) {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: credential ACL failed; fix surface: grant only the current user read/write access",
    );
  }
}

function split(value: string): string[] {
  return value
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

function configurationError(why: string): string {
  return JSON.stringify({
    protocol: "lens/1",
    ok: false,
    error: {
      code: "invalid_input",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-001#transport: ${why}; fix surface: set the entry-root environment without printing credentials`,
    },
  });
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await runCli(process.argv.slice(2), process.env, console.log);
}
