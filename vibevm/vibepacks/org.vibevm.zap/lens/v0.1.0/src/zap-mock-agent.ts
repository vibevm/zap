#!/usr/bin/env node
/** Runnable synthetic ZapMockAgent CLI. @scope spec://org.vibevm.zap/lens/PROP-013#agent */
import { pathToFileURL } from "node:url";
import { z } from "zod";
import {
  appendZapMockTestControl,
  runZapMockManagedProcess,
} from "./cells/mock-agent/managed-process.ts";
export { createZapMockAgent } from "./cells/mock-agent/index.ts";
export type { ZapMockAgentOptions } from "./cells/mock-agent/index.ts";

const MODEL_ID = "zap-mock/deterministic-v1";
const ManagedOptionsSchema = z
  .object({
    scenario: z.string().min(1),
    model: z.literal(MODEL_ID),
    mcpConfig: z.string().min(1),
    sideband: z.string().min(1),
    input: z.string().min(1),
    session: z.string().min(3),
    assignment: z.string().min(2),
    instructions: z.string().nullable(),
    resume: z.string().min(3).nullable(),
  })
  .strict()
  .superRefine((value, context) => {
    if ((value.instructions === null) === (value.resume === null))
      context.addIssue({
        code: "custom",
        message: "managed mode requires exactly one of --instructions or --resume",
      });
  });
const ControlOptionsSchema = z
  .object({
    input: z.string().min(1),
    inputId: z.string().min(1).max(160),
    tick: z
      .string()
      .regex(/^[1-9][0-9]*$/)
      .nullable(),
    openGate: z.string().min(1).max(160).nullable(),
  })
  .strict()
  .superRefine((value, context) => {
    if ((value.tick === null) === (value.openGate === null))
      context.addIssue({ code: "custom", message: "control requires tick or open-gate" });
    if (value.tick !== null && Number(value.tick) > 1_000)
      context.addIssue({ code: "custom", message: "tick count exceeds the bounded maximum" });
  });

export async function runZapMockAgentCli(
  argv: readonly string[],
  runtime: {
    readonly output?: Pick<NodeJS.WriteStream, "write">;
    readonly error?: Pick<NodeJS.WriteStream, "write">;
    readonly signal?: AbortSignal;
  } = {},
): Promise<number> {
  const output = runtime.output ?? process.stdout;
  const error = runtime.error ?? process.stderr;
  if (argv.length === 0 || argv[0] === "--help" || argv[0] === "help") {
    output.write(helpText());
    return 0;
  }
  if (argv[0] === "control") {
    const parsed = parseControl(argv.slice(1));
    if (!parsed.ok) {
      error.write(`${parsed.message}\n${helpText()}`);
      return 2;
    }
    const written = appendZapMockTestControl(
      parsed.value.input,
      parsed.value.tick === null
        ? {
            kind: "open_gate",
            inputId: parsed.value.inputId,
            gateId: parsed.value.openGate,
          }
        : {
            kind: "tick",
            inputId: parsed.value.inputId,
            count: Number(parsed.value.tick),
          },
    );
    return written ? 0 : 3;
  }
  if (argv[0] !== "managed") {
    error.write("ZapMockAgent supports only explicit managed mode. Use --help.\n");
    return 2;
  }
  const parsed = parseManaged(argv.slice(1));
  if (!parsed.ok) {
    error.write(`${parsed.message}\n${helpText()}`);
    return 2;
  }
  let ownedController: AbortController | null = null;
  let signal = runtime.signal;
  const stop = () => ownedController?.abort();
  if (signal === undefined) {
    ownedController = new AbortController();
    signal = ownedController.signal;
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
  }
  try {
    return await runZapMockManagedProcess(
      {
        scenarioPath: parsed.value.scenario,
        mcpConfigPath: parsed.value.mcpConfig,
        sidebandPath: parsed.value.sideband,
        inputPath: parsed.value.input,
        sessionId: parsed.value.session,
        assignment: parsed.value.assignment,
        instructions: parsed.value.instructions,
        resumeSessionId: parsed.value.resume,
      },
      { signal, output },
    );
  } finally {
    if (ownedController !== null) {
      process.removeListener("SIGINT", stop);
      process.removeListener("SIGTERM", stop);
    }
  }
}

export function helpText(): string {
  return [
    "ZapMockAgent — deterministic synthetic Zap worker (0 LLM inference)",
    "",
    "Usage:",
    "  zap-mock-agent managed --scenario <file> --model zap-mock/deterministic-v1 \\",
    "    --mcp-config <file> --assignment <json> --sideband <jsonl> --input <jsonl> \\",
    "    --session <id> (--instructions <text> | --resume <session-id>)",
    "  zap-mock-agent control --input <managed-input-jsonl> --input-id <id> \\",
    "    (--tick <1..1000> | --open-gate <gate-id>)",
    "",
    "The process accepts only generated local Zap MCP configuration and never reads provider auth stores.",
    "",
  ].join("\n");
}

function parseControl(
  args: readonly string[],
):
  | { readonly ok: true; readonly value: z.infer<typeof ControlOptionsSchema> }
  | { readonly ok: false; readonly message: string } {
  const parsed = parsePairs(
    args,
    new Map([
      ["--input", "input"],
      ["--input-id", "inputId"],
      ["--tick", "tick"],
      ["--open-gate", "openGate"],
    ]),
    { tick: null, openGate: null },
  );
  if (!parsed.ok) return parsed;
  const checked = ControlOptionsSchema.safeParse(parsed.value);
  return checked.success
    ? { ok: true, value: checked.data }
    : { ok: false, message: "ZapMockAgent control arguments failed validation." };
}

function parseManaged(
  args: readonly string[],
):
  | { readonly ok: true; readonly value: z.infer<typeof ManagedOptionsSchema> }
  | { readonly ok: false; readonly message: string } {
  const names = new Map([
    ["--scenario", "scenario"],
    ["--model", "model"],
    ["--mcp-config", "mcpConfig"],
    ["--sideband", "sideband"],
    ["--input", "input"],
    ["--session", "session"],
    ["--assignment", "assignment"],
    ["--instructions", "instructions"],
    ["--resume", "resume"],
  ]);
  const parsed = parsePairs(args, names, { instructions: null, resume: null });
  if (!parsed.ok) return parsed;
  const values = parsed.value;
  const checked = ManagedOptionsSchema.safeParse(values);
  return checked.success
    ? { ok: true, value: checked.data }
    : { ok: false, message: "ZapMockAgent managed arguments failed validation." };
}

function parsePairs(
  args: readonly string[],
  names: ReadonlyMap<string, string>,
  initial: Readonly<Record<string, string | null>>,
):
  | { readonly ok: true; readonly value: Record<string, string | null> }
  | {
      readonly ok: false;
      readonly message: string;
    } {
  const values: Record<string, string | null> = { ...initial };
  const seen = new Set<string>();
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    const name = flag === undefined ? undefined : names.get(flag);
    if (name === undefined || value === undefined || seen.has(name))
      return { ok: false, message: "ZapMockAgent arguments are invalid or duplicated." };
    seen.add(name);
    values[name] = value;
  }
  return { ok: true, value: values };
}

const entry = process.argv[1];
if (entry !== undefined && import.meta.url === pathToFileURL(entry).href) {
  process.exitCode = await runZapMockAgentCli(process.argv.slice(2));
}
