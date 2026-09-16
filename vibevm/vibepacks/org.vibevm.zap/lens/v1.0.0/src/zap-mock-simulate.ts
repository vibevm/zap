#!/usr/bin/env node
/** Zero-LLM simulation corpus CLI. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import { pathToFileURL } from "node:url";
import {
  loadMockSimulationCorpus,
  runMockSimulations,
  type MockSimulationRunInput,
} from "./cells/mock-simulation/index.ts";

export async function runZapMockSimulationCli(
  argv: readonly string[],
  output: Pick<NodeJS.WriteStream, "write"> = process.stdout,
  error: Pick<NodeJS.WriteStream, "write"> = process.stderr,
): Promise<number> {
  const parsed = parse(argv);
  if (!parsed.ok) {
    error.write(`${parsed.message}\n${help()}`);
    return 2;
  }
  if (parsed.value.mode === "help") {
    output.write(help());
    return 0;
  }
  const corpus = loadMockSimulationCorpus();
  if (!corpus.ok) {
    error.write(`${JSON.stringify(corpus.error)}\n`);
    return 2;
  }
  if (parsed.value.mode === "list") {
    output.write(
      `${JSON.stringify({
        scenarios: corpus.value.documents.map((document) => ({
          scenarioId: document.scenarioId,
          runnerId: document.runnerId,
          kind: document.kind,
          tags: document.tags,
        })),
        zeroLlmInference: true,
      })}\n`,
    );
    return 0;
  }
  if (parsed.value.mode === "coverage") {
    output.write(
      `${JSON.stringify({ coverage: corpus.value.registry.coverage, gaps: corpus.value.gaps })}\n`,
    );
    return 0;
  }
  const result = await runMockSimulations(parsed.value.input);
  if (!result.ok) {
    error.write(`${JSON.stringify(result.error)}\n`);
    return 2;
  }
  output.write(`${JSON.stringify(result.value)}\n`);
  return result.value.passed ? 0 : 1;
}

type Parsed =
  | { readonly mode: "help" }
  | { readonly mode: "list" }
  | { readonly mode: "coverage" }
  | { readonly mode: "run"; readonly input: MockSimulationRunInput };

function parse(
  argv: readonly string[],
):
  | { readonly ok: true; readonly value: Parsed }
  | { readonly ok: false; readonly message: string } {
  if (argv.length === 0 || argv[0] === "--help" || argv[0] === "help")
    return { ok: true, value: { mode: "help" } };
  let all = false;
  let list = false;
  let coverage = false;
  let repeat = 1;
  let requireCompleteCoverage = false;
  let seed: string | undefined;
  const ids: string[] = [];
  const tags: string[] = [];
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--all") all = true;
    else if (argument === "--require-complete-coverage") requireCompleteCoverage = true;
    else if (argument === "--list") list = true;
    else if (argument === "--coverage") coverage = true;
    else if (
      argument === "--id" ||
      argument === "--tag" ||
      argument === "--repeat" ||
      argument === "--seed"
    ) {
      const value = argv[++index];
      if (value === undefined) return { ok: false, message: `missing value for ${argument}` };
      if (argument === "--id") ids.push(value);
      else if (argument === "--tag") tags.push(value);
      else if (argument === "--seed") seed = value;
      else {
        repeat = Number(value);
        if (!Number.isInteger(repeat) || repeat < 1 || repeat > 10_000)
          return { ok: false, message: "repeat must be an integer from 1 to 10000" };
      }
    } else return { ok: false, message: `unknown argument ${String(argument)}` };
  }
  const modes = Number(all) + Number(list) + Number(coverage);
  if (
    modes > 1 ||
    ((list || coverage) &&
      (ids.length > 0 ||
        tags.length > 0 ||
        seed !== undefined ||
        repeat !== 1 ||
        requireCompleteCoverage))
  )
    return { ok: false, message: "list, coverage and run selections cannot be combined" };
  if (list) return { ok: true, value: { mode: "list" } };
  if (coverage) return { ok: true, value: { mode: "coverage" } };
  return {
    ok: true,
    value: {
      mode: "run",
      input: {
        all,
        ids,
        tags,
        repeat,
        requireCompleteCoverage,
        ...(seed === undefined ? {} : { seed }),
      },
    },
  };
}

function help(): string {
  return [
    "Zap mock simulation corpus (0 LLM inference)",
    "",
    "Usage:",
    "  zap-mock-simulate --list",
    "  zap-mock-simulate --coverage",
    "  zap-mock-simulate --all [--repeat N] [--seed SEED] [--require-complete-coverage]",
    "  zap-mock-simulate (--id ID | --tag TAG)... [--repeat N] [--seed SEED]",
    "",
  ].join("\n");
}

const entry = process.argv[1];
if (entry !== undefined && import.meta.url === pathToFileURL(entry).href)
  process.exitCode = await runZapMockSimulationCli(process.argv.slice(2));
