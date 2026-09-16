/** Managed PTY reincarnation from an exact saved provider session.
 * @scope spec://org.vibevm.zap/lens/PROP-012#continuation
 */
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";
import type {
  ManagedExecutionFencePort,
  ManagedWorkClaim,
  ManagedWorkResult,
} from "./contracts.ts";
import type { ManagedActorBindingPort } from "./backend.ts";
import type { ManagedControlProvisioner } from "./control.ts";
import type {
  ManagedAgentProfile,
  ManagedProviderDriver,
  ProtectedEnvironmentPort,
} from "./providers.ts";
import { ManagedAgentProfileSchema } from "./providers.ts";
import type { ManagedWorkStore } from "./store.ts";
import { resolveManagedWorkspace } from "./workspace-backend.ts";
import type { ManagedWorkspaceProvisioningPort } from "./workspace.ts";

export async function resumeManagedWork(input: {
  readonly access: WorkspaceAccessContext;
  readonly claim: ManagedWorkClaim;
  readonly expectedRevision: string;
  readonly profile: ManagedAgentProfile;
  readonly driver: ManagedProviderDriver;
  readonly environment: ProtectedEnvironmentPort;
  readonly bindings: ManagedActorBindingPort;
  readonly control: ManagedControlProvisioner;
  readonly terminals: ManagedTerminalServicePort;
  readonly execution: ManagedExecutionFencePort;
  readonly store: ManagedWorkStore;
  readonly workspaces: ManagedWorkspaceProvisioningPort;
  readonly terminalId: ManagedWorkClaim["terminalId"];
}): Promise<ManagedWorkResult<ManagedWorkClaim>> {
  const control = input.claim.managedControl;
  if (
    input.claim.revision !== input.expectedRevision ||
    !["paused", "stopped"].includes(input.claim.state) ||
    control === null
  )
    return fail("conflict", "managed work is not stopped at the expected revision");
  const target = {
    runId: input.claim.runId,
    actorId: ActorIdSchema.parse(input.claim.actorId),
    sessionId: input.claim.sessionId,
    terminalId: input.claim.terminalId,
    expectedProcessEpoch: control.processEpoch,
  };
  const observed = input.control.inspect(target);
  if (
    !observed.ok ||
    observed.value.readiness !== "stopped" ||
    observed.value.providerSessionId === null
  )
    return fail(
      "unavailable",
      "managed conversation has no observed stopped provider session to resume",
    );
  const admitted = input.execution.canStart(input.access, input.claim);
  if (!admitted.ok) return admitted;
  const environment = await input.environment.resolve(input.profile.environmentRef);
  if (!environment.ok) return fail("unavailable", environment.message);
  const activated = await input.bindings.activate({
    runId: input.claim.runId,
    actorId: input.claim.actorId,
    adapterSessionId: input.claim.adapterSessionId,
    mcpConfigPath: input.profile.mcpConfigPath,
    provider: input.profile.provider,
    mcpCommandPath: input.profile.mcpCommandPath,
    mcpArgs: input.profile.mcpArgs,
  });
  if (!activated.ok) return activated;
  const workspace = await resolveManagedWorkspace(
    input.workspaces,
    "resume",
    input.access,
    input.claim,
    input.profile.cwd,
  );
  if (!workspace.ok) return workspace;
  const launch = input.driver.resume({
    profile: ManagedAgentProfileSchema.parse({
      ...input.profile,
      mcpConfigPath: activated.value.mcpConfigPath,
    }),
    workspaceCwd: workspace.value.cwd,
    selection: input.claim.modelSelection,
    providerSessionId: observed.value.providerSessionId,
    environment: { ...environment.value, ...activated.value.environment },
    trustedZapMcp: true,
  });
  const nextTerminalId = input.terminalId;
  const prepared = await input.control.prepare({
    runId: input.claim.runId,
    actorId: ActorIdSchema.parse(input.claim.actorId),
    sessionId: input.claim.sessionId,
    terminalId: nextTerminalId,
    provider: input.claim.provider,
    launch,
  });
  if (!prepared.ok) return prepared;
  const readmitted = input.execution.canStart(input.access, input.claim);
  if (!readmitted.ok) {
    input.control.discard(prepared.value.target);
    return readmitted;
  }
  const launching = transition(input.store, input.claim, "launching", {
    terminalId: nextTerminalId,
    controlLeaseId: null,
    controlEpoch: null,
    managedControl: null,
  });
  if (!launching.ok) {
    input.control.discard(prepared.value.target);
    return launching;
  }
  const started = await input.terminals.start({
    accessProjectId: input.claim.packet.projectId,
    accessContextId: input.claim.packet.contextId,
    spec: {
      terminalId: nextTerminalId,
      projectId: input.claim.packet.projectId,
      contextId: input.claim.packet.contextId,
      sessionId: input.claim.sessionId,
      runId: input.claim.runId,
      executable: prepared.value.launch.executable,
      args: [...prepared.value.launch.args],
      cwd: prepared.value.launch.cwd,
      env: prepared.value.launch.env,
    },
  });
  if (!started.ok) return transition(input.store, launching.value, "uncertain");
  const lease = input.terminals.acquire(input.access, {
    terminalId: nextTerminalId,
    expectedControlEpoch: started.value.controlEpoch,
    takeover: true,
  });
  if (!lease.ok || lease.value.lease === null)
    return transition(input.store, launching.value, "uncertain");
  const bound = await input.control.activate({
    target: prepared.value.target,
    access: input.access,
    leaseId: lease.value.lease.leaseId,
    automationControlEpoch: String(lease.value.lease.controlEpoch),
  });
  if (!bound.ok) return transition(input.store, launching.value, "uncertain");
  const latest = input.control.inspect(prepared.value.target);
  return transition(input.store, launching.value, "running", {
    controlLeaseId: lease.value.lease.leaseId,
    controlEpoch: String(lease.value.lease.controlEpoch),
    managedControl: latest.ok
      ? {
          processEpoch: prepared.value.target.expectedProcessEpoch,
          readiness: latest.value.readiness,
          providerSessionId: latest.value.providerSessionId,
          providerTurnId: latest.value.providerTurnId,
          observationId: latest.value.observationId,
          automationControlEpoch: latest.value.automationControlEpoch ?? "1",
          pauseRequested: latest.value.pauseRequested,
          continuation:
            input.claim.provider === "claude_code" ? "restart_from_saved_session" : "live",
        }
      : null,
  });
}

function transition(
  store: ManagedWorkStore,
  current: ManagedWorkClaim,
  state: ManagedWorkClaim["state"],
  extra: Partial<ManagedWorkClaim> = {},
) {
  const next = {
    ...current,
    ...extra,
    state,
    revision: DecimalSchema.parse(String(BigInt(current.revision) + 1n)),
  };
  return store.transition(current.runId, current.revision, next);
}

function fail(code: "conflict" | "unavailable", message: string): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
