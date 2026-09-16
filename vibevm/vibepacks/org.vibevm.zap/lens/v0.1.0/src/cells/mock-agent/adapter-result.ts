/** ZapMock adapter result construction. @scope spec://org.vibevm.zap/lens/PROP-013#agent */
import {
  CoordinatorSessionDescriptorSchema,
  type AgentRuntimeResult,
  type CoordinatorCapabilities,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
} from "../agent-runtime/index.ts";
import { NativeRefSchema, type ExecutionHostIdSchema } from "../workspace-model/index.ts";
import { ZAP_MOCK_MODEL_ID } from "../mock-model/index.ts";

export function mockDescriptor(
  input: CoordinatorStartInput | CoordinatorResumeInput,
  hostId: ReturnType<typeof ExecutionHostIdSchema.parse>,
  epoch: string,
  bootstrap: "submitted" | "not_observed",
  state: CoordinatorSessionDescriptor["state"],
  capabilities: CoordinatorCapabilities,
  nativeThreadId?: string,
): CoordinatorSessionDescriptor {
  return CoordinatorSessionDescriptorSchema.parse({
    coordinatorSessionId: input.coordinatorSessionId,
    projectId: input.projectId,
    contextId: input.contextId,
    conversationId: input.conversationId,
    coordinatorActorId: input.coordinatorActorId,
    hostId,
    profileId: input.profileId,
    productId: "zap-mock",
    role: "coordinator",
    launchOrigin: "lens",
    interactionKind: "structured",
    state,
    nativeThreadRef: NativeRefSchema.parse({
      namespace: "zap-mock.thread",
      value:
        nativeThreadId ??
        `thread.${input.projectId}.${input.contextId}.${input.coordinatorSessionId}`,
      incarnation: "1",
    }),
    nativeSessionId: `mock.session.${input.coordinatorSessionId}`,
    cwd: input.cwd,
    processEpoch: epoch,
    bootstrap,
    instructionSources: [ZAP_MOCK_MODEL_ID],
    capabilities,
  });
}

export function mockFailure<T>(
  code:
    | "invalid_input"
    | "not_found"
    | "already_exists"
    | "busy"
    | "stale_epoch"
    | "transport_lost"
    | "protocol_error"
    | "unsupported",
  message: string,
): AgentRuntimeResult<T> {
  return {
    ok: false,
    error: { code, message, retry: code === "stale_epoch" ? "after_reconcile" : "never" },
  };
}
