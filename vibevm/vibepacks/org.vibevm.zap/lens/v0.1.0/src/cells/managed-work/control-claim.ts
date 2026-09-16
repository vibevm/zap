/** Durable managed-control projection for one work claim.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import { ActorIdSchema } from "../protocol/index.ts";
import type { ManagedWorkClaim } from "./contracts.ts";
import type { ManagedControlTarget, ManagedSessionReadiness } from "./control.ts";
import type { ManagedAgentProfile } from "./providers.ts";

export function managedControlView(
  target: ManagedControlTarget,
  observed: {
    readonly readiness: ManagedSessionReadiness;
    readonly providerSessionId: string | null;
    readonly providerTurnId: string | null;
    readonly observationId: string;
    readonly automationControlEpoch: string | null;
    readonly pauseRequested: boolean;
  },
  provider: ManagedAgentProfile["provider"],
): NonNullable<ManagedWorkClaim["managedControl"]> {
  return {
    processEpoch: target.expectedProcessEpoch,
    readiness: observed.readiness,
    providerSessionId: observed.providerSessionId,
    providerTurnId: observed.providerTurnId,
    observationId: observed.observationId,
    automationControlEpoch: observed.automationControlEpoch ?? "1",
    pauseRequested: observed.pauseRequested,
    continuation: provider === "claude_code" ? "restart_from_saved_session" : "live",
  };
}

export function controlTarget(claim: ManagedWorkClaim): ManagedControlTarget | null {
  return claim.managedControl === null
    ? null
    : {
        runId: claim.runId,
        actorId: ActorIdSchema.parse(claim.actorId),
        sessionId: claim.sessionId,
        terminalId: claim.terminalId,
        expectedProcessEpoch: claim.managedControl.processEpoch,
      };
}
