/** @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#root */
import type { ZodType } from "zod";
import type { SpecificationWatch } from "../specification-watch/index.ts";
import type {
  CanonicalJsonInput,
  PrepareBundleRequest,
  PrepareComparisonRequest,
  PrepareProjectedRecordRequest,
  ReconcileRequest,
  RecordedCompositeSuccessorView,
  SubmissionStatus,
  ZapClient,
  ZapDigest,
  ZapId,
  ZapQueryPage,
} from "../zap-client/index.ts";
import type {
  AuthoringContext,
  PreparedAdmissionStep,
  PreparedAssessmentProposal,
  PreparedCommand,
  PreparedCompositePlanProposal,
  PreparedMilestonePrecursors,
  PreparedPlanProposal,
  PreparedSuccessor,
  SuccessorPlanAuthoringInput,
  MilestonePrecursorAuthoringInput,
} from "./schemas.ts";

export type {
  AuthoringContext,
  PreparedAdmissionStep,
  PreparedAssessmentProposal,
  PreparedCommand,
  PreparedCompositePlanProposal,
  PreparedMilestonePrecursors,
  PreparedPlanProposal,
  PreparedSuccessor,
  SuccessorPlanAuthoringInput,
  MilestonePrecursorAuthoringInput,
};

export type PlanAuthoringErrorCode =
  | "invalid_input"
  | "unavailable"
  | "stale_basis"
  | "economics_context_unavailable"
  | "ambiguous_baseline"
  | "refused"
  | "uncertain";

export interface PlanAuthoringError {
  readonly code: PlanAuthoringErrorCode;
  readonly message: string;
  readonly recovery: string;
}

export type PlanAuthoringResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: PlanAuthoringError };

export interface PrepareEffectInput {
  readonly effectIndex: number;
  readonly completedPrefix: readonly ZapId[];
  readonly decisionId: ZapId | null;
}

export interface PlanAuthoringOptions {
  readonly reader: ZapClient;
  readonly data: ZapClient;
  readonly coordinator: ZapClient;
  readonly specifications: SpecificationWatch;
}

export interface PlanAuthoringPort {
  discover(): Promise<PlanAuthoringResult<AuthoringContext>>;
  query<T>(
    queryId: ZapId,
    input: CanonicalJsonInput,
    schema: ZodType<T>,
  ): Promise<PlanAuthoringResult<ZapQueryPage<T>>>;
  prepareBundle(request: PrepareBundleRequest): ReturnType<ZapClient["prepareBundle"]>;
  prepareComparison(request: PrepareComparisonRequest): ReturnType<ZapClient["prepareComparison"]>;
  prepareProjectedRecord(
    request: PrepareProjectedRecordRequest,
  ): ReturnType<ZapClient["prepareProjectedRecord"]>;
  prepareSuccessor(
    input: SuccessorPlanAuthoringInput,
  ): Promise<PlanAuthoringResult<PreparedPlanProposal>>;
  prepareMilestonePrecursors(
    input: MilestonePrecursorAuthoringInput,
  ): Promise<PlanAuthoringResult<PreparedMilestonePrecursors>>;
  prepareCompositeSuccessor(
    input: SuccessorPlanAuthoringInput,
    precursors: PreparedMilestonePrecursors,
  ): Promise<PlanAuthoringResult<PreparedCompositePlanProposal>>;
  recordCompositeSuccessor(
    prepared: PreparedCompositePlanProposal,
  ): Promise<PlanAuthoringResult<RecordedCompositeSuccessorView>>;
  submitMetadata(command: PreparedCommand): Promise<PlanAuthoringResult<SubmissionStatus>>;
  reconcileMetadata(request: ReconcileRequest): Promise<PlanAuthoringResult<SubmissionStatus>>;
  prepareAssessment(
    prepared: PreparedPlanProposal,
    receipt: MetadataReceipt,
  ): Promise<PlanAuthoringResult<PreparedAssessmentProposal>>;
  prepareCompositeAssessment(
    prepared: PreparedCompositePlanProposal,
    receipt: MetadataReceipt,
  ): Promise<PlanAuthoringResult<PreparedAssessmentProposal>>;
  finishSuccessor(
    prepared: PreparedAssessmentProposal,
    receipt: MetadataReceipt,
  ): Promise<PlanAuthoringResult<PreparedSuccessor>>;
  prepareEffect(
    prepared: PreparedSuccessor,
    input: PrepareEffectInput,
  ): Promise<PlanAuthoringResult<PreparedAdmissionStep>>;
}

export interface MetadataReceipt {
  readonly commandId: ZapId;
  readonly commandDigest: ZapDigest;
  readonly revision: string;
}
