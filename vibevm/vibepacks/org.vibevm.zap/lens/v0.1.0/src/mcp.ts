#!/usr/bin/env node
/**
 * Official MCP stdio entry point. Ambient configuration ends here.
 * @scope spec://org.vibevm.zap/lens/PROP-001#transport
 */
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { z } from "zod";
import {
  createAgentHttpClient,
  createManagedWorkHttpClient,
  createNativeWorkHttpClient,
} from "./cells/http/index.ts";
import { createCodlensMcpServer } from "./cells/mcp/index.ts";
import { CredentialSchema } from "./cells/protocol/index.ts";
import { AdapterSessionIdSchema } from "./cells/transport/index.ts";
import { createAgentPlanningHttpClient } from "./cells/workspace-planning/index.ts";

export async function runMcp(environment: NodeJS.ProcessEnv): Promise<number> {
  const brokerUrl = environment["CODLENS_URL"];
  const principal = agentCredential(environment);
  if (brokerUrl === undefined || !principal.success) {
    return 2;
  }
  let baseUrl: URL;
  try {
    baseUrl = new URL(brokerUrl);
  } catch {
    return 2;
  }
  const agent = createAgentHttpClient({ baseUrl, principalToken: principal.data });
  const assignedSession = AdapterSessionIdSchema.safeParse(
    environment["CODLENS_ADAPTER_SESSION_ID"],
  );
  const server = createCodlensMcpServer({
    agent,
    ...(assignedSession.success ? { assignedSession: assignedSession.data } : {}),
    planProposal: createAgentPlanningHttpClient({
      baseUrl,
      principalToken: principal.data,
    }),
    managedWork: createManagedWorkHttpClient({ baseUrl, principalToken: principal.data }),
    nativeWork: createNativeWorkHttpClient({ baseUrl, principalToken: principal.data }),
  });
  await server.connect(new StdioServerTransport());
  return 0;
}

function agentCredential(environment: NodeJS.ProcessEnv) {
  const direct = CredentialSchema.safeParse(environment["CODLENS_PRINCIPAL_TOKEN"]);
  if (direct.success) return direct;
  const path = environment["CODLENS_CREDENTIAL_FILE"];
  if (path === undefined) return direct;
  try {
    const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
    const parsed = z
      .looseObject({
        protocol: z.literal("lens/1"),
        agent: z.looseObject({ principalToken: CredentialSchema }),
      })
      .safeParse(raw);
    return parsed.success ? CredentialSchema.safeParse(parsed.data.agent.principalToken) : direct;
  } catch {
    return direct;
  }
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await runMcp(process.env);
}
