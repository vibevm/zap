/** Real PTY and local-MCP proof for ZapMockAgent CLI. @scope spec://org.vibevm.zap/lens/PROP-013#verification */
import assert from "node:assert/strict";
import test from "node:test";
import { appendFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { createOptionalNodePtyFactory } from "../managed-terminal/index.ts";
import {
  ZapMockManagedAssignmentSchema,
  ZapMockManagedInputSchema,
  ZapMockManagedSidebandEventSchema,
} from "../managed-work/index.ts";
import { ZapMockSnapshotSchema } from "../mock-model/index.ts";
import { runZapMockAgentCli } from "../../zap-mock-agent.ts";

const root = fileURLToPath(new URL("../../..", import.meta.url));
const entry = fileURLToPath(new URL("../../zap-mock-agent.ts", import.meta.url));
const fixture = fileURLToPath(new URL("./managed-process.fixture.ts", import.meta.url));

test("ZapMockAgent managed CLI uses a real PTY and local MCP, then restores its snapshot", async () => {
  const directory = mkdtempSync(join(tmpdir(), "zap-mock-process-"));
  try {
    const scenarioPath = join(directory, "scenario.json");
    const mcpConfigPath = join(directory, "mcp.json");
    const inputPath = join(directory, "input.jsonl");
    const sidebandPath = join(directory, "sideband.jsonl");
    const callLog = join(directory, "mcp-calls.jsonl");
    writeFileSync(
      scenarioPath,
      JSON.stringify({
        protocol: "zap-mock-scenario/1",
        seed: "seed.managed-process.fixture",
        scenario: {
          scenarioId: "scenario.managed-process.fixture",
          steps: [
            { kind: "ready" },
            {
              kind: "busy_gate",
              gateId: "gate.managed.fixture",
              completionText: "busy gate released",
            },
            {
              kind: "question",
              questionId: "question.logical.fixture",
              prompt: "Choose the deterministic path",
              options: ["first", "second"],
            },
            {
              kind: "late_answer",
              questionId: "question.logical.fixture",
              acknowledgementId: "ack.logical.fixture",
            },
            { kind: "report", reportId: "report.logical.fixture" },
          ],
        },
      }),
      "utf8",
    );
    writeFileSync(inputPath, "", "utf8");
    writeFileSync(sidebandPath, "", "utf8");
    writeFileSync(callLog, "", "utf8");
    writeFileSync(
      mcpConfigPath,
      JSON.stringify({
        mcpServers: {
          "zap-wayfinder": {
            command: process.execPath,
            args: ["--experimental-strip-types", fixture],
            env: {
              CODLENS_URL: "http://127.0.0.1:1",
              CODLENS_CREDENTIAL_FILE: callLog,
              CODLENS_ADAPTER_SESSION_ID: "adapter.mock.fixture",
              CODLENS_WORKSPACE_ID: "workspace.mock.fixture",
              CODLENS_CONVERSATION_ID: "conversation.mock.fixture",
            },
          },
        },
      }),
      "utf8",
    );
    const assignment = ZapMockManagedAssignmentSchema.parse({
      protocol: "zap-mock-managed-assignment/1",
      runId: "run.mock.fixture",
      actorId: "actor.mock.fixture",
      sessionId: "session.mock.fixture",
      terminalId: "terminal.mock.fixture",
      expectedProcessEpoch: "process.mock.fixture.1",
    });
    const spawned = await spawnManaged({
      scenarioPath,
      mcpConfigPath,
      inputPath,
      sidebandPath,
      assignment,
      mode: { kind: "instructions", value: "Read the synthetic packet" },
    });
    await until(() => sideband(sidebandPath).some((event) => event.kind === "turn_started"));
    assert.equal(
      sideband(sidebandPath).some((event) => event.kind === "turn_settled"),
      false,
    );
    const beforeTick = processSnapshot(`${sidebandPath}.snapshot.json`).model.state.logicalTick;
    assert.equal(
      await runZapMockAgentCli([
        "control",
        "--input",
        inputPath,
        "--input-id",
        "control.tick.fixture",
        "--tick",
        "2",
      ]),
      0,
    );
    await until(
      () =>
        processSnapshot(`${sidebandPath}.snapshot.json`).model.state.logicalTick === beforeTick + 2,
    );
    assert.equal(
      await runZapMockAgentCli([
        "control",
        "--input",
        inputPath,
        "--input-id",
        "control.gate.fixture",
        "--open-gate",
        "gate.managed.fixture",
      ]),
      0,
    );
    await until(() => sideband(sidebandPath).some((event) => event.kind === "turn_settled"));
    const askOffer = ZapMockManagedInputSchema.parse({
      kind: "offer",
      deliveryId: "delivery.ask.fixture",
      bodyMarkdown: "Ask the declared synthetic question",
    });
    appendFileSync(inputPath, `${JSON.stringify(askOffer)}\n`, "utf8");
    await until(
      () =>
        sideband(sidebandPath).some((event) => event.kind === "turn_settled") &&
        spawned.output().includes("question sent; turn is idle"),
    );
    assert.match(spawned.output(), /question sent; turn is idle/);
    const answerOffer = ZapMockManagedInputSchema.parse({
      kind: "offer",
      deliveryId: "delivery.wake.fixture",
      bodyMarkdown: "ANSWER READY (synthetic fixture)",
    });
    appendFileSync(inputPath, `${JSON.stringify(answerOffer)}\n`, "utf8");
    await until(() => readFileSync(callLog, "utf8").includes('"kind":"report"'));
    await until(() =>
      sideband(sidebandPath).some(
        (event) =>
          event.kind === "turn_settled" && event.transportCorrelation === "delivery.wake.fixture",
      ),
    );
    spawned.process.interrupt();
    assert.equal(await spawned.exit, 0);
    spawned.close();
    const calls = readFileSync(callLog, "utf8");
    assert.match(calls, /"kind":"ask"/);
    assert.match(calls, /"kind":"inbox"/);
    assert.match(calls, /"kind":"ack"/);
    assert.match(calls, /"kind":"read"/);
    assert.match(calls, /"kind":"report"/);
    const snapshot = processSnapshot(`${sidebandPath}.snapshot.json`);
    assert.equal(snapshot.model.state.status, "complete");
    writeFileSync(inputPath, "", "utf8");
    writeFileSync(sidebandPath, "", "utf8");
    const resumedAssignment = ZapMockManagedAssignmentSchema.parse({
      ...assignment,
      expectedProcessEpoch: "process.mock.fixture.2",
    });
    const resumed = await spawnManaged({
      scenarioPath,
      mcpConfigPath,
      inputPath,
      sidebandPath,
      assignment: resumedAssignment,
      mode: { kind: "resume", value: "session.mock.fixture" },
    });
    await until(() => sideband(sidebandPath).some((event) => event.kind === "session_ready"));
    resumed.process.interrupt();
    assert.equal(await resumed.exit, 0);
    resumed.close();
    assert.equal(processSnapshot(`${sidebandPath}.snapshot.json`).model.state.status, "complete");
  } finally {
    rmSync(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test("ZapMockAgent help is explicit and starts no managed process", async () => {
  let output = "";
  const exit = await runZapMockAgentCli(["--help"], {
    output: {
      write(value) {
        output += String(value);
        return true;
      },
    },
  });
  assert.equal(exit, 0);
  assert.match(output, /0 LLM inference/);
  assert.match(output, /zap-mock\/deterministic-v1/);
});

async function spawnManaged(input: {
  readonly scenarioPath: string;
  readonly mcpConfigPath: string;
  readonly inputPath: string;
  readonly sidebandPath: string;
  readonly assignment: z.infer<typeof ZapMockManagedAssignmentSchema>;
  readonly mode:
    | { readonly kind: "instructions"; readonly value: string }
    | { readonly kind: "resume"; readonly value: string };
}) {
  const factory = await createOptionalNodePtyFactory(true);
  assert.equal(factory.ok, true);
  if (!factory.ok) throw new Error();
  const args = [
    "--experimental-strip-types",
    entry,
    "managed",
    "--scenario",
    input.scenarioPath,
    "--model",
    "zap-mock/deterministic-v1",
    "--mcp-config",
    input.mcpConfigPath,
    "--assignment",
    JSON.stringify(input.assignment),
    "--sideband",
    input.sidebandPath,
    "--input",
    input.inputPath,
    "--session",
    input.assignment.sessionId,
    input.mode.kind === "instructions" ? "--instructions" : "--resume",
    input.mode.value,
  ];
  const launched = await factory.value.spawn({
    terminalId: input.assignment.terminalId,
    projectId: "project.mock.fixture",
    contextId: "context.mock.fixture",
    sessionId: input.assignment.sessionId,
    runId: input.assignment.runId,
    executable: process.execPath,
    args,
    cwd: root,
    env: safeEnvironment(),
  });
  assert.equal(launched.ok, true);
  if (!launched.ok) throw new Error();
  let output = "";
  const unsubscribeData = launched.value.onData((data) => {
    output += data;
  });
  let unsubscribeExit = (): void => undefined;
  const exit = new Promise<number | null>((resolve) => {
    unsubscribeExit = launched.value.onExit(resolve);
  });
  return {
    process: launched.value,
    exit,
    output: () => output,
    close() {
      unsubscribeData();
      unsubscribeExit();
      launched.value.stop();
    },
  };
}

function sideband(path: string) {
  return readFileSync(path, "utf8")
    .split(/\r?\n/)
    .filter((line) => line !== "")
    .flatMap((line) => {
      try {
        const parsed = ZapMockManagedSidebandEventSchema.safeParse(JSON.parse(line));
        return parsed.success ? [parsed.data] : [];
      } catch {
        return [];
      }
    });
}

function processSnapshot(path: string) {
  const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
  return z.object({ model: ZapMockSnapshotSchema }).loose().parse(raw);
}

async function until(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.fail();
}

function safeEnvironment(): Record<string, string> {
  const allowed = new Set(["PATH", "Path", "PATHEXT", "SystemRoot", "WINDIR", "TEMP", "TMP"]);
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined && allowed.has(entry[0]),
    ),
  );
}
