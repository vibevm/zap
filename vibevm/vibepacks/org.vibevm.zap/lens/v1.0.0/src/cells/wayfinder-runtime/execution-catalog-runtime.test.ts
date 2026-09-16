/** Actual HTTP-to-ZapMock catalog dispatch. @scope spec://org.vibevm.zap/lens/PROP-015#mock */
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createZapMockManagedControlAdapter } from "../managed-work/index.ts";
import { ExecutionCatalogRuntimeSimulationSchema } from "../mock-simulation/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createWorkspaceHttpConnection } from "../workspace-client/index.ts";
import {
  ProjectIdSchema,
  QuestionGroupIdSchema,
  QuestionItemIdSchema,
  QuestionOptionIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";
import {
  MOCK_CONTEXT_ID,
  MOCK_PROJECT_ID,
  mockUnusedHost,
  writeMockProductScenario,
} from "./mock-managed.support.ts";
import {
  catalogRuntimeConfig,
  ensureCatalogRuntimeConfiguration,
} from "./execution-catalog-runtime.support.ts";

const scenario = ExecutionCatalogRuntimeSimulationSchema.parse(
  JSON.parse(
    readFileSync(new URL("./execution-catalog-runtime.simulation.json", import.meta.url), "utf8"),
  ),
);

test("catalog policy dispatches an authenticated managed ZapMock process over HTTP", async () => {
  assert.equal(process.env["ZAP_MOCK_SIMULATION_ID"] ?? scenario.scenarioId, scenario.scenarioId);
  const root = mkdtempSync(join(tmpdir(), "zap-catalog-runtime-"));
  const behavior = writeMockProductScenario(root, process.env["ZAP_MOCK_SIMULATION_SEED"]);
  const config = catalogRuntimeConfig(root, behavior.path);
  const runtime = createWayfinderRuntime(config, {
    hosts: [mockUnusedHost()],
    managedControlAdapters: [
      createZapMockManagedControlAdapter({ directory: join(root, "mock-control") }),
    ],
  });
  assert.equal(runtime.ok, true, JSON.stringify(runtime));
  if (!runtime.ok) return;
  let processClosed = false;
  try {
    const started = await runtime.value.start();
    assert.equal(started.ok, true, JSON.stringify(started));
    if (!started.ok) return;
    const ticket = runtime.value.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const connection = createWorkspaceHttpConnection({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://mock-product.test",
    });
    assert.notEqual(connection, null);
    if (connection === null) return;
    const configured = await ensureCatalogRuntimeConfiguration(
      connection.product,
      behavior.expected.modelId,
    );
    assert.equal(configured.ok, true, configured.ok ? undefined : configured.error.message);
    if (!configured.ok) return;
    const configuration = configured.value;
    const created = await connection.workspace.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.create.v1",
        clientRequestId: "request.catalog.runtime.create",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        selection: {
          mode: "catalog_policy",
          effort: { mode: "unspecified" },
          context: { mode: "default" },
        },
        specialization: scenario.inputs.specialization,
        goal: scenario.inputs.goal,
        expectedResult: scenario.inputs.expectedResult,
        targetRefs: [],
        contextRefs: [],
        parentTaskId: null,
        parentRunId: null,
        sourceBasisRef: "scenario.catalog.runtime",
        planRevision: null,
        depth: 0,
        budgets: { maximumTurns: 8, wallTimeMs: 60_000 },
      }),
    );
    assert.equal(created.ok, true, JSON.stringify(created));
    if (!created.ok || created.value.operation !== "managed-work.create.v1") return;
    assert.equal(
      created.value.work.executionSelection?.configurationId === configuration.configurationId,
      scenario.expected.selectionPinned,
    );
    assert.equal(
      created.value.work.executionSelection?.modelFamilyId,
      scenario.expected.modelFamilyId,
    );
    assert.equal(created.value.work.provider, scenario.expected.provider);
    const launched = await connection.workspace.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.start.v1",
        clientRequestId: "request.catalog.runtime.start",
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        runId: created.value.work.runId,
        expectedRevision: created.value.work.revision,
      }),
    );
    assert.equal(launched.ok, true, JSON.stringify(launched));
    if (!launched.ok || launched.value.operation !== "managed-work.start.v1") return;
    assert.equal(launched.value.work.modelSelection.modelId, scenario.expected.modelId);
    const question = await waitForQuestion(connection.workspace);
    assert.equal(question.originActorId, launched.value.work.actorId);
    assert.equal(question.items[0]?.promptMarkdown, scenario.expected.questionPrompt);
    const item = question.items[0];
    const option = item?.options[behavior.inputs.answerOptionIndex];
    assert.ok(item !== undefined && option !== undefined);
    if (item === undefined || option === undefined) return;
    const answered = await connection.workspace.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "question.answer.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.catalog.runtime.answer"),
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
    const reported = await waitForManagedState(
      connection.workspace,
      launched.value.work.runId,
      "reported",
    );
    await waitForTerminalExit(connection.workspace, reported.terminalId);
    processClosed = true;
    process.stdout.write(
      `ZAP_MOCK_SIMULATION_RECEIPT ${JSON.stringify({
        scenarioId: scenario.scenarioId,
        seed: behavior.seed,
        passed: true,
        inputPosition: 5,
        zeroLlmInference: scenario.expected.zeroLlmInference,
      })}\n`,
    );
  } finally {
    await runtime.value.close();
    if (processClosed)
      rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
});

type WorkspacePort = NonNullable<ReturnType<typeof createWorkspaceHttpConnection>>["workspace"];

async function waitForQuestion(client: WorkspacePort) {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    const result = await client.read({
      operation: "question.list.v1",
      projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
      contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
      state: "open",
      limit: 10,
    });
    if (
      result.ok &&
      result.value.operation === "question.list.v1" &&
      result.value.questions[0] !== undefined
    )
      return result.value.questions[0];
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  const work = await client.read({
    operation: "managed-work.get.v1",
    projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
    contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
    runId: RunIdSchema.parse("run.missing"),
  });
  assert.fail(
    `violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: catalog-selected ZapMock did not ask through MCP; fix surface: inspect runtime state ${JSON.stringify(work)}`,
  );
}

async function managedWork(client: WorkspacePort, runId: string) {
  const result = await client.read({
    operation: "managed-work.get.v1",
    projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
    contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
    runId: RunIdSchema.parse(runId),
  });
  assert.equal(result.ok, true, JSON.stringify(result));
  if (!result.ok || result.value.operation !== "managed-work.get.v1")
    assert.fail(
      "violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: managed work read failed; fix surface: inspect the HTTP projection",
    );
  return result.value.work;
}

async function waitForManagedState(client: WorkspacePort, runId: string, state: "reported") {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    const work = await managedWork(client, runId);
    if (work.state === state) return work;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.fail(
    "violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: catalog-selected ZapMock did not report; fix surface: inspect managed MCP delivery",
  );
}

async function waitForTerminalExit(client: WorkspacePort, terminalId: string): Promise<void> {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    const result = await client.read({
      operation: "terminal.list.v1",
      projectId: ProjectIdSchema.parse(MOCK_PROJECT_ID),
      contextId: WorkContextIdSchema.parse(MOCK_CONTEXT_ID),
    });
    if (result.ok && result.value.operation === "terminal.list.v1") {
      const terminal = result.value.terminals.find(
        (candidate) => candidate.terminalId === terminalId,
      );
      if (terminal?.state === "exited") return;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.fail(
    "violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: ZapMock terminal did not exit; fix surface: inspect managed stop reconciliation",
  );
}
