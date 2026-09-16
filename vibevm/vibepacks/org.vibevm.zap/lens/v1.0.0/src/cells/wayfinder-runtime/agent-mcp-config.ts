/** Protected per-run MCP files. @scope spec://org.vibevm.zap/lens/PROP-010#credential-safety */
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { zapPreauthorizedToolNames } from "../protocol/index.ts";

export function writeManagedMcpConfig(input: {
  readonly basePath: string;
  readonly runId: string;
  readonly provider: "codex" | "claude_code" | "opencode" | "qwen_code" | "zap_mock";
  readonly commandPath: string;
  readonly args: readonly string[];
  readonly brokerUrl: string;
  readonly credential: string;
  readonly adapterSessionId: string;
  readonly workspaceId: string;
  readonly conversationId: string;
}) {
  try {
    const stem = input.basePath + "." + digest(input.runId);
    const configPath = stem + ".json";
    const credentialPath = stem + ".credential.json";
    const env = {
      CODLENS_URL: input.brokerUrl,
      CODLENS_CREDENTIAL_FILE: credentialPath,
      CODLENS_ADAPTER_SESSION_ID: input.adapterSessionId,
      CODLENS_WORKSPACE_ID: input.workspaceId,
      CODLENS_CONVERSATION_ID: input.conversationId,
    };
    mkdirSync(dirname(configPath), { recursive: true });
    writeFileSync(
      credentialPath,
      JSON.stringify({ protocol: "lens/1", agent: { principalToken: input.credential } }),
      { encoding: "utf8", mode: 0o600 },
    );
    const server = { command: input.commandPath, args: [...input.args], env };
    const providerConfig =
      input.provider === "opencode"
        ? {
            mcp: {
              "zap-wayfinder": {
                type: "local",
                command: [input.commandPath, ...input.args],
                environment: env,
                enabled: true,
              },
            },
            permission: opencodeZapPermissions(),
          }
        : { mcpServers: { "zap-wayfinder": server } };
    writeFileSync(
      configPath,
      JSON.stringify(
        input.provider === "codex" ? { mcp_servers: { "zap-wayfinder": server } } : providerConfig,
        null,
        2,
      ),
      { encoding: "utf8", mode: 0o600 },
    );
    if (input.provider === "codex") {
      const codexHome = stem + ".codex-home";
      mkdirSync(codexHome, { recursive: true });
      writeFileSync(
        codexHome + "/config.toml",
        "[mcp_servers.zap-wayfinder]\n" +
          "command = " +
          JSON.stringify(input.commandPath) +
          "\nargs = " +
          JSON.stringify([...input.args]) +
          "\n[mcp_servers.zap-wayfinder.env]\n" +
          "CODLENS_URL = " +
          JSON.stringify(input.brokerUrl) +
          "\nCODLENS_CREDENTIAL_FILE = " +
          JSON.stringify(credentialPath) +
          "\nCODLENS_ADAPTER_SESSION_ID = " +
          JSON.stringify(input.adapterSessionId) +
          "\nCODLENS_WORKSPACE_ID = " +
          JSON.stringify(input.workspaceId) +
          "\nCODLENS_CONVERSATION_ID = " +
          JSON.stringify(input.conversationId) +
          "\n",
        { encoding: "utf8", mode: 0o600 },
      );
    }
    return { ok: true as const, value: { mcpConfigPath: configPath, environment: env } };
  } catch {
    return managedBindingFailure(
      "unavailable",
      "managed MCP configuration could not be provisioned",
    );
  }
}

function opencodeZapPermissions(): Record<string, "allow" | "ask"> {
  const entries: Array<readonly [string, "allow" | "ask"]> = [["*", "ask"]];
  for (const tool of zapPreauthorizedToolNames(true)) {
    entries.push([`zap-wayfinder_${tool}`, "allow"]);
    entries.push([`zap_wayfinder_${tool}`, "allow"]);
  }
  return Object.fromEntries(entries);
}

export function defaultMcpLaunch(
  commandPath: string | undefined,
  args: readonly string[] | undefined,
) {
  if (commandPath !== undefined)
    return { commandPath, args: args === undefined ? ["mcp", "serve"] : [...args] };
  const distEntry = fileURLToPath(new URL("../../mcp.js", import.meta.url));
  if (existsSync(distEntry)) return { commandPath: process.execPath, args: [distEntry] };
  const sourceEntry = fileURLToPath(new URL("../../mcp.ts", import.meta.url));
  return {
    commandPath: process.execPath,
    args: process.execArgv.includes("--experimental-strip-types")
      ? [sourceEntry]
      : ["--experimental-strip-types", sourceEntry],
  };
}

export function managedBindingFailure(code: "forbidden" | "unavailable", message: string) {
  return { ok: false as const, error: { code, message } };
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
