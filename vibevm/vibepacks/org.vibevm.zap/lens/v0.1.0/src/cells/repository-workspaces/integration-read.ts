/** Durable integration lookup. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import type { IntegrationAttempt } from "../repository-model/index.ts";
import type { RepositoryWorkspaceResult } from "./contracts.ts";
import { fail, type RepositoryWorkspaceRuntime } from "./runtime.ts";

export function publicIntegration(
  runtime: RepositoryWorkspaceRuntime,
  id: string,
): RepositoryWorkspaceResult<IntegrationAttempt> {
  const integration = runtime.store.getIntegration(id);
  return integration === null
    ? fail("unavailable", "completed integration is unavailable")
    : { ok: true, value: integration };
}
