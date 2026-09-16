/** Workspace-store managed wake delegation. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import * as managedWake from "./managed-wake.ts";
import { WorkspaceInteractionStoreFacade } from "./interaction-facade.ts";
import type { WorkspaceState } from "./state.ts";
import type {
  ManagedWakeClaim,
  ManagedWakeNotice,
  ManagedWakeSettlement,
  WorkspaceStore,
} from "./types.ts";
import { failure } from "./errors.ts";

export abstract class ManagedWakeStoreFacade extends WorkspaceInteractionStoreFacade {
  protected abstract wakeState(): WorkspaceState;
  protected abstract wakeClosed(): boolean;

  queueManagedWake(input: ManagedWakeNotice) {
    if (this.wakeClosed()) return managedWakeFailure("closed", "workspace store is closed");
    return managedWake.queueManagedWake(this.wakeState(), input);
  }
  nextManagedWake(
    projectId: ProjectId,
    contextId: WorkContextId,
    actorId: Parameters<WorkspaceStore["nextManagedWake"]>[2],
  ) {
    if (this.wakeClosed()) return managedWakeFailure("closed", "workspace store is closed");
    return managedWake.nextManagedWake(this.wakeState(), projectId, contextId, actorId);
  }
  queuedManagedWakeActors(projectId: ProjectId, contextId: WorkContextId) {
    if (this.wakeClosed()) return managedWakeFailure("closed", "workspace store is closed");
    return managedWake.queuedManagedWakeActors(this.wakeState(), projectId, contextId);
  }
  claimManagedWake(input: ManagedWakeClaim) {
    if (this.wakeClosed()) return managedWakeFailure("closed", "workspace store is closed");
    return managedWake.claimManagedWake(this.wakeState(), input);
  }
  releaseManagedWake(input: ManagedWakeClaim) {
    if (this.wakeClosed()) return managedWakeFailure("closed", "workspace store is closed");
    return managedWake.releaseManagedWake(this.wakeState(), input);
  }
  settleManagedWake(input: ManagedWakeSettlement) {
    if (this.wakeClosed()) return managedWakeFailure("closed", "workspace store is closed");
    return managedWake.settleManagedWake(this.wakeState(), input);
  }
}

function managedWakeFailure(code: "closed", message: string) {
  return failure(code, message);
}
