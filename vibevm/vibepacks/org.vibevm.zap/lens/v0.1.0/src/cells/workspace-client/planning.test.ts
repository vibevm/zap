import assert from "node:assert/strict";
import test from "node:test";

import {
  ExactDecimalSchema,
  PlanBasisSchema,
  PlanOperationResultSchema,
  QuicklensRefSchema,
  QuicklensSnapshotSchema,
  type PlanApplyInput,
  type PlanDecisionInput,
  type PlanIntentInput,
  type PlanPreviewInput,
  type PlanReconcileInput,
} from "../quicklens-model/index.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  HistoryCursorSchema,
  HistoryEventSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceErrorSchema,
  WorkspaceReadResponseSchema,
  type HistoryEvent,
  type WorkspaceClientPort,
  type WorkspaceCommandResponse,
  type WorkspaceCommandRequest,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { createWorkspacePlanningDataSource } from "./planning.ts";

const projectId = ProjectIdSchema.parse("project.plan.fixture");
const contextId = WorkContextIdSchema.parse("context.plan.fixture");

test("workspace planning source reads scoped snapshots and sends every plan command with basis", async () => {
  const calls: WorkspaceCommandRequest[] = [];
  const port = fakePort(calls);
  const source = createWorkspacePlanningDataSource({ port, projectId, contextId });
  const snapshot = QuicklensSnapshotSchema.parse({
    sourceMode: "live",
    sourceLabel: "Workspace planning fixture",
    phase: "ready",
    phaseDetail: null,
    capturedAt: "2026-09-15T00:00:00.000Z",
    revision: "1",
    objects: [],
    relationships: [],
    questions: [],
    agentTargets: [],
    plan: null,
  });
  const read = await source.read({ signal: new AbortController().signal });
  assert.equal(read.ok, true);
  if (read.ok) {
    assert.deepEqual(read.value, {
      ...snapshot,
      questionAnswer: {
        enabled: false,
        reason: "Answer questions in the shared workspace Questions view.",
      },
    });
  }

  const basis = PlanBasisSchema.parse({
    storeRef: QuicklensRefSchema.parse("store.fixture"),
    baseRef: QuicklensRefSchema.parse("base.fixture"),
    revision: ExactDecimalSchema.parse("3"),
    sourceBasisRef: QuicklensRefSchema.parse("source.fixture"),
  });
  const intent: PlanIntentInput = {
    text: "Plan the next step",
    basis,
    targetActorRef: QuicklensRefSchema.parse("actor.fixture"),
  };
  const preview: PlanPreviewInput = {
    intentRef: QuicklensRefSchema.parse("intent.fixture"),
    basis,
  };
  const apply: PlanApplyInput = {
    operationRef: QuicklensRefSchema.parse("operation.fixture"),
    previewRef: QuicklensRefSchema.parse("preview.fixture"),
    basis,
  };
  const reconcile: PlanReconcileInput = { operationRef: apply.operationRef, basis };
  const decide: PlanDecisionInput = {
    operationRef: apply.operationRef,
    holdRef: QuicklensRefSchema.parse("hold.fixture"),
    basis,
    choice: "approve",
    reason: "Approved fixture plan",
  };
  assert.equal((await source.proposePlanIntent(intent)).ok, true);
  assert.equal((await source.previewPlan(preview)).ok, true);
  assert.equal((await source.applyPlan(apply)).ok, true);
  assert.equal((await source.reconcilePlan(reconcile)).ok, true);
  assert.equal((await source.decidePlan(decide)).ok, true);
  assert.equal(calls.length, 5);
  assert.deepEqual(
    calls.map((call) => call.operation),
    ["plan.intent.v1", "plan.preview.v1", "plan.apply.v1", "plan.reconcile.v1", "plan.decide.v1"],
  );
  for (const call of calls) {
    assert.equal(call.projectId, projectId);
    assert.equal(call.contextId, contextId);
    assert.equal(ClientRequestIdSchema.safeParse(call.clientRequestId).success, true);
  }
  const intentCall = calls[0];
  assert.equal(intentCall?.operation, "plan.intent.v1");
  if (intentCall?.operation === "plan.intent.v1") assert.deepEqual(intentCall.input, intent);
});

