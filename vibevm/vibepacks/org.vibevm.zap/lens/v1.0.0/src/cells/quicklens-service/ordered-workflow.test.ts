/** @verifies spec://org.vibevm.zap/lens/PROP-002#plan-control */
import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  PreparedAdmissionStepSchema,
  PreparedSuccessorSchema,
  type PlanAuthoringPort,
} from "../plan-authoring/index.ts";
import {
  ZapDigestSchema,
  ZapIdSchema,
  canonicalQueryInput,
  protectedCommandDigest,
} from "../zap-client/index.ts";
import {
  connection,
  fixtureBasis,
  fixtureProposal,
  intent,
  workflowZap,
  u64,
} from "./workflow-fixture.test.ts";
import { openSqlitePlanWorkflowStore } from "./workflow-store.ts";
import { AgentPlanProposalSchema, createLivePlanWorkflow } from "./workflow.ts";

test("ordered effects persist their prefix across restart and complete only after the last product", async () => {
  const directory = await mkdtemp(join(tmpdir(), "quicklens-ordered-"));
  const path = join(directory, "workflow.sqlite");
  const scope = { workspaceId: "workspace.workflow", conversationId: "conversation.workflow" };
  const base = fixtureProposal(fixtureBasis());
  const firstAlternative = base.admission.comparison.draft.alternatives[0];
  const firstEffect = firstAlternative?.effects[0];
  assert.ok(firstAlternative && firstEffect);
  const secondEffect = {
    ...firstEffect,
    effect_id: ZapIdSchema.parse("effect.workflow.second"),
    index: 1,
    product_event_id: ZapIdSchema.parse("event.workflow.second"),
  };
  const zap = workflowZap("multi");
  const prepared = PreparedSuccessorSchema.parse({
    intentBasis: base.intentBasis,
    preparedBasis: base.basis,
    operationId: base.admission.operation_id,
    assessmentId: base.admission.assessment_id,
    alternativeId: base.admission.alternative_id,
    sourceAssessmentDigest: base.admission.source_assessment_digest,
    assessmentProposalRevision: u64("7"),
    planPayload: canonicalQueryInput({}),
    effects: [
      {
        effectId: firstEffect.effect_id,
        index: 0,
        kind: firstEffect.kind,
        payload: firstEffect.payload,
        predecessors: [],
        productEventId: firstEffect.product_event_id,
        productCommandId: base.admission.product.frame.header.command_id,
        subjects: [{ kind: "outcome", id: ZapIdSchema.parse("outcome.workflow") }],
      },
      {
        effectId: secondEffect.effect_id,
        index: 1,
        kind: secondEffect.kind,
        payload: secondEffect.payload,
        predecessors: [],
        productEventId: secondEffect.product_event_id,
        productCommandId: ZapIdSchema.parse("command.workflow.second"),
        subjects: [{ kind: "outcome", id: ZapIdSchema.parse("outcome.workflow") }],
      },
    ],
    planReceipt: receipt("plan", "6"),
    assessmentReceipt: receipt("assessment", "7"),
  });
  const firstAdvance = {
    ...base.admission,
    comparison: {
      ...base.admission.comparison,
      draft: {
        ...base.admission.comparison.draft,
        alternatives: [
          {
            ...firstAlternative,
            effects: [firstEffect, secondEffect],
          },
        ],
      },
    },
  };
  const firstCompared = await zap.prepareComparison(firstAdvance.comparison);
  assert.equal(firstCompared.ok, true);
  if (!firstCompared.ok) return;
  const firstStep = PreparedAdmissionStepSchema.parse({
    executionBasis: base.basis,
    effectIndex: 0,
    advanceRequest: firstAdvance,
    productDigest: protectedCommandDigest(base.admission.product),
    comparisonView: firstCompared.value,
  });
  const secondProduct = {
    frame: {
      ...base.admission.product.frame,
      header: {
        ...base.admission.product.frame.header,
        command_id: ZapIdSchema.parse("command.workflow.second"),
        event_id: secondEffect.product_event_id,
        expected_revision: 10n,
      },
    },
  };
  const secondAdvance = {
    ...base.admission,
    expected_revision: 9n,
    comparison: {
      ...base.admission.comparison,
      draft: {
        ...base.admission.comparison.draft,
        alternatives: [
          {
            ...firstAlternative,
            committed_prefix: [firstEffect.effect_id],
            effects: [secondEffect],
          },
        ],
      },
    },
    product: secondProduct,
  };
  const secondCompared = await zap.prepareComparison(secondAdvance.comparison);
  assert.equal(secondCompared.ok, true);
  if (!secondCompared.ok) return;
  const secondStep = PreparedAdmissionStepSchema.parse({
    executionBasis: { ...base.basis, revision: "9" },
    effectIndex: 1,
    advanceRequest: secondAdvance,
    productDigest: protectedCommandDigest(secondProduct),
    comparisonView: secondCompared.value,
  });
  const authoring = authoringFixture(secondStep);
  const proposal = AgentPlanProposalSchema.parse({
    ...base,
    preparedSuccessor: prepared,
    preparedStep: firstStep,
  });
  const opened = openSqlitePlanWorkflowStore(path, scope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    ...scope,
    authoring,
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  await workflow.submitProposal(connection(), intent(proposal), proposal);
  const apply = {
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis: proposal.basis,
  };
  const first = await workflow.apply(apply);
  assert.equal(first.ok && first.value.state, "admitted");
  opened.value.close();
  const reopened = openSqlitePlanWorkflowStore(path, scope);
  assert.equal(reopened.ok, true);
  if (!reopened.ok || !first.ok || first.value.nextBasis === null) return;
  const resumed = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: reopened.value,
    ...scope,
    authoring,
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const final = await resumed.apply({ ...apply, basis: first.value.nextBasis });
  assert.equal(
    final.ok && final.value.state,
    "completed",
    final.ok ? final.value.message : JSON.stringify(final.error),
  );
  assert.equal(zap.advances, 2);
  assert.equal(zap.submits, 2);
  reopened.value.close();
});

function receipt(id: string, revision: string) {
  return {
    commandId: ZapIdSchema.parse(`command.${id}`),
    commandDigest: ZapDigestSchema.parse("8".repeat(64)),
    revision: u64(revision),
  };
}

function authoringFixture(
  step: ReturnType<typeof PreparedAdmissionStepSchema.parse>,
): PlanAuthoringPort {
  const unused = async () => ({
    ok: false as const,
    error: {
      code: "unavailable" as const,
      message: "unused",
      recovery: "use prepareEffect",
    },
  });
  const zapUnused = async () => ({
    ok: false as const,
    error: { kind: "configuration" as const, message: "unused preparation" },
  });
  return {
    discover: unused,
    query: unused,
    prepareBundle: zapUnused,
    prepareComparison: zapUnused,
    prepareProjectedRecord: zapUnused,
    prepareSuccessor: unused,
    submitMetadata: unused,
    reconcileMetadata: unused,
    prepareAssessment: unused,
    finishSuccessor: unused,
    prepareMilestonePrecursors: unused,
    prepareCompositeSuccessor: unused,
    recordCompositeSuccessor: unused,
    prepareCompositeAssessment: unused,
    prepareEffect: async (_prepared, input) =>
      input.effectIndex === 1
        ? { ok: true, value: step }
        : {
            ok: false,
            error: { code: "invalid_input", message: "wrong effect", recovery: "use index 1" },
          },
  };
}
