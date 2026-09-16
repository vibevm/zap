/** Authenticated human Owner continuation. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { randomUUID } from "node:crypto";
import { PlanBasisSchema, type PlanBasis, type QuicklensResult } from "../quicklens-model/index.ts";
import type { Result } from "../protocol/index.ts";
import {
  ZapDigestSchema,
  ZapIdSchema,
  ProtectedSubmissionSchema,
  canonicalQueryInput,
  protectedCommandDigest,
  type SubmissionStatus,
  type ZapClient,
} from "../zap-client/index.ts";
import type { HumanOwnerWorkflowPort, PlanWorkflowStore, WorkflowRecord } from "./workflow.ts";

export interface HumanOwnerWorkflowOptions {
  readonly ownerZap: ZapClient;
  readonly readerZap: ZapClient;
  readonly store: PlanWorkflowStore;
  readonly validateExecution: (
    basis: PlanBasis,
    expectedRevision: string,
  ) => Promise<QuicklensResult<PlanBasis>>;
  readonly idFactory?: () => string;
}

export function createHumanOwnerWorkflow(
  options: HumanOwnerWorkflowOptions,
): HumanOwnerWorkflowPort {
  const ids = options.idFactory ?? (() => randomUUID().replaceAll("-", ""));
  return {
    decide: async (record, input) => {
      const view = record.admissionView;
      if (
        view?.status !== "owner_decision_required" ||
        record.phase !== "owner_decision_required"
      ) {
        return failure("held admission decision context is unavailable");
      }
      const decisionId = ZapIdSchema.parse(`decision.${ids()}`);
      const commandId = ZapIdSchema.parse(`command.${ids()}`);
      const eventId = ZapIdSchema.parse(`event.${ids()}`);
      const binding = view.decision;
      const request = record.currentStep?.advanceRequest ?? record.proposal.admission;
      const command = {
        frame: {
          header: {
            protocol: 1,
            store_id: request.store.store_id,
            campaign_id: request.store.campaign_id,
            base_id: request.store.base_id,
            command_id: commandId,
            event_id: eventId,
            expected_revision: BigInt(view.observed_revision),
            kind: ZapIdSchema.parse("control.change-decision-recorded"),
            causes: [],
            basis: { kind: "not_applicable" as const },
          },
          reason: {
            summary: input.reason,
            evidence: [],
            decision: decisionId,
            change: null,
          },
          payload: canonicalQueryInput({
            decision: {
              decision_id: decisionId,
              assessment_id: request.assessment_id,
              assessment_digest: binding.assessment_digest,
              forecast_id: binding.forecast_id,
              forecast_digest: binding.forecast_digest,
              policy_id: binding.policy_id,
              policy_revision: BigInt(binding.policy_revision),
              recommended_alternative_id: binding.recommended_alternative_id,
              choice: input.choice,
              reason: input.reason,
              effect_fingerprints: binding.effect_fingerprints,
              effect_preflight_digests: binding.effect_preflight_digests,
              revision: BigInt(binding.decision_revision),
            },
          }),
        },
      };
      const digest = ZapDigestSchema.parse(protectedCommandDigest(command));
      const submission = ProtectedSubmissionSchema.parse({
        command,
        reconciliation: { command_id: commandId, command_digest: digest },
      });
      const pending = options.store.transition(
        record.proposal.operationRef,
        record.proposalDigest,
        ["owner_decision_required"],
        {
          ...record,
          phase: "owner_decision_uncertain",
          ["ownerSubmission"]: submission,
          ownerChoice: input.choice,
          message: "Human Owner decision is durably pending exact reconciliation.",
        },
      );
      if (!pending.ok) return pending;
      const submitted = await options.ownerZap.submit("control", submission);
      if (submitted.ok) return finishDecision(options, pending.value, submitted.value, decisionId);
      return submitted.error.kind === "http_refusal"
        ? reject(options, pending.value, `Owner command refused: ${submitted.error.refusal.code}.`)
        : { ok: true, value: pending.value };
    },
    reconcile: async (record) => {
      const submission = record.ownerSubmission;
      if (submission === null || record.phase !== "owner_decision_uncertain") {
        return failure("owner decision has no uncertain command to reconcile");
      }
      const decisionId = submission.command.frame.reason.decision;
      if (decisionId === null) return failure("owner command has no decision identity");
      const status = await options.readerZap.reconcile(submission.reconciliation);
      if (!status.ok || status.value.status === "unknown") return { ok: true, value: record };
      if (status.value.status === "not_committed") {
        const fresh = await options.validateExecution(
          record.currentStep?.executionBasis ?? record.proposal.basis,
          submission.command.frame.header.expected_revision.toString(),
        );
        if (!fresh.ok) return failure(fresh.error.message);
        const submitted = await options.ownerZap.submit("control", submission);
        if (!submitted.ok) {
          return submitted.error.kind === "http_refusal"
            ? reject(options, record, `Owner command refused: ${submitted.error.refusal.code}.`)
            : { ok: true, value: record };
        }
        return finishDecision(options, record, submitted.value, decisionId);
      }
      return finishDecision(options, record, status.value, decisionId);
    },
  };
}

function reject(
  options: HumanOwnerWorkflowOptions,
  record: WorkflowRecord,
  message: string,
): Result<WorkflowRecord> {
  return options.store.transition(
    record.proposal.operationRef,
    record.proposalDigest,
    [record.phase],
    { ...record, phase: "rejected", message },
  );
}

async function finishDecision(
  options: HumanOwnerWorkflowOptions,
  record: WorkflowRecord,
  status: SubmissionStatus,
  decisionId: ReturnType<typeof ZapIdSchema.parse>,
): Promise<Result<WorkflowRecord>> {
  if (status.status !== "committed") return { ok: true, value: record };
  if (record.ownerChoice !== "approve") {
    return options.store.transition(
      record.proposal.operationRef,
      record.proposalDigest,
      ["owner_decision_uncertain"],
      {
        ...record,
        phase: "rejected",
        message: `Owner selected ${record.ownerChoice ?? "no decision"}; prepare a new intent to continue.`,
      },
    );
  }
  const held = record.admissionView;
  if (held?.status !== "owner_decision_required")
    return failure("held decision binding disappeared");
  const current = record.currentStep?.advanceRequest ?? record.proposal.admission;
  const comparison = {
    ...current.comparison,
    at: { kind: "current" as const },
  };
  const prepared = await options.readerZap.prepareComparison(comparison);
  if (!prepared.ok)
    return failure(`post-decision comparison preparation failed: ${prepared.error.kind}`);
  const currentRevision = BigInt(prepared.value.observed_revision);
  const continuation = {
    ...current,
    expected_revision: currentRevision,
    assessment_digest: held.assessment_digest,
    relevant_basis: prepared.value.relevant_basis,
    comparison,
    product: {
      frame: {
        ...current.product.frame,
        header: {
          ...current.product.frame.header,
          expected_revision: currentRevision + 1n,
        },
      },
    },
    decision_id: decisionId,
  };
  const productDigest = ZapDigestSchema.parse(protectedCommandDigest(continuation.product));
  const currentStep =
    record.currentStep === null
      ? null
      : {
          ...record.currentStep,
          executionBasis: PlanBasisSchema.parse({
            ...record.currentStep.executionBasis,
            revision: currentRevision.toString(),
          }),
          advanceRequest: continuation,
          productDigest,
        };
  return options.store.transition(
    record.proposal.operationRef,
    record.proposalDigest,
    ["owner_decision_uncertain"],
    {
      ...record,
      phase: "admission_uncertain",
      continuation,
      currentStep,
      message: "Owner decision committed; admission continuation is durably pending.",
    },
  );
}

function failure(message: string): Result<never> {
  return {
    ok: false,
    error: {
      code: "conflict",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: ${message}; fix surface: reconcile the exact human Owner command and refresh the held operation`,
    },
  };
}