test("workspace planning source preserves backend errors and marks question writes unavailable", async () => {
  const backend = WorkspaceErrorSchema.parse({
    code: "forbidden",
    message: "violates REQ spec://workspace/fixture: planning is denied",
    details: { reason: "fixture" },
  });
  const port: WorkspaceClientPort = {
    read: async () => ({ ok: false, error: backend }),
    command: async () => ({ ok: false, error: backend }),
    events: async () => ({ ok: false, error: backend }),
    subscribe: async function* () {},
  };
  const source = createWorkspacePlanningDataSource({ port, projectId, contextId });
  const read = await source.read({ signal: new AbortController().signal });
  assert.equal(read.ok, false);
  if (!read.ok) {
    assert.equal(read.error.code, "forbidden");
    assert.equal(read.error.message, backend.message);
  }
  const plan = await source.previewPlan({
    intentRef: QuicklensRefSchema.parse("intent.fixture"),
    basis: basisFixture(),
  });
  assert.equal(plan.ok, false);
  if (!plan.ok) assert.equal(plan.error.message, backend.message);
  const question = await source.answerQuestion({
    questionRef: QuicklensRefSchema.parse("question.fixture"),
    expectedRevision: ExactDecimalSchema.parse("1"),
    answer: "No",
  });
  assert.equal(question.ok, false);
  if (!question.ok) assert.match(question.error.message, /does not own question answers/);
});

test("workspace planning source forwards scoped semantic invalidations and reconnect errors", async () => {
  const calls: WorkspaceCommandRequest[] = [];
  let cursorScope: unknown;
  const relevant = historyEvent("plan.applied", projectId, contextId);
  const unrelated = historyEvent(
    "question.created",
    "project.other.fixture",
    "context.other.fixture",
  );
  const port: WorkspaceClientPort = {
    ...fakePort(calls),
    subscribe: async function* (request) {
      cursorScope = request.cursor.scope;
      yield { ok: true, value: unrelated };
      yield { ok: true, value: relevant };
      yield {
        ok: false,
        error: {
          code: "unavailable",
          message: "violates REQ spec://workspace/fixture: stream ended",
        },
      };
    },
  };
  const source = createWorkspacePlanningDataSource({ port, projectId, contextId });
  const reasons: string[] = [];
  const stop = source.subscribe?.((reason) => reasons.push(reason));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(reasons, ["plan", "reconnect"]);
  assert.deepEqual(cursorScope, { kind: "project", projectId });
  stop?.();
});

function fakePort(calls: WorkspaceCommandRequest[]): WorkspaceClientPort {
  return {
    read: async () => ({
      ok: true,
      value: WorkspaceReadResponseSchema.parse({
        operation: "project.snapshot.v1",
        snapshot: { state: "ready", snapshot: snapshotFixture() },
      }),
    }),
    command: async (request) => {
      calls.push(request);
      return commandFixture(request.operation);
    },
    events: async () => ({
      ok: true,
      value: {
        events: [],
        resume: HistoryCursorSchema.parse({
          scope: { kind: "project", projectId },
          afterGlobalSequence: "0",
        }),
        next: null,
        coverage: { state: "complete" },
      },
    }),
    subscribe: async function* () {},
  };
}

function commandFixture(
  operation: WorkspaceCommandRequest["operation"],
): WorkspaceResult<WorkspaceCommandResponse> {
  if (!operation.startsWith("plan."))
    return {
      ok: false,
      error: {
        code: "unsupported_operation",
        message: "violates REQ spec://workspace/fixture: plan only",
      },
    };
  return {
    ok: true,
    value: WorkspaceCommandResponseSchema.parse({
      operation,
      result: PlanOperationResultSchema.parse({
        operationRef: QuicklensRefSchema.parse("operation.fixture"),
        previewRef: null,
        preview: null,
        state: "queued",
        message: "Queued fixture plan",
        nextBasis: null,
      }),
    }),
  };
}

function snapshotFixture() {
  return QuicklensSnapshotSchema.parse({
    sourceMode: "live",
    sourceLabel: "Workspace planning fixture",
    phase: "ready",
    phaseDetail: null,
    capturedAt: "2026-09-15T00:00:00.000Z",
    revision: "1",
    objects: [],
    relationships: [],
    questions: [],
    agentTargets: [],
    plan: null,
  });
}

function basisFixture() {
  return PlanBasisSchema.parse({
    storeRef: QuicklensRefSchema.parse("store.fixture"),
    baseRef: QuicklensRefSchema.parse("base.fixture"),
    revision: ExactDecimalSchema.parse("1"),
    sourceBasisRef: QuicklensRefSchema.parse("source.fixture"),
  });
}

function historyEvent(kind: string, eventProjectId: string, eventContextId: string): HistoryEvent {
  return HistoryEventSchema.parse({
    historyEventId: `history.${kind}.${eventProjectId}`,
    projectId: eventProjectId,
    contextId: eventContextId,
    globalSequence: DecimalSchema.parse("1"),
    projectSequence: DecimalSchema.parse("1"),
    sourceSequence: null,
    kind,
    source: "lens",
    actorId: null,
    occurrenceAt: "2026-09-15T00:00:00.000Z",
    ingestedAt: "2026-09-15T00:00:00.000Z",
    sourceEventId: null,
    correlationId: null,
    causationId: null,
    planProvenance: null,
    payload: {},
  });
}
