/** @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { WorkspaceError, WorkspaceResult } from "../workspace-model/index.ts";

export function workspaceFailure(
  code: WorkspaceError["code"],
  detail: string,
): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: ${detail}`,
    },
  };
}

export function runtimeFailure(code: string, detail: string): WorkspaceResult<never> {
  if (code === "policy_denied") return workspaceFailure("forbidden", detail);
  if (code === "not_found") return workspaceFailure("not_found", detail);
  if (code === "unsupported") return workspaceFailure("unsupported_operation", detail);
  if (code === "busy") return workspaceFailure("conflict", detail);
  return workspaceFailure("unavailable", detail);
}
