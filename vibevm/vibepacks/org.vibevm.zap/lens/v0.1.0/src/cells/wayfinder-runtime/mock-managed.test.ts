/** Actual no-LLM managed product gate. @scope spec://org.vibevm.zap/lens/PROP-013#verification */
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createZapMockManagedControlAdapter } from "../managed-work/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import {
  ProjectIdSchema,
  QuestionGroupIdSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
  type ManagedWorkView,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";
import {
  MOCK_CONTEXT_ID,
  MOCK_PROFILE_ID,
  MOCK_PROJECT_ID,
  mockProductConfig,
  mockUnusedHost,
  writeMockProductScenario,
} from "./mock-managed.support.ts";

test("actual Wayfinder managed ZapMock question survives pause and reports after Continue", async () => {
  const root = mkdtempSync(join(tmpdir(), "zap-mock-managed-product-"));
  const behavior = writeMockProductScenario(root, process.env["ZAP_MOCK_SIMULATION_SEED"]);
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? behavior.scenarioId, behavior.scenarioId);
  const runtime = createWayfinderRuntime(mockProductConfig(root, behavior.path), {
    hosts: [mockUnusedHost()],
    managedControlAdapters: [
      createZapMockManagedControlAdapter({ directory: join(root, "mock-control") }),
    ],
  });
  assert.equal(runtime.ok, true);
  if (!runtime.ok) return;
  let processClosed = false;
  try {
    const started = await runtime.value.start();
    assert.equal(started.ok, true, JSON.stringify(started));
    if (!started.ok) return;
    const ticket = runtime.value.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const client = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://mock-product.test",
    });
    assert.notEqual(client, null);
    if (client === null) return;
    const created = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.create.v1",
        clientRequestId: "request.mock.product.create",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        selection: {
          mode: "profile_override",
          profileId: MOCK_PROFILE_ID,
          reasonMarkdown: "Run the explicit deterministic no-model product gate.",
        },
        goal: "Ask, retain the late answer across Pause, acknowledge it and report.",
        expectedResult: "A typed managed report awaiting human review.",
        targetRefs: [],
        contextRefs: [],
        parentTaskId: null,
        parentRunId: null,
        sourceBasisRef: "scenario.mock.product",
        planRevision: null,
        depth: 0,
        budgets: { maximumTurns: 8, wallTimeMs: 60_000 },
      }),
    );
    assert.equal(created.ok, true, JSON.stringify(created));
    if (!created.ok || created.value.operation !== "managed-work.create.v1") return;
    const launched = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.start.v1",
        clientRequestId: "request.mock.product.start",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        runId: created.value.work.runId,
        expectedRevision: created.value.work.revision,
      }),
    );
    assert.equal(launched.ok, true, JSON.stringify(launched));
    if (!launched.ok || launched.value.operation !== "managed-work.start.v1") return;
    assert.equal(launched.value.work.provider, behavior.expected.provider);
    assert.equal(launched.value.work.modelSelection.modelId, behavior.expected.modelId);
    const question = await waitForQuestion(client);
    assert.equal(question.originActorId, launched.value.work.actorId);
    assert.equal(question.items[0]?.promptMarkdown, behavior.expected.questionPrompt);
    const idle = await waitForManagedIdle(client, launched.value.work.runId);
    assert.equal(idle.managedControl?.providerSessionId, launched.value.work.sessionId);
    const item = question.items[0];
    const option = item?.options[behavior.inputs.answerOptionIndex];
    assert.ok(item !== undefined && option !== undefined);
    if (item === undefined || option === undefined) return;
    const beforePause = await execution(client);
    const paused = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "project.pause.v1",
        clientRequestId: "request.mock.product.pause",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        sessionId: null,
        expectedRevision: beforePause.revision,
        reasonMarkdown: "Retain the synthetic question while no coordinator exists.",
      }),
    );
    assert.equal(paused.ok, true, JSON.stringify(paused));
    if (!paused.ok || paused.value.operation !== "project.pause.v1") return;
    assert.equal(paused.value.execution.state, behavior.expected.pausedExecutionState);
    const answered = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "question.answer.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.mock.product.answer"),
        projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
        contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
        questionGroupId: QuestionGroupIdSchema.parse(question.questionGroupId),
        expectedRevision: question.revision,
        submission: {
          answers: [
            {
              questionItemId: QuestionItemIdSchema.parse(item.questionItemId),
              answer: {
                kind: "single_choice",
                optionId: QuestionOptionIdSchema.parse(option.optionId),
              },
            },
          ],
          noteMarkdown: behavior.inputs.answerNote,
        },
      }),
    );
    assert.equal(answered.ok, true, JSON.stringify(answered));
    const held = await managedWork(client, launched.value.work.runId);
    assert.notEqual(held.state, behavior.expected.heldWorkStateMustNotBe);
    const beforeContinue = await execution(client);
    const continued = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "project.continue.v1",
        clientRequestId: "request.mock.product.continue",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        sessionId: null,
        expectedRevision: beforeContinue.revision,
        reasonMarkdown: "Release the retained deterministic answer.",
      }),
    );
    assert.equal(continued.ok, true, JSON.stringify(continued));
    if (!continued.ok || continued.value.operation !== "project.continue.v1") return;
    assert.equal(continued.value.execution.state, "running");
    const reported = await waitForManagedState(client, launched.value.work.runId, "reported");
    assert.equal(reported.actorId, launched.value.work.actorId);
    assert.equal(
      (reported.report?.summaryMarkdown ?? "").includes(behavior.expected.reportIncludes),
      true,
    );
    assert.equal(reported.managedControl?.providerSessionId, launched.value.work.sessionId);
    const network = await client.read({
      operation: "agent.network.v1",
      projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
      contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
    });
    assert.equal(network.ok, true);
    if (network.ok && network.value.operation === "agent.network.v1")
      assert.equal(
        network.value.network.agents.some(
          (actor) => actor.actorId === reported.actorId && actor.role === "worker",
        ),
        true,
      );
    await waitForTerminalExit(client, reported.terminalId);
    processClosed = true;
    const reviewed = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.review.v1",
        clientRequestId: "request.mock.product.review",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        runId: reported.runId,
        expectedRevision: reported.revision,
        disposition: "accepted",
        commentMarkdown: "Human accepted the deterministic no-model report.",
      }),
    );
    assert.equal(reviewed.ok, true, JSON.stringify(reviewed));
    if (reviewed.ok && reviewed.value.operation === "managed-work.review.v1")
      assert.equal(reviewed.value.work.state, behavior.expected.reviewedWorkState);
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: behavior.scenarioId,
        seed: behavior.seed,
        passed: true,
        inputPosition: 7,
        zeroLlmInference: behavior.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await runtime.value.close();
    if (processClosed)
      rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

