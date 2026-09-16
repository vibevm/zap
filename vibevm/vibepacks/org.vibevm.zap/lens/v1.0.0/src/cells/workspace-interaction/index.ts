/** Native and rich interaction orchestration. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
export { createWorkspaceInteractionFeature } from "./service.ts";
export { nativeQuestionResponse, projectNativeRequest } from "./native.ts";
export type {
  AgentQuestionPublisher,
  AgentAnswerDeliveryPort,
  InteractionResult,
  NativeInteractionDispatch,
  ObservedInteractionScope,
  WorkspaceInteractionFeature,
} from "./types.ts";
export { AgentQuestionInputSchema, interactionFailure } from "./types.ts";
export type { AgentQuestionInput } from "./types.ts";
