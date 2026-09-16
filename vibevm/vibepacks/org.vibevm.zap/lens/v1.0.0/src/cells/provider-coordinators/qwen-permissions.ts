/** Qwen CLI permission arguments for one generated authenticated Zap MCP server. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { ZAP_MCP_SERVER_NAME, zapPreauthorizedToolNames } from "../protocol/index.ts";

export function qwenZapMcpPermissionArguments(trustedGeneratedConfig: boolean): readonly string[] {
  if (!trustedGeneratedConfig) return [];
  return [
    "--allowed-mcp-server-names",
    ZAP_MCP_SERVER_NAME,
    ...zapPreauthorizedToolNames(true).flatMap((tool) => [
      "--allowed-tools",
      `mcp__${ZAP_MCP_SERVER_NAME}__${tool}`,
    ]),
  ];
}
