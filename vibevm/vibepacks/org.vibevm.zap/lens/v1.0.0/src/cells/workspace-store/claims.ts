/** @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import { z } from "zod";
import type { ClientRequestId } from "../protocol/index.ts";
import {
  CoordinatorLaunchClaimInputSchema,
  CoordinatorLaunchClaimSchema,
  CoordinatorLaunchReceiptSchema,
  type CoordinatorLaunchClaim,
  type CoordinatorLaunchClaimInput,
  type CoordinatorLaunchReceipt,
} from "./types.ts";
import { failure } from "./errors.ts";
import type { ProjectId, WorkContextId, WorkspaceResult } from "../workspace-model/index.ts";
import type { WorkspaceState } from "./state.ts";

const ClaimRowSchema = z.object({
  project_id: z.string(),
  context_id: z.string(),
  claim_id: z.string(),
  principal_id: z.string(),
  actor_key: z.string(),
  client_request_id: z.string(),
  profile_id: z.string(),
  interaction_kind: z.string(),
  state: z.string(),
  session_id: z.string().nullable(),
  actor_id: z.string().nullable(),
  native_thread_id: z.string().nullable(),
  process_epoch: z.string().nullable(),
  updated_at: z.string(),
});

function claim(row: z.infer<typeof ClaimRowSchema>): CoordinatorLaunchClaim {
  return CoordinatorLaunchClaimSchema.parse({
    projectId: row.project_id,
    contextId: row.context_id,
    claimId: row.claim_id,
    principalId: row.principal_id,
    actorKey: row.actor_key,
    clientRequestId: row.client_request_id,
    profileId: row.profile_id,
    interactionKind: row.interaction_kind,
    state: row.state,
    sessionId: row.session_id,
    actorId: row.actor_id,
    nativeThreadId: row.native_thread_id,
    processEpoch: row.process_epoch,
    updatedAt: row.updated_at,
  });
}

export function claimCoordinatorLaunch(
  state: WorkspaceState,
  raw: CoordinatorLaunchClaimInput,
): WorkspaceResult<{ claim: CoordinatorLaunchClaim; acquired: boolean }> {
  const input = CoordinatorLaunchClaimInputSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "coordinator launch claim is malformed");
  try {
    return state.database.transaction(() => {
      const row = state.database.get(
        `SELECT project_id, context_id, claim_id, principal_id, actor_key, client_request_id,
           profile_id, interaction_kind, state, session_id, actor_id, native_thread_id,
           process_epoch, updated_at FROM workspace_coordinator_claims WHERE project_id = ? AND context_id = ?`,
        ClaimRowSchema,
        [input.data.projectId, input.data.contextId],
      );
      if (row !== null && row.state !== "failed" && row.state !== "stopped")
        return { ok: true, value: { claim: claim(row), acquired: false } };
      state.database.run(
        `INSERT INTO workspace_coordinator_claims(
           project_id, context_id, claim_id, principal_id, actor_key, client_request_id,
           profile_id, interaction_kind, state, session_id, actor_id, native_thread_id,
           process_epoch, updated_at
         ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, 'starting', NULL, NULL, NULL, NULL, ?)
         ON CONFLICT(project_id, context_id) DO UPDATE SET claim_id = excluded.claim_id,
           principal_id = excluded.principal_id, actor_key = excluded.actor_key,
           client_request_id = excluded.client_request_id, profile_id = excluded.profile_id,
           interaction_kind = excluded.interaction_kind, state = 'starting', session_id = NULL,
           actor_id = NULL, native_thread_id = NULL, process_epoch = NULL, updated_at = excluded.updated_at`,
        [
          input.data.projectId,
          input.data.contextId,
          input.data.claimId,
          input.data.principalId,
          input.data.actorKey,
          input.data.clientRequestId,
          input.data.profileId,
          input.data.interactionKind,
          input.data.updatedAt,
        ],
      );
      const created = state.database.get(
        "SELECT project_id, context_id, claim_id, principal_id, actor_key, client_request_id, profile_id, interaction_kind, state, session_id, actor_id, native_thread_id, process_epoch, updated_at FROM workspace_coordinator_claims WHERE project_id = ? AND context_id = ?",
        ClaimRowSchema,
        [input.data.projectId, input.data.contextId],
      );
      if (created === null)
        return failure("storage_failure", "coordinator claim disappeared after commit");
      return { ok: true, value: { claim: claim(created), acquired: true } };
    });
  } catch {
    return failure("storage_failure", "coordinator launch claim transaction failed");
  }
}

export function recordCoordinatorReceipt(
  state: WorkspaceState,
  raw: CoordinatorLaunchReceipt,
): WorkspaceResult<CoordinatorLaunchClaim> {
  const input = CoordinatorLaunchReceiptSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "coordinator launch receipt is malformed");
  try {
    const row = state.database.get(
      "SELECT project_id, context_id, claim_id, principal_id, actor_key, client_request_id, profile_id, interaction_kind, state, session_id, actor_id, native_thread_id, process_epoch, updated_at FROM workspace_coordinator_claims WHERE project_id = ? AND context_id = ?",
      ClaimRowSchema,
      [input.data.projectId, input.data.contextId],
    );
    if (row === null || row.claim_id !== input.data.claimId)
      return failure("conflict", "coordinator receipt does not match the durable claim");
    state.database.run(
      "UPDATE workspace_coordinator_claims SET state = 'running', session_id = ?, actor_id = ?, native_thread_id = ?, process_epoch = ?, updated_at = ? WHERE project_id = ? AND context_id = ? AND claim_id = ?",
      [
        input.data.sessionId,
        input.data.actorId,
        input.data.nativeThreadId,
        input.data.processEpoch,
        input.data.updatedAt,
        input.data.projectId,
        input.data.contextId,
        input.data.claimId,
      ],
    );
    return {
      ok: true,
      value: claim({
        ...row,
        state: "running",
        session_id: input.data.sessionId,
        actor_id: input.data.actorId,
        native_thread_id: input.data.nativeThreadId,
        process_epoch: input.data.processEpoch,
        updated_at: input.data.updatedAt,
      }),
    };
  } catch {
    return failure("storage_failure", "coordinator receipt write failed");
  }
}

export function readCoordinatorClaim(
  state: WorkspaceState,
  projectId: ProjectId,
  contextId: WorkContextId,
): WorkspaceResult<CoordinatorLaunchClaim | null> {
  try {
    const row = state.database.get(
      "SELECT project_id, context_id, claim_id, principal_id, actor_key, client_request_id, profile_id, interaction_kind, state, session_id, actor_id, native_thread_id, process_epoch, updated_at FROM workspace_coordinator_claims WHERE project_id = ? AND context_id = ?",
      ClaimRowSchema,
      [projectId, contextId],
    );
    return { ok: true, value: row === null ? null : claim(row) };
  } catch {
    return failure("storage_failure", "coordinator claim read failed");
  }
}

export function markCoordinatorClaimState(
  state: WorkspaceState,
  projectId: ProjectId,
  contextId: WorkContextId,
  claimId: ClientRequestId,
  nextState: CoordinatorLaunchClaim["state"],
  updatedAt: string,
): WorkspaceResult<CoordinatorLaunchClaim> {
  try {
    const row = state.database.get(
      "SELECT project_id, context_id, claim_id, principal_id, actor_key, client_request_id, profile_id, interaction_kind, state, session_id, actor_id, native_thread_id, process_epoch, updated_at FROM workspace_coordinator_claims WHERE project_id = ? AND context_id = ?",
      ClaimRowSchema,
      [projectId, contextId],
    );
    if (row === null || row.claim_id !== claimId)
      return failure("conflict", "coordinator claim identity changed");
    state.database.run(
      "UPDATE workspace_coordinator_claims SET state = ?, updated_at = ? WHERE project_id = ? AND context_id = ? AND claim_id = ?",
      [nextState, updatedAt, projectId, contextId, claimId],
    );
    return { ok: true, value: claim({ ...row, state: nextState, updated_at: updatedAt }) };
  } catch {
    return failure("storage_failure", "coordinator claim state update failed");
  }
}
