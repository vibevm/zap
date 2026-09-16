/** Typed workspace-store failures. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { WorkspaceError, WorkspaceResult } from "../workspace-model/index.ts";

const REQUIREMENT = "spec://org.vibevm.zap/lens/PROP-005#server-ownership";

export function failure(code: WorkspaceError["code"], detail: string): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ ${REQUIREMENT}: ${detail}`,
    },
  };
}

export function storageFailure(): WorkspaceResult<never> {
  return failure("storage_failure", "workspace persistence operation failed");
}
