/** Native declared-target HTTP client. @scope spec://org.vibevm.zap/lens/PROP-011#deferred-instructions */
import type { NativeWorkAgentPort } from "../managed-work/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import type { AdapterSessionId } from "../transport/index.ts";
import type { AgentHttpClientOptions } from "./client.ts";
import { executeAgentSessionHttpCommand } from "./client.ts";

export function createNativeWorkHttpClient(options: AgentHttpClientOptions): NativeWorkAgentPort {
  const call = (path: string, session: AdapterSessionId, input: unknown) =>
    executeAgentSessionHttpCommand(options, path, session, input, JsonValueSchema);
  return {
    beforeWork: (session, input) => call("/v1/native-work/before", session, input),
    read: (session, input) => call("/v1/native-work/read", session, input),
    acknowledgeAttachment: (session, input) =>
      call("/v1/native-work/attachment-ack", session, input),
  };
}
