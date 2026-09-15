/** Codex adapter test profile. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import { resolve } from "node:path";
import type { JsonValue } from "../protocol/index.ts";

export function codexProfileFixture() {
  return {
    profileId: "profile.codex",
    executablePath: resolve("codex.exe"),
    requestTimeoutMs: 5_000,
    model: "gpt-test",
    effort: "low" as const,
    approvalPolicy: "on-request" as const,
    sandbox: "workspace-write" as const,
    personality: "pragmatic" as const,
    serviceName: "quicklens-test",
    lensMcp: {
      serverName: "codlens",
      commandPath: resolve("codlens.exe"),
      args: ["mcp", "serve"],
      brokerUrl: "http://127.0.0.1:32191",
      credentialFile: resolve("fixture-credentials.json"),
      communicationPreauthorization: { allowDelegation: false },
    },
  };
}

export function codexCollabFixture(
  id: string,
  tool: string,
  receiverThreadIds: string[],
): JsonValue {
  return {
    id,
    type: "collabAgentToolCall",
    tool,
    status: "completed",
    senderThreadId: "thread-root",
    receiverThreadIds,
    agentsStates: {},
  };
}
