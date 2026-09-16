/** Repository work HTTP client. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import type { RepositoryWorkspaceAgentPort } from "../managed-work/index.ts";
import { JsonValueSchema } from "../protocol/index.ts";
import type { AdapterSessionId } from "../transport/index.ts";
import type { AgentHttpClientOptions } from "./client.ts";
import { executeAgentSessionHttpCommand } from "./client.ts";

export function createRepositoryWorkHttpClient(
  options: AgentHttpClientOptions,
): RepositoryWorkspaceAgentPort {
  const call = (operation: string, session: AdapterSessionId, input: unknown) =>
    executeAgentSessionHttpCommand(
      options,
      `/v1/repository-work/${operation}`,
      session,
      input,
      JsonValueSchema,
    );
  return {
    planList: (session, input) => call("plan-list", session, input),
    worktreeList: (session, input) => call("worktree-list", session, input),
    worktreeGet: (session, input) => call("worktree-get", session, input),
    integrationList: (session, input) => call("integration-list", session, input),
    integrationGet: (session, input) => call("integration-get", session, input),
    integrationDiff: (session, input) => call("integration-diff", session, input),
    integrationPrepare: (session, input) => call("integration-prepare", session, input),
    integrationTest: (session, input) => call("integration-test", session, input),
  };
}
