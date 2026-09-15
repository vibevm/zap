/** Workflow projections. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import {
  PlanOperationResultSchema,
  type PlanBasis,
  type PlanOperationResult,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import type { WorkflowRecord } from "./workflow.ts";

export function recordResult(record: WorkflowRecord): QuicklensResult<PlanOperationResult> {
  if (record.phase === "prepared") return preparedResult(record);
  const state =
    record.phase === "owner_decision_required"
      ? "held"
      : record.phase === "owner_decision_uncertain"
        ? "uncertain"
        : record.phase === "step_ready"
          ? "admitted"
          : record.phase === "completed"
            ? "completed"
            : record.phase === "rejected"
              ? "rejected"
              : "uncertain";
  return operation(
    record.proposal.operationRef,
    state,
    record.message,
    record.currentStep?.executionBasis ?? record.proposal.basis,
  );
}

function preparedResult(record: WorkflowRecord): QuicklensResult<PlanOperationResult> {
  return {
    ok: true,
    value: PlanOperationResultSchema.parse({
      operationRef: record.proposal.operationRef,
      previewRef: record.proposal.previewRef,
      preview: { basis: record.proposal.basis, changes: record.verifiedChanges },
      state: "prepared",
      message: record.message,
      nextBasis: record.proposal.basis,
    }),
  };
}

export function operation(
  operationRef: string,
  state: "requested" | "admitted" | "held" | "completed" | "rejected" | "uncertain",
  message: string,
  basis: PlanBasis,
): QuicklensResult<PlanOperationResult> {
  return {
    ok: true,
    value: PlanOperationResultSchema.parse({
      operationRef,
      previewRef: null,
      preview: null,
      state,
      message,
      nextBasis: basis,
    }),
  };
}

export function sameBasis(left: PlanBasis, right: PlanBasis): boolean {
  return (
    left.storeRef === right.storeRef &&
    left.baseRef === right.baseRef &&
    left.revision === right.revision &&
    left.sourceBasisRef === right.sourceBasisRef
  );
}

export function qerror(
  code: "unavailable" | "invalid_data" | "stale_basis" | "forbidden" | "unsupported" | "uncertain",
  message: string,
): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code,
      message,
      recovery: "Refresh exact plan state and retry through the configured workflow.",
    },
  };
}
