/** Managed work HTTP client. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import type { ManagedWorkAgentPort } from "../managed-work/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import type { AdapterSessionId } from "../transport/index.ts";
import type { AgentHttpClientOptions } from "./client.ts";
import { executeAgentSessionHttpCommand } from "./client.ts";

export function createManagedWorkHttpClient(options: AgentHttpClientOptions): ManagedWorkAgentPort {
  const call = (path: string, session: AdapterSessionId, input: unknown) =>
    executeAgentSessionHttpCommand(options, path, session, input, JsonValueSchema);
  return {
    profiles: (session) => call("/v1/managed-work/profiles", session, {}),
    create: (session, input) => call("/v1/managed-work/create", session, input),
    start: (session, input) => call("/v1/managed-work/start", session, input),
    read: (session, input) => call("/v1/managed-work/read", session, input),
    report: (session, input) => call("/v1/managed-work/report", session, input),
    acknowledgeAttachment: (session, input) =>
      call("/v1/managed-work/attachment-ack", session, input),
  };
}
