/** Workspace interaction store delegation. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import * as agentQuestions from "./agent-questions.ts";
import * as interactions from "./interactions.ts";
import type { WorkspaceState } from "./state.ts";
import type {
  AgentAnswerSettlement,
  AgentQuestionRecordInput,
  NativeApprovalRecordInput,
  NativeQuestionRecordInput,
  NativeResponsePreparation,
  NativeResponseSettlement,
  WorkspaceStore,
} from "./types.ts";

export abstract class WorkspaceInteractionStoreFacade {
  protected abstract interactionState(): WorkspaceState;

  resolveAgentScope(
    workspaceId: Parameters<WorkspaceStore["resolveAgentScope"]>[0],
    conversationId: Parameters<WorkspaceStore["resolveAgentScope"]>[1],
  ) {
    return interactions.resolveAgentScope(this.interactionState(), workspaceId, conversationId);
  }
  recordNativeQuestion(input: NativeQuestionRecordInput) {
    return interactions.recordNativeQuestion(this.interactionState(), input);
  }
  recordNativeApproval(input: NativeApprovalRecordInput) {
    return interactions.recordNativeApproval(this.interactionState(), input);
  }
  readNativeInteractionForQuestion(
    questionGroupId: Parameters<WorkspaceStore["readNativeInteractionForQuestion"]>[0],
  ) {
    return interactions.readNativeInteractionForQuestion(this.interactionState(), questionGroupId);
  }
  prepareNativeResponse(input: NativeResponsePreparation) {
    return interactions.prepareNativeResponse(this.interactionState(), input);
  }
  settleNativeResponse(input: NativeResponseSettlement) {
    return interactions.settleNativeResponse(this.interactionState(), input);
  }
  pendingNativeResponses(projectId: ProjectId, contextId: WorkContextId, limit: number) {
    return interactions.pendingNativeResponses(
      this.interactionState(),
      projectId,
      contextId,
      limit,
    );
  }
  recordAgentQuestion(input: AgentQuestionRecordInput) {
    return agentQuestions.recordAgentQuestion(this.interactionState(), input);
  }
  readAgentQuestionBinding(
    questionGroupId: Parameters<WorkspaceStore["readAgentQuestionBinding"]>[0],
  ) {
    return agentQuestions.readAgentQuestionBinding(this.interactionState(), questionGroupId);
  }
  settleAgentAnswer(input: AgentAnswerSettlement) {
    return agentQuestions.settleAgentAnswer(this.interactionState(), input);
  }
}
