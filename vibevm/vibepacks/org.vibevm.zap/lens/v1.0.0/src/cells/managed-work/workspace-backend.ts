/** Backend joins for managed workspace preparation and launch. @scope spec://org.vibevm.zap/lens/PROP-014#assignment */
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";
import type { ManagedWorkClaim, ManagedWorkRequest, ManagedWorkResult } from "./contracts.ts";
import type { ManagedWorkStore } from "./store.ts";
import type {
  ManagedWorkspaceLaunchBinding,
  ManagedWorkspaceProvisioningPort,
} from "./workspace.ts";

export async function prepareManagedWorkspace(input: {
  readonly workspaces: ManagedWorkspaceProvisioningPort;
  readonly store: ManagedWorkStore;
  readonly access: WorkspaceAccessContext;
  readonly request: ManagedWorkRequest;
  readonly taskId: string;
  readonly runId: string;
  readonly attemptId: string;
  readonly actorId: string;
}) {
  const parent =
    input.request.parentRunId === null ? null : input.store.load(input.request.parentRunId);
  if (parent !== null && !parent.ok) return parent;
  if (
    parent !== null &&
    (parent.value.packet.projectId !== input.request.projectId ||
      parent.value.packet.contextId !== input.request.contextId)
  )
    return failure("forbidden", "parent workspace assignment is outside managed work scope");
  return input.workspaces.prepare({
    access: input.access,
    request: input.request,
    taskId: input.taskId,
    runId: input.runId,
    attemptId: input.attemptId,
    actorId: input.actorId,
    parentAssignment: parent?.value.packet.workspaceAssignment ?? null,
  });
}

export function resolveManagedWorkspace(
  workspaces: ManagedWorkspaceProvisioningPort,
  mode: "initial" | "resume",
  access: WorkspaceAccessContext,
  claim: ManagedWorkClaim,
  legacyProtectedCwd: string,
): Promise<ManagedWorkResult<ManagedWorkspaceLaunchBinding>> {
  const assignment = claim.packet.workspaceAssignment;
  if (assignment === null)
    return Promise.resolve(
      failure("unavailable", "managed work has no durable workspace assignment"),
    );
  const input = {
    access,
    attemptId: claim.attemptId,
    assignment,
    legacyProtectedCwd,
  };
  return mode === "initial"
    ? workspaces.resolveInitialLaunch(input)
    : workspaces.resolveResume(input);
}

function failure(code: "forbidden" | "unavailable", message: string): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
