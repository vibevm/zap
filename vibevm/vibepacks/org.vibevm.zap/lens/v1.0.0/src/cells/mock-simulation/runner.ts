/** Deterministic corpus selection and execution. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import { resolve } from "node:path";
import { z } from "zod";
import { createZapMockModel } from "../mock-model/index.ts";
import { loadMockSimulationCorpus, type MockCorpusResult } from "./corpus.ts";
import type { MockSimulationDocument, MockSimulationRunnerId } from "./schemas.ts";
import { runOwnedProductProcess } from "./product-process.ts";

export type MockSimulationEvidenceKind =
  | "pure_model"
  | "managed_product"
  | "coordinator_http"
  | "workspace_store"
  | "repository_git"
  | "portfolio_repository"
  | "public_service"
  | "public_runtime";

export interface MockSimulationRunInput {
  readonly root?: string;
  readonly all?: boolean;
  readonly ids?: readonly string[];
  readonly tags?: readonly string[];
  readonly repeat?: number;
  readonly seed?: string;
  readonly requireCompleteCoverage?: boolean;
}

export interface MockSimulationScenarioReceipt {
  readonly scenarioId: string;
  readonly runnerId: MockSimulationRunnerId;
  readonly evidenceKind: MockSimulationEvidenceKind;
  readonly iteration: number;
  readonly seed: string;
  readonly passed: boolean;
  readonly inputPosition: number | null;
  readonly message: string;
  readonly replay: { readonly command: "npm"; readonly args: readonly string[] };
}

export interface MockSimulationRunReceipt {
  readonly passed: boolean;
  readonly checksPassed: boolean;
  readonly coverageComplete: boolean;
  readonly selectedScenarioIds: readonly string[];
  readonly repetitions: number;
  readonly scenarios: readonly MockSimulationScenarioReceipt[];
  readonly coverageGaps: readonly { readonly feature: string; readonly gap: string }[];
  readonly zeroLlmInference: true;
}

const RunInputSchema = z
  .object({
    root: z.string().min(1).optional(),
    all: z.boolean().default(false),
    ids: z.array(z.string().min(3).max(160)).max(10_000).default([]),
    tags: z.array(z.string().min(1).max(80)).max(1_000).default([]),
    repeat: z.number().int().min(1).max(10_000).default(1),
    seed: z.string().min(1).max(512).optional(),
    requireCompleteCoverage: z.boolean().default(false),
  })
  .strict();

const PRODUCT_RUNNERS: Readonly<Partial<Record<MockSimulationRunnerId, string>>> = {
  "wayfinder.mock-managed": "src/cells/wayfinder-runtime/mock-managed.test.ts",
  coordinator_http: "src/cells/mock-agent/question-http.test.ts",
  "workspace.store-plan-context": "src/cells/workspace-store/plan-context.test.ts",
  "repository.git": "src/cells/repository-workspaces/simulation.test.ts",
  "quicklens.portfolio-repository": "src/cells/quicklens-graph/portfolio-repository.test.ts",
  "workspace.annotations-product": "src/cells/workspace-annotations/product-simulation.test.ts",
  "workspace.model-policy-product": "src/cells/model-policy-service/product-simulation.test.ts",
  "wayfinder.repository-product": "src/cells/wayfinder-runtime/repository-product.test.ts",
  "wayfinder.repository-managed-product":
    "src/cells/wayfinder-runtime/repository-managed-product.test.ts",
  "provider-coordinator.public-projection": "src/cells/provider-coordinators/index.test.ts",
  "execution-catalog.public-routing": "src/cells/execution-catalog/product-simulation.test.ts",
  "execution-catalog.managed-runtime":
    "src/cells/wayfinder-runtime/execution-catalog-runtime.test.ts",
  "source-install.public-lifecycle": "tooling/source-install/product-simulation.js",
};

export async function runMockSimulations(
  input: MockSimulationRunInput,
): Promise<MockCorpusResult<MockSimulationRunReceipt>> {
  const parsed = RunInputSchema.safeParse(input);
  if (!parsed.success) return failure("invalid_selection", "simulation selection is invalid");
  const corpus = loadMockSimulationCorpus(parsed.data.root);
  if (!corpus.ok) return corpus;
  const ids = new Set(parsed.data.ids);
  const tags = new Set(parsed.data.tags);
  if (parsed.data.ids.some((id) => !corpus.value.documents.some((item) => item.scenarioId === id)))
    return failure("unknown_scenario", "selected simulation ID is unknown");
  if (
    parsed.data.tags.some((tag) => !corpus.value.documents.some((item) => item.tags.includes(tag)))
  )
    return failure("unknown_tag", "selected simulation tag is unknown");
  const selected = corpus.value.documents.filter(
    (document) =>
      parsed.data.all ||
      ((ids.size === 0 || ids.has(document.scenarioId)) &&
        (tags.size === 0 || document.tags.some((tag) => tags.has(tag)))),
  );
  if (selected.length === 0 || (!parsed.data.all && ids.size === 0 && tags.size === 0))
    return failure("empty_selection", "simulation selection matched no scenarios");
  const receipts: MockSimulationScenarioReceipt[] = [];
  for (const document of selected)
    for (let iteration = 0; iteration < parsed.data.repeat; iteration += 1) {
      const baseSeed = parsed.data.seed ?? seedOf(document);
      const seed = parsed.data.repeat === 1 ? baseSeed : `${baseSeed}#${String(iteration + 1)}`;
      receipts.push(await runOne(corpus.value.root, document, seed, iteration + 1));
    }
  return {
    ok: true,
    value: {
      passed:
        receipts.every((receipt) => receipt.passed) &&
        (!parsed.data.requireCompleteCoverage || corpus.value.gaps.length === 0),
      checksPassed: receipts.every((receipt) => receipt.passed),
      coverageComplete: corpus.value.gaps.length === 0,
      selectedScenarioIds: selected.map((document) => document.scenarioId),
      repetitions: parsed.data.repeat,
      scenarios: receipts,
      coverageGaps: corpus.value.gaps,
      zeroLlmInference: true,
    },
  };
}

async function runOne(
  root: string,
  document: MockSimulationDocument,
  seed: string,
  iteration: number,
): Promise<MockSimulationScenarioReceipt> {
  const evidenceKind = evidence(document.runnerId);
  const replay = {
    command: "npm" as const,
    args: [
      "run",
      "simulate:mock",
      "--",
      "--id",
      document.scenarioId,
      "--seed",
      seed,
      "--repeat",
      "1",
    ],
  };
  if (document.kind === "model_reducer") {
    const model = createZapMockModel({
      seed,
      scenario: document.scenarioFile.scenario,
    });
    if (!model.ok)
      return receipt(
        document,
        evidenceKind,
        iteration,
        seed,
        false,
        null,
        model.error.message,
        replay,
      );
    for (const [position, input] of document.inputTape.entries()) {
      const transition = model.value.dispatch(input);
      if (!transition.ok)
        return receipt(
          document,
          evidenceKind,
          iteration,
          seed,
          false,
          position,
          transition.error.message,
          replay,
        );
      const actual = transition.value.effects.map((effect) => effect.kind);
      const expected = document.expected.effectKindsByInput[position];
      if (JSON.stringify(actual) !== JSON.stringify(expected))
        return receipt(
          document,
          evidenceKind,
          iteration,
          seed,
          false,
          position,
          "effect expectation mismatch",
          replay,
        );
    }
    const state = model.value.state();
    const expectedState = document.expected.finalState;
    const passed =
      state.status === expectedState.status &&
      state.cursor === expectedState.cursor &&
      state.logicalTick === expectedState.logicalTick &&
      state.generation === expectedState.generation &&
      model.value.trace().length === document.expected.traceLength;
    return receipt(
      document,
      evidenceKind,
      iteration,
      seed,
      passed,
      passed ? null : document.inputTape.length,
      passed ? "passed" : "final state expectation mismatch",
      replay,
    );
  }
  const path = PRODUCT_RUNNERS[document.runnerId];
  if (path === undefined)
    return receipt(
      document,
      evidenceKind,
      iteration,
      seed,
      false,
      null,
      "known product runner is not registered",
      replay,
    );
  const product = await runProduct(root, resolve(root, path), document.scenarioId, seed, iteration);
  return receipt(
    document,
    evidenceKind,
    iteration,
    seed,
    product.ok,
    product.inputPosition,
    product.message,
    replay,
  );
}

async function runProduct(
  root: string,
  testPath: string,
  scenarioId: string,
  seed: string,
  iteration: number,
): Promise<{ readonly ok: boolean; readonly message: string; readonly inputPosition: number }> {
  const product = await runOwnedProductProcess({
    root,
    testPath,
    environment: {
      ...safeEnvironment(),
      ZAP_MOCK_SIMULATION_ID: scenarioId,
      ZAP_MOCK_SIMULATION_SEED: seed,
      ZAP_MOCK_SIMULATION_ITERATION: String(iteration),
    },
    timeoutMs: 60_000,
  });
  if (product.startError)
    return { ok: false, message: "product runner could not start", inputPosition: 0 };
  if (product.timedOut)
    return {
      ok: false,
      message: product.exactTreeTerminated
        ? "product runner timed out and its owned process tree was stopped"
        : "product runner timed out and exact process-tree cleanup failed",
      inputPosition: 0,
    };
  const observed = productReceipt(product.output, scenarioId, seed);
  return product.exitCode === 0 && observed
    ? { ok: true, message: "passed", inputPosition: observed.inputPosition }
    : {
        ok: false,
        message:
          product.exitCode === 0
            ? "product runner omitted its scenario receipt"
            : `product runner failed: ${product.output.slice(-2_000)}`,
        inputPosition: observed?.inputPosition ?? 0,
      };
}

function productReceipt(
  output: string,
  scenarioId: string,
  seed: string,
): { readonly inputPosition: number } | null {
  const line = output
    .split(/\r?\n/)
    .find((candidate) => candidate.includes("ZAP_MOCK_SIMULATION_RECEIPT "));
  if (line === undefined) return null;
  try {
    const marker = "ZAP_MOCK_SIMULATION_RECEIPT ";
    const raw: unknown = JSON.parse(line.slice(line.indexOf(marker) + marker.length));
    const parsed = z
      .object({
        scenarioId: z.literal(scenarioId),
        seed: z.literal(seed),
        passed: z.literal(true),
        inputPosition: z.number().int().min(0),
        zeroLlmInference: z.literal(true),
      })
      .strict()
      .safeParse(raw);
    return parsed.success ? { inputPosition: parsed.data.inputPosition } : null;
  } catch {
    return null;
  }
}

function seedOf(document: MockSimulationDocument): string {
  return document.kind === "model_reducer" ||
    document.kind === "managed_product" ||
    document.kind === "coordinator_http"
    ? document.scenarioFile.seed
    : document.seed;
}

function evidence(runnerId: MockSimulationRunnerId): MockSimulationEvidenceKind {
  if (runnerId === "model.reducer") return "pure_model";
  if (runnerId === "wayfinder.mock-managed") return "managed_product";
  if (runnerId === "workspace.store-plan-context") return "workspace_store";
  if (runnerId === "quicklens.portfolio-repository") return "portfolio_repository";
  if (
    runnerId === "workspace.annotations-product" ||
    runnerId === "workspace.model-policy-product" ||
    runnerId === "provider-coordinator.public-projection" ||
    runnerId === "execution-catalog.public-routing" ||
    runnerId === "source-install.public-lifecycle"
  )
    return "public_service";
  if (
    runnerId === "wayfinder.repository-product" ||
    runnerId === "wayfinder.repository-managed-product"
  )
    return "public_runtime";
  if (runnerId === "execution-catalog.managed-runtime") return "public_runtime";
  return runnerId === "repository.git" ? "repository_git" : "coordinator_http";
}

function receipt(
  document: MockSimulationDocument,
  evidenceKind: MockSimulationEvidenceKind,
  iteration: number,
  seed: string,
  passed: boolean,
  inputPosition: number | null,
  message: string,
  replay: MockSimulationScenarioReceipt["replay"],
): MockSimulationScenarioReceipt {
  return {
    scenarioId: document.scenarioId,
    runnerId: document.runnerId,
    evidenceKind,
    iteration,
    seed,
    passed,
    inputPosition,
    message,
    replay,
  };
}

function safeEnvironment(): Record<string, string> {
  const allowed = new Set(["SystemRoot", "WINDIR", "PATH", "Path", "PATHEXT", "TEMP", "TMP"]);
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined && allowed.has(entry[0]),
    ),
  );
}

function failure(code: string, message: string): MockCorpusResult<never> {
  return { ok: false, error: { code, message } };
}
