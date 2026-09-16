import { expect, test } from "vitest";

import {
  ExactDecimalSchema,
  PlanDecisionInputSchema,
  PlanOperationResultSchema,
  PlanStatusSchema,
  QuicklensSnapshotSchema,
  unavailableDataSource,
} from "./index.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-002#acceptance */
test("model preserves semantic kinds, distinct planning metrics and source authority", async () => {
  const snapshot = QuicklensSnapshotSchema.parse({
    sourceMode: "live",
    sourceLabel: "Test plan",
    phase: "ready",
    phaseDetail: null,
    capturedAt: "2026-09-15T04:00:00.000Z",
    revision: "90071992547409931234",
    objects: [
      {
        ref: "fact.one",
        category: "other",
        semanticType: "verified_source_fact",
        title: "A verified fact",
        purpose: null,
        acceptance: null,
        status: { code: "known", label: "Known", tone: "neutral" },
        metrics: {
          complexity: { state: "unknown", reason: "Facts have no task complexity." },
          difficulty: { state: "known", value: "2", unit: "level", explanation: null },
          effort: { state: "unknown", reason: "No effort supplied." },
          waiting: { state: "known", value: "0", unit: "items", explanation: null },
          uncertainty: { state: "known", value: "low", unit: null, explanation: null },
        },
        provenance: [],
        position: null,
      },
    ],
    relationships: [],
    questions: [],
    agentTargets: [],
    plan: {
      outcomeLabel: "Outcome",
      strategyLabel: "Strategy",
      planLabel: "Plan",
      basis: {
        storeRef: "store.one",
        baseRef: "base.one",
        revision: "90071992547409931234",
        sourceBasisRef: "source.one",
      },
      state: "held",
      detail: null,
      actions: {
        propose: { enabled: true, reason: null },
        preview: { enabled: false, reason: "No intent exists." },
        apply: { enabled: false, reason: "No preview exists." },
        reconcile: { enabled: false, reason: "No uncertain operation exists." },
      },
    },
  });
  expect(snapshot.objects[0]?.semanticType).toBe("verified_source_fact");
  expect(snapshot.objects[0]?.metrics.complexity.state).toBe("unknown");
  expect(snapshot.plan?.actions.preview.reason).toBe("No intent exists.");
  expect(snapshot.revision).toBe("90071992547409931234");
  expect(ExactDecimalSchema.safeParse(Number.MAX_SAFE_INTEGER + 1).success).toBe(false);

  const unavailable = await unavailableDataSource("Disconnected").read({
    signal: new AbortController().signal,
  });
  expect(unavailable.ok).toBe(false);
});

test("owner decision input carries only held identity, basis, choice and bounded reason", () => {
  const input = {
    operationRef: "operation.held",
    holdRef: "hold.owner-review",
    basis: {
      storeRef: "store.one",
      baseRef: "base.one",
      revision: "9",
      sourceBasisRef: "source.one",
    },
    choice: "revise",
    reason: "Acceptance evidence needs one more browser observation.",
  };
  expect(PlanDecisionInputSchema.safeParse(input).success).toBe(true);
  expect(PlanDecisionInputSchema.safeParse({ ...input, reason: "" }).success).toBe(false);
  expect(
    PlanDecisionInputSchema.safeParse({ ...input, policyDigest: "must-not-cross-ui" }).success,
  ).toBe(false);
});

test("owner decision availability cannot escape an exact held-operation binding", () => {
  const status = {
    outcomeLabel: "Outcome",
    strategyLabel: "Strategy",
    planLabel: "Plan",
    basis: {
      storeRef: "store.one",
      baseRef: "base.one",
      revision: "9",
      sourceBasisRef: "source.one",
    },
    state: "held",
    detail: "Owner review is required.",
    decision: { operationRef: "operation.held", holdRef: "hold.owner-review" },
    actions: {
      propose: { enabled: true, reason: null },
      preview: { enabled: true, reason: null },
      apply: { enabled: false, reason: "Operation is held." },
      reconcile: { enabled: false, reason: "No uncertain operation." },
      decide: { enabled: true, reason: null },
    },
  };
  expect(PlanStatusSchema.safeParse(status).success).toBe(true);
  expect(PlanStatusSchema.safeParse({ ...status, state: "current" }).success).toBe(false);
});

test("queued intent and prepared preview identities cannot be conflated", () => {
  const queued = PlanOperationResultSchema.parse({
    operationRef: "intent.request.one",
    previewRef: null,
    preview: null,
    state: "queued",
    message: "Intent queued for the selected coordinator.",
    nextBasis: null,
  });
  expect(queued.state).toBe("queued");
  expect(PlanOperationResultSchema.safeParse({ ...queued, state: "prepared" }).success).toBe(false);
  expect(
    PlanOperationResultSchema.safeParse({
      ...queued,
      state: "prepared",
      previewRef: "preview.one",
      preview: {
        basis: {
          storeRef: "store.one",
          baseRef: "base.one",
          revision: "9",
          sourceBasisRef: "source.one",
        },
        changes: [
          {
            subjectLabel: "Renderer milestone",
            changeKind: "revise_acceptance",
            beforeSummary: "Visual proof pending.",
            afterSummary: "Browser and Electron visual proof required.",
          },
        ],
      },
    }).success,
  ).toBe(true);
});
