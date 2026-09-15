/** Node-only durable workspace registry and interaction history. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { SqliteWorkspaceStore } from "./store.ts";
import type { OpenWorkspaceStoreOptions, WorkspaceStore } from "./types.ts";
import type { WorkspaceResult } from "../workspace-model/index.ts";
import { storageFailure } from "./errors.ts";

export type {
  OpenWorkspaceStoreOptions,
  CoordinatorLaunchClaim,
  CoordinatorLaunchClaimInput,
  CoordinatorLaunchReceipt,
  ProjectLifecycleSettlement,
  ChatDispatchClaim,
  ChatDispatchSettlement,
  ObservedChatReply,
  ObservedAgentOutput,
  NativeQuestionRecordInput,
  NativeApprovalRecordInput,
  NativeResponsePreparation,
  NativeResponseSettlement,
  AgentAnswerSettlement,
  AgentQuestionRecordInput,
  AgentScopeResolution,
  TrustedProjectLaunch,
  TrustedProjectRegistration,
  WorkspaceStore,
} from "./types.ts";
export {
  CoordinatorLaunchClaimInputSchema,
  CoordinatorLaunchClaimSchema,
  CoordinatorLaunchReceiptSchema,
  ProjectLifecycleSettlementSchema,
  ChatDispatchClaimSchema,
  ChatDispatchSettlementSchema,
  ObservedChatReplySchema,
  ObservedAgentOutputSchema,
  AgentScopeResolutionSchema,
  NativeQuestionRecordInputSchema,
  NativeApprovalRecordInputSchema,
  NativeResponsePreparationSchema,
  NativeResponseSettlementSchema,
  AgentAnswerSettlementSchema,
  AgentQuestionRecordInputSchema,
  TrustedProjectLaunchSchema,
  TrustedProjectRegistrationSchema,
} from "./types.ts";

export function openWorkspaceStore(
  options: OpenWorkspaceStoreOptions,
): WorkspaceResult<WorkspaceStore> {
  try {
    return { ok: true, value: new SqliteWorkspaceStore(options) };
  } catch {
    return storageFailure();
  }
}
