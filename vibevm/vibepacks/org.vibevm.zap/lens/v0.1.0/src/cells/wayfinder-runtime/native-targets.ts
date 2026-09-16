/** Explicit native-work target delivery. @scope spec://org.vibevm.zap/lens/PROP-011#deferred-instructions */
import type { WorkPacket } from "../agent-runtime/index.ts";
import type {
  ManagedWorkClaim,
  ManagedWorkResult,
  WorkAttachmentPort,
} from "../managed-work/index.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";

export interface DeclaredNativeWorkTargetBridge {
  prepare(input: {
    readonly access: WorkspaceAccessContext;
    readonly attemptId: ManagedWorkClaim["attemptId"];
    readonly recipientActorId: ManagedWorkClaim["actorId"];
    readonly packet: Pick<WorkPacket, "targetRefs" | "sourceBasisRef" | "planRevision">;
  }): Promise<
    ManagedWorkResult<{
      readonly state: "ready" | "waiting_for_target";
      readonly instructions: readonly {
        readonly attachmentId: string;
        readonly version: string;
        readonly bodyMarkdown: string;
      }[];
    }>
  >;
  acknowledge(input: {
    readonly access: WorkspaceAccessContext;
    readonly attemptId: ManagedWorkClaim["attemptId"];
    readonly attachmentId: string;
    readonly version: string;
  }): Promise<ManagedWorkResult<null>>;
}

export function createDeclaredNativeWorkTargetBridge(
  attachments: WorkAttachmentPort,
): DeclaredNativeWorkTargetBridge {
  return {
    async prepare(input) {
      if (input.packet.targetRefs.length === 0)
        return { ok: true, value: { state: "waiting_for_target", instructions: [] } };
      return attachments.prepareBeforeWork({
        access: input.access,
        attemptId: input.attemptId,
        recipientActorId: input.recipientActorId,
        targets: input.packet.targetRefs,
        sourceBasisRef: input.packet.sourceBasisRef,
        planRevision: input.packet.planRevision,
      });
    },
    acknowledge: (input) => attachments.acknowledge(input),
  };
}