type MockClient = NonNullable<ReturnType<typeof createWorkspaceHttpClient>>;

async function waitForQuestion(client: MockClient) {
  return poll(async () => {
    const result = await client.read({
      operation: "question.list.v1",
      projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
      contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
      state: "open",
      limit: 10,
    });
    return result.ok && result.value.operation === "question.list.v1"
      ? result.value.questions[0]
      : undefined;
  });
}

async function managedWork(client: MockClient, runId: string): Promise<ManagedWorkView> {
  const result = await client.read({
    operation: "managed-work.get.v1",
    projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
    contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
    runId: RunIdSchema.parse(runId),
  });
  assert.equal(result.ok, true);
  if (!result.ok || result.value.operation !== "managed-work.get.v1") throw new Error();
  return result.value.work;
}

async function waitForManagedState(
  client: MockClient,
  runId: string,
  state: ManagedWorkView["state"],
) {
  return poll(async () => {
    const work = await managedWork(client, runId);
    return work.state === state ? work : undefined;
  });
}

async function waitForManagedIdle(client: MockClient, runId: string): Promise<ManagedWorkView> {
  let last: ManagedWorkView | undefined;
  for (let attempt = 0; attempt < 400; attempt += 1) {
    last = await managedWork(client, runId);
    if (last.managedControl?.readiness === "idle" && last.managedControl.providerSessionId !== null)
      return last;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-013#verification: managed question became visible before an exact idle provider session was observed; fix surface: inspect managed control projection ${JSON.stringify(
      {
        state: last?.state ?? null,
        managedControl: last?.managedControl ?? null,
      },
    )}`,
  );
}

async function execution(client: MockClient) {
  const result = await client.read({
    operation: "project.execution.get.v1",
    projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
    contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
  });
  assert.equal(result.ok, true);
  if (!result.ok || result.value.operation !== "project.execution.get.v1") throw new Error();
  return result.value.execution;
}

async function waitForTerminalExit(client: MockClient, terminalId: string): Promise<void> {
  await poll(async () => {
    const result = await client.read({
      operation: "terminal.list.v1",
      projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
      contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
    });
    if (!result.ok || result.value.operation !== "terminal.list.v1") return undefined;
    const terminal = result.value.terminals.find((item) => item.terminalId === terminalId);
    return terminal?.state === "exited" ? true : undefined;
  });
}

async function poll<T>(read: () => Promise<T | undefined>): Promise<T> {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    const value = await read();
    if (value !== undefined) return value;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error();
}
