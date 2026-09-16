/** Exact owned-writer observation for repository promotion. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import type { ManagedAgentBackend, ManagedWorkClaim } from "../managed-work/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import { PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";

export interface RuntimeRepositoryWriterActivity {
  observe(projectId: string, contextId: string): "idle" | "active" | "unknown";
}

export function createRuntimeRepositoryWriterActivity(input: {
  readonly managedWork: () => ManagedAgentBackend | undefined;
  readonly terminals: () => ManagedTerminalServicePort | undefined;
}): RuntimeRepositoryWriterActivity {
  return {
    observe(projectId, contextId) {
      const access = WorkspaceAccessContextSchema.safeParse({
        principalId: PrincipalIdSchema.parse("principal.repository.writer-observer"),
        actorId: null,
        clientId: ClientIdSchema.parse("client.repository.writer-observer"),
        authorizedProjectIds: [ProjectIdSchema.parse(projectId)],
      });
      const scopedContext = WorkContextIdSchema.safeParse(contextId);
      if (!access.success || !scopedContext.success) return "unknown";
      const backend = input.managedWork();
      const claims = backend?.list(access.data, projectId, scopedContext.data);
      if (claims !== undefined && !claims.ok) return "unknown";
      const knownClaims = claims?.value ?? [];
      const terminals = input.terminals();
      const listed = terminals?.list(access.data, projectId, scopedContext.data);
      if (listed !== undefined && !listed.ok) return "unknown";
      return classifyRepositoryWriterActivity(knownClaims, listed?.value ?? []);
    },
  };
}

export function classifyRepositoryWriterActivity(
  claims: readonly Pick<ManagedWorkClaim, "runId" | "state" | "managedControl">[],
  terminals: readonly {
    readonly runId: string;
    readonly state: "running" | "stopping" | "exited" | "stopped" | "unknown";
  }[],
): "idle" | "active" {
  if (claims.some(activeManagedClaim)) return "active";
  for (const terminal of terminals) {
    if (terminal.state !== "running" && terminal.state !== "stopping") continue;
    const claim = claims.find((candidate) => candidate.runId === terminal.runId);
    if (claim === undefined || activeManagedClaim(claim)) return "active";
  }
  return "idle";
}

function activeManagedClaim(claim: Pick<ManagedWorkClaim, "state" | "managedControl">): boolean {
  if (["prepared", "paused", "reported", "accepted", "stopped", "failed"].includes(claim.state))
    return false;
  if (
    claim.managedControl?.pauseRequested === true &&
    ["idle", "stopped"].includes(claim.managedControl.readiness)
  )
    return false;
  return true;
}
