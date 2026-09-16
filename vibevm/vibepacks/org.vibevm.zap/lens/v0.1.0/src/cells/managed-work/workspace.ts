/** Managed worktree preparation and protected launch resolution. @scope spec://org.vibevm.zap/lens/PROP-014#assignment */
import { isAbsolute } from "node:path";
import {
  ManagedWorkspaceAssignmentSchema,
  type ManagedWorkspaceAssignment,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import type { ManagedWorkRequest, ManagedWorkResult } from "./contracts.ts";

export interface ManagedWorkspacePreparationInput {
  readonly access: WorkspaceAccessContext;
  readonly request: ManagedWorkRequest;
  readonly taskId: string;
  readonly runId: string;
  readonly attemptId: string;
  readonly actorId: string;
  readonly parentAssignment: ManagedWorkspaceAssignment | null;
}

export interface ManagedWorkspaceLaunchBinding {
  readonly assignment: ManagedWorkspaceAssignment;
  readonly cwd: string;
  readonly observedHead: string | null;
  readonly dirty: boolean | null;
}

export interface ManagedWorkspaceProvisioningPort {
  prepare(
    input: ManagedWorkspacePreparationInput,
  ): Promise<ManagedWorkResult<ManagedWorkspaceAssignment>>;
  resolveInitialLaunch(input: {
    readonly access: WorkspaceAccessContext;
    readonly attemptId: string;
    readonly assignment: ManagedWorkspaceAssignment;
    readonly legacyProtectedCwd: string;
  }): Promise<ManagedWorkResult<ManagedWorkspaceLaunchBinding>>;
  resolveResume(input: {
    readonly access: WorkspaceAccessContext;
    readonly attemptId: string;
    readonly assignment: ManagedWorkspaceAssignment;
    readonly legacyProtectedCwd: string;
  }): Promise<ManagedWorkResult<ManagedWorkspaceLaunchBinding>>;
}

export function createLegacyManagedWorkspaceProvisioningPort(options: {
  readonly id: () => string;
  readonly now: () => string;
}): ManagedWorkspaceProvisioningPort {
  return {
    prepare(input) {
      if (input.request.planId !== null)
        return Promise.resolve(
          failure("unavailable", "repository workspace provisioning is not configured"),
        );
      return Promise.resolve({
        ok: true,
        value: ManagedWorkspaceAssignmentSchema.parse({
          assignmentId: options.id(),
          kind: "legacy_registered",
          planId: null,
          projectId: null,
          contextId: null,
          repositoryId: null,
          worktreeId: null,
          executionHostId: null,
          basisCommit: null,
          initialHead: null,
          worktreeRevisionAtAssignment: null,
          assignedByPrincipalId: input.access.principalId,
          assignedAt: options.now(),
        }),
      });
    },
    resolveInitialLaunch: (input) => Promise.resolve(legacyBinding(input)),
    resolveResume: (input) => Promise.resolve(legacyBinding(input)),
  };
}

function legacyBinding(input: {
  readonly assignment: ManagedWorkspaceAssignment;
  readonly legacyProtectedCwd: string;
}): ManagedWorkResult<ManagedWorkspaceLaunchBinding> {
  if (input.assignment.kind !== "legacy_registered" || !isAbsolute(input.legacyProtectedCwd))
    return failure("forbidden", "legacy workspace binding is invalid");
  return {
    ok: true,
    value: {
      assignment: input.assignment,
      cwd: input.legacyProtectedCwd,
      observedHead: null,
      dirty: null,
    },
  };
}

function failure(
  code: "invalid_input" | "forbidden" | "conflict" | "unavailable" | "uncertain",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
