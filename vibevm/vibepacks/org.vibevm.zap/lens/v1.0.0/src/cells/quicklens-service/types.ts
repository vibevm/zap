/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import type {
  PlanApplyInput,
  PlanDecisionInput,
  PlanOperationResult,
  PlanPreviewInput,
  PlanReconcileInput,
  QuicklensResult,
  QuicklensRef,
} from "../quicklens-model/index.ts";

export interface HeldDecisionView {
  readonly operationRef: QuicklensRef;
  readonly holdRef: QuicklensRef;
}

export interface PlanWorkflowPort {
  readonly available: boolean;
  readonly unavailableReason: string | null;
  preview(input: PlanPreviewInput): Promise<QuicklensResult<PlanOperationResult>>;
  apply(input: PlanApplyInput): Promise<QuicklensResult<PlanOperationResult>>;
  reconcile(input: PlanReconcileInput): Promise<QuicklensResult<PlanOperationResult>>;
  decide(input: PlanDecisionInput): Promise<QuicklensResult<PlanOperationResult>>;
  decision(): QuicklensResult<HeldDecisionView | null>;
}

export function unavailablePlanWorkflow(reason: string): PlanWorkflowPort {
  const unavailable = (): Promise<QuicklensResult<PlanOperationResult>> =>
    Promise.resolve({
      ok: false,
      error: {
        code: "unsupported",
        message: reason,
        recovery: "Configure the public ZAP economics admission workflow, then retry.",
      },
    });
  return {
    available: false,
    unavailableReason: reason,
    preview: unavailable,
    apply: unavailable,
    reconcile: unavailable,
    decide: unavailable,
    decision: () => ({ ok: true, value: null }),
  };
}
