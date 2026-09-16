/**
 * Trusted agent plan-authoring seam over public ZAP operations.
 * @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#root
 */
export { createPlanAuthoring } from "./service.ts";
export {
  AuthoringContextSchema,
  EconomicsContextViewSchema,
  MetadataReceiptSchema,
  MilestonePrecursorAuthoringInputSchema,
  PreparedAdmissionStepSchema,
  PreparedAssessmentProposalSchema,
  PreparedCommandSchema,
  PreparedCompositePlanProposalSchema,
  PreparedMilestonePrecursorsSchema,
  PreparedPlanProposalSchema,
  PreparedSuccessorSchema,
  SuccessorPlanAuthoringInputSchema,
} from "./schemas.ts";
export type {
  AuthoringContext,
  PlanAuthoringError,
  PlanAuthoringOptions,
  PlanAuthoringPort,
  PlanAuthoringResult,
  MetadataReceipt,
  MilestonePrecursorAuthoringInput,
  PrepareEffectInput,
  PreparedAdmissionStep,
  PreparedAssessmentProposal,
  PreparedCommand,
  PreparedCompositePlanProposal,
  PreparedMilestonePrecursors,
  PreparedPlanProposal,
  PreparedSuccessor,
  SuccessorPlanAuthoringInput,
} from "./types.ts";
