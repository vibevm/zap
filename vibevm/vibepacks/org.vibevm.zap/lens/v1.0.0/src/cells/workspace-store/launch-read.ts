/** Protected project launch lookup. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import { z } from "zod";
import type { WorkspaceStore } from "./types.ts";
import { TrustedProjectLaunchSchema } from "./types.ts";
import { failure } from "./errors.ts";
import type { WorkspaceState } from "./state.ts";

const LaunchRowSchema = z.object({
  protected_cwd: z.string(),
  protected_profile_ref: z.string(),
  workspace_id: z.string().nullable(),
  conversation_id: z.string().nullable(),
});

export function resolveProjectLaunch(
  state: WorkspaceState,
  projectId: Parameters<WorkspaceStore["resolveProjectLaunch"]>[0],
  contextId: Parameters<WorkspaceStore["resolveProjectLaunch"]>[1],
) {
  if (state.closed) return failure("closed", "workspace store is closed");
  try {
    const row = state.database.get(
      `SELECT c.protected_cwd, c.protected_profile_ref, s.workspace_id, s.conversation_id
       FROM workspace_contexts c
       LEFT JOIN workspace_agent_scopes s
         ON s.project_id = c.project_id AND s.context_id = c.context_id
       WHERE c.project_id = ? AND c.context_id = ?`,
      LaunchRowSchema,
      [projectId, contextId],
    );
    return row === null
      ? failure("not_found", "trusted project launch context does not exist")
      : {
          ok: true as const,
          value: TrustedProjectLaunchSchema.parse({
            projectId,
            contextId,
            cwd: row.protected_cwd,
            launchProfileRef: row.protected_profile_ref,
            agentScope:
              row.workspace_id === null || row.conversation_id === null
                ? null
                : { workspaceId: row.workspace_id, conversationId: row.conversation_id },
          }),
        };
  } catch {
    return failure("storage_failure", "trusted project launch read failed");
  }
}
