/** Durable addressed managed-worker wake queue. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import { z } from "zod";
import type {
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import { ActorIdSchema } from "../protocol/index.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";
import {
  ManagedWakeClaimSchema,
  ManagedWakeDeliverySchema,
  ManagedWakeNoticeSchema,
  ManagedWakeSettlementSchema,
  type ManagedWakeClaim,
  type ManagedWakeDelivery,
  type ManagedWakeNotice,
  type ManagedWakeSettlement,
} from "./types.ts";

const RowSchema = z.object({ public_json: z.string() });
const ActorRowSchema = z.object({ actor_id: z.string() });

export function queueManagedWake(
  state: WorkspaceState,
  raw: ManagedWakeNotice,
): WorkspaceResult<ManagedWakeNotice> {
  const parsed = ManagedWakeNoticeSchema.safeParse(raw);
  if (!parsed.success) return failure("invalid_input", "managed wake is malformed");
  try {
    return state.database.transaction(() => {
      const existing = state.database.get(
        `SELECT public_json FROM workspace_managed_wakes WHERE project_id = ? AND context_id = ?
         AND actor_id = ? AND source_event_id = ?`,
        RowSchema,
        [
          parsed.data.projectId,
          parsed.data.contextId,
          parsed.data.actorId,
          parsed.data.sourceEventId,
        ],
      );
      if (existing !== null)
        return { ok: true, value: state.parse(existing.public_json, ManagedWakeNoticeSchema) };
      state.database.run(
        `INSERT INTO workspace_managed_wakes(
          wake_id, project_id, context_id, actor_id, run_id, attempt_id,
          adapter_session_id, process_epoch, source_event_id, state, public_json
        ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        [
          parsed.data.wakeId,
          parsed.data.projectId,
          parsed.data.contextId,
          parsed.data.actorId,
          parsed.data.runId,
          parsed.data.attemptId,
          parsed.data.adapterSessionId,
          "",
          parsed.data.sourceEventId,
          parsed.data.state,
          state.json(parsed.data),
        ],
      );
      return { ok: true, value: parsed.data };
    });
  } catch {
    return failure("storage_failure", "managed wake queue write failed");
  }
}

export function nextManagedWake(
  state: WorkspaceState,
  projectId: z.infer<typeof ProjectIdSchema>,
  contextId: z.infer<typeof WorkContextIdSchema>,
  actorId: z.infer<typeof ActorIdSchema>,
): WorkspaceResult<ManagedWakeNotice | null> {
  try {
    const row = state.database.get(
      `SELECT public_json FROM workspace_managed_wakes
       WHERE project_id = ? AND context_id = ? AND actor_id = ? AND state = 'queued'
       ORDER BY rowid LIMIT 1`,
      RowSchema,
      [projectId, contextId, actorId],
    );
    return {
      ok: true,
      value: row === null ? null : state.parse(row.public_json, ManagedWakeNoticeSchema),
    };
  } catch {
    return failure("storage_failure", "managed wake queue read failed");
  }
}

export function queuedManagedWakeActors(
  state: WorkspaceState,
  projectId: z.infer<typeof ProjectIdSchema>,
  contextId: z.infer<typeof WorkContextIdSchema>,
): WorkspaceResult<readonly z.infer<typeof ActorIdSchema>[]> {
  try {
    const rows = state.database.all(
      `SELECT DISTINCT actor_id FROM workspace_managed_wakes
       WHERE project_id = ? AND context_id = ? AND state = 'queued' ORDER BY actor_id`,
      ActorRowSchema,
      [projectId, contextId],
    );
    return { ok: true, value: rows.map((row) => ActorIdSchema.parse(row.actor_id)) };
  } catch {
    return failure("storage_failure", "managed wake actor list failed");
  }
}

export function claimManagedWake(
  state: WorkspaceState,
  raw: ManagedWakeClaim,
): WorkspaceResult<ManagedWakeDelivery> {
  return transitionManagedWake(state, raw, "queued", "offered");
}

export function releaseManagedWake(
  state: WorkspaceState,
  raw: ManagedWakeClaim,
): WorkspaceResult<ManagedWakeDelivery> {
  return transitionManagedWake(state, raw, "offered", "queued");
}

export function settleManagedWake(
  state: WorkspaceState,
  raw: ManagedWakeSettlement,
): WorkspaceResult<ManagedWakeDelivery> {
  const parsed = ManagedWakeSettlementSchema.safeParse(raw);
  if (!parsed.success) return failure("invalid_input", "managed wake settlement is malformed");
  const next =
    parsed.data.observation === "host_accepted" ? "host_accepted" : parsed.data.observation;
  const claim = ManagedWakeClaimSchema.parse({
    wakeId: parsed.data.wakeId,
    projectId: parsed.data.projectId,
    contextId: parsed.data.contextId,
    actorId: parsed.data.actorId,
    runId: parsed.data.runId,
    attemptId: parsed.data.attemptId,
    adapterSessionId: parsed.data.adapterSessionId,
    processEpoch: parsed.data.processEpoch,
    leaseId: parsed.data.leaseId,
  });
  return transitionManagedWake(state, claim, "offered", next);
}

function transitionManagedWake(
  state: WorkspaceState,
  raw: ManagedWakeClaim,
  expected: ManagedWakeNotice["state"],
  next: ManagedWakeNotice["state"],
): WorkspaceResult<ManagedWakeDelivery> {
  const parsed = ManagedWakeClaimSchema.safeParse(raw);
  if (!parsed.success) return failure("invalid_input", "managed wake claim is malformed");
  try {
    return state.database.transaction(() => {
      const row = state.database.get(
        "SELECT public_json FROM workspace_managed_wakes WHERE wake_id = ?",
        RowSchema,
        [parsed.data.wakeId],
      );
      if (row === null) return failure("not_found", "managed wake does not exist");
      const current = state.parse(row.public_json, ManagedWakeNoticeSchema);
      if (
        current.projectId !== parsed.data.projectId ||
        current.contextId !== parsed.data.contextId ||
        current.actorId !== parsed.data.actorId ||
        current.runId !== parsed.data.runId ||
        current.attemptId !== parsed.data.attemptId ||
        current.adapterSessionId !== parsed.data.adapterSessionId ||
        current.state !== expected
      )
        return failure("conflict", "managed wake target or state is stale");
      const updated = ManagedWakeNoticeSchema.parse({
        ...current,
        state: next,
        updatedAt: state.now(),
      });
      state.database.run(
        "UPDATE workspace_managed_wakes SET state = ?, process_epoch = ?, public_json = ? WHERE wake_id = ?",
        [updated.state, parsed.data.processEpoch, state.json(updated), updated.wakeId],
      );
      return {
        ok: true,
        value: ManagedWakeDeliverySchema.parse({
          ...updated,
          processEpoch: parsed.data.processEpoch,
          leaseId: parsed.data.leaseId,
        }),
      };
    });
  } catch {
    return failure("storage_failure", "managed wake transition failed");
  }
}
