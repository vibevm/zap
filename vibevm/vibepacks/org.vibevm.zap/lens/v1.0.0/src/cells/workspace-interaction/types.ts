/** Shared interaction service contracts. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import type { CoordinatorAdapter, CoordinatorEvent } from "../agent-runtime/index.ts";
import {
  ClientRequestIdSchema,
  type MessageId,
  type PublicConnection,
  type Result,
} from "../protocol/index.ts";
import { QuestionGroupDraftSchema } from "../workspace-model/index.ts";
import { z } from "zod";
import type {
  AgentSessionId,
  AgentQuestionBinding,
  NativeApprovalRequest,
  ProjectId,
  QuestionGroup,
  WorkContextId,
  WorkspaceAccessContext,
  WorkspaceCommandRequest,
  WorkspaceCommandResponse,
  WorkspaceError,
  QuestionAnswerVersion,
} from "../workspace-model/index.ts";

export type InteractionResult<T> = Result<T, WorkspaceError>;

export interface ObservedInteractionScope {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly conversationId: PublicConnection["actor"]["conversationId"];
  readonly originActorId: PublicConnection["actor"]["actorId"] | null;
  readonly event: CoordinatorEvent;
}

export interface NativeInteractionDispatch {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly coordinatorSessionId: AgentSessionId;
  readonly adapter: CoordinatorAdapter;
  readonly processEpoch: string;
  readonly executionEnabled: boolean;
}

export interface WorkspaceInteractionFeature {
  observeNativeRequest(
    scope: ObservedInteractionScope,
  ): InteractionResult<QuestionGroup | NativeApprovalRequest | null>;
  afterQuestionCommand(
    access: WorkspaceAccessContext,
    request: WorkspaceCommandRequest,
    response: WorkspaceCommandResponse,
    dispatch: NativeInteractionDispatch | null,
  ): Promise<InteractionResult<null>>;
  respondToApproval(
    access: WorkspaceAccessContext,
    request: Extract<WorkspaceCommandRequest, { operation: "native-approval.respond.v1" }>,
    dispatch: NativeInteractionDispatch | null,
  ): Promise<InteractionResult<WorkspaceCommandResponse>>;
  drain(dispatch: NativeInteractionDispatch): Promise<InteractionResult<null>>;
  observeResolved(scope: ObservedInteractionScope): InteractionResult<null>;
}

export interface AgentQuestionPublisher {
  publish(actor: PublicConnection, input: AgentQuestionInput): Promise<Result<QuestionGroup>>;
}

export interface AgentAnswerDeliveryPort {
  deliver(input: {
    readonly binding: AgentQuestionBinding;
    readonly answer: QuestionAnswerVersion;
  }): Promise<
    InteractionResult<{ readonly observation: "persisted"; readonly messageId: MessageId }>
  >;
}

export const AgentQuestionInputSchema = z
  .object({ clientRequestId: ClientRequestIdSchema, draft: QuestionGroupDraftSchema })
  .strict();
export type AgentQuestionInput = z.infer<typeof AgentQuestionInputSchema>;

export function interactionFailure(
  code: WorkspaceError["code"],
  why: string,
): InteractionResult<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#question-routing: ${why}`,
    },
  };
}
