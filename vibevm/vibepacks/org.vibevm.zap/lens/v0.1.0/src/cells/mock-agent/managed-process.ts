/** Runnable deterministic managed ZapMockAgent process. @scope spec://org.vibevm.zap/lens/PROP-013#agent */
import { createHash } from "node:crypto";
import { appendFileSync, readFileSync, renameSync, watch, writeFileSync } from "node:fs";
import { z } from "zod";
import {
  ZapMockManagedAssignmentSchema,
  ZapMockManagedInputSchema,
  ZapMockManagedSidebandEventSchema,
} from "../managed-work/index.ts";
import {
  createZapMockModel,
  ZapMockScenarioSchema,
  ZapMockSnapshotSchema,
  type ZapMockModel,
  type ZapMockTransition,
} from "../mock-model/index.ts";
import { openZapMockMcpSession, type ZapMockMcpSession } from "./managed-mcp.ts";

export const ZapMockScenarioFileSchema = z
  .object({
    protocol: z.literal("zap-mock-scenario/1"),
    seed: z.string().min(1).max(512),
    scenario: ZapMockScenarioSchema,
  })
  .strict();
export const ZapMockTestControlSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("tick"),
      inputId: z.string().min(1).max(160),
      count: z.number().int().min(1).max(1_000),
    })
    .strict(),
  z
    .object({
      kind: z.literal("open_gate"),
      inputId: z.string().min(1).max(160),
      gateId: z.string().min(1).max(160),
    })
    .strict(),
]);

export function appendZapMockTestControl(inputPath: string, raw: unknown): boolean {
  const command = ZapMockTestControlSchema.safeParse(raw);
  if (!command.success) return false;
  try {
    appendFileSync(`${inputPath}.control.jsonl`, `${JSON.stringify(command.data)}\n`, "utf8");
    return true;
  } catch {
    return false;
  }
}

const SnapshotEnvelopeSchema = z
  .object({
    protocol: z.literal("zap-mock-process-snapshot/1"),
    runId: z.string().min(3),
    sessionId: z.string().min(3),
    inboxCursor: z.string().regex(/^\d+$/),
    questionGroups: z.record(z.string(), z.string().min(3)),
    model: ZapMockSnapshotSchema,
  })
  .strict();

export interface ZapMockManagedProcessOptions {
  readonly scenarioPath: string;
  readonly mcpConfigPath: string;
  readonly sidebandPath: string;
  readonly inputPath: string;
  readonly sessionId: string;
  readonly assignment: unknown;
  readonly instructions: string | null;
  readonly resumeSessionId: string | null;
}

export async function runZapMockManagedProcess(
  options: ZapMockManagedProcessOptions,
  runtime: {
    readonly signal: AbortSignal;
    readonly output?: Pick<NodeJS.WriteStream, "write">;
  },
): Promise<number> {
  const configured = loadConfiguration(options);
  if (!configured.ok) return 2;
  const mcp = await openZapMockMcpSession(options.mcpConfigPath);
  if (!mcp.ok) return 3;
  const output = runtime.output ?? process.stdout;
  try {
    const process = new ManagedMockProcess({
      ...configured.value,
      mcp: mcp.value,
      sidebandPath: options.sidebandPath,
      inputPath: options.inputPath,
      output,
    });
    const started = await process.start(options.instructions, options.resumeSessionId);
    if (!started) return 4;
    await process.observe(runtime.signal);
    return 0;
  } finally {
    await mcp.value.close();
  }
}

interface ProcessConfiguration {
  readonly assignment: z.infer<typeof ZapMockManagedAssignmentSchema>;
  readonly scenario: z.infer<typeof ZapMockScenarioFileSchema>;
  readonly snapshotPath: string;
  readonly restored: z.infer<typeof SnapshotEnvelopeSchema> | null;
}

class ManagedMockProcess {
  readonly #assignment: ProcessConfiguration["assignment"];
  readonly #scenario: ProcessConfiguration["scenario"];
  readonly #snapshotPath: string;
  readonly #restored: ProcessConfiguration["restored"];
  readonly #mcp: ZapMockMcpSession;
  readonly #sidebandPath: string;
  readonly #inputPath: string;
  readonly #controlPath: string;
  readonly #output: Pick<NodeJS.WriteStream, "write">;
  readonly #questionGroups: Record<string, string>;
  readonly #completion: Promise<void>;
  #complete: () => void = () => undefined;
  #model: ZapMockModel | null = null;
  #lines = 0;
  #controlLines = 0;
  #inboxCursor: string;
  #activeTurnId: string | null = null;
  #activeCorrelation: string | null = null;

  constructor(
    input: ProcessConfiguration & {
      readonly mcp: ZapMockMcpSession;
      readonly sidebandPath: string;
      readonly inputPath: string;
      readonly output: Pick<NodeJS.WriteStream, "write">;
    },
  ) {
    this.#assignment = input.assignment;
    this.#scenario = input.scenario;
    this.#snapshotPath = input.snapshotPath;
    this.#restored = input.restored;
    this.#mcp = input.mcp;
    this.#sidebandPath = input.sidebandPath;
    this.#inputPath = input.inputPath;
    this.#controlPath = `${input.inputPath}.control.jsonl`;
    this.#output = input.output;
    this.#questionGroups = { ...(input.restored?.questionGroups ?? {}) };
    this.#inboxCursor = input.restored?.inboxCursor ?? "0";
    this.#completion = new Promise((resolve) => {
      this.#complete = resolve;
    });
    writeFileSync(this.#controlPath, "", { encoding: "utf8", mode: 0o600 });
  }

  async start(instructions: string | null, resumeSessionId: string | null): Promise<boolean> {
    const model = createZapMockModel({
      seed: this.#scenario.seed,
      scenario: this.#scenario.scenario,
      ...(this.#restored === null ? {} : { snapshot: this.#restored.model }),
    });
    if (!model.ok) return false;
    this.#model = model.value;
    if (resumeSessionId === null) {
      const started = await this.#dispatch(
        { kind: "start", inputId: `input.start.${this.#assignment.runId}` },
        null,
      );
      if (!started) return false;
    } else if (resumeSessionId !== this.#assignment.sessionId || this.#restored === null) {
      return false;
    } else {
      if (this.#model.state().status === "paused") {
        const continued = await this.#dispatch(
          { kind: "continue", inputId: `input.resume.${this.#assignment.expectedProcessEpoch}` },
          null,
        );
        if (!continued) return false;
      }
      const step = this.#model.snapshot().scenario.steps[this.#model.state().cursor];
      if (step?.kind === "restart") {
        const restarted = await this.#dispatch(
          { kind: "restart", inputId: `input.restart.${this.#assignment.expectedProcessEpoch}` },
          null,
        );
        if (!restarted) return false;
      }
    }
    this.#sideband("session_ready", null, null);
    this.#output.write(`[ZapMockAgent ${this.#assignment.runId}] ready (0 LLM inference)\n`);
    if (this.#model.state().status === "complete") this.#complete();
    if (instructions !== null && instructions !== "")
      return this.#dispatch(
        {
          kind: "message",
          inputId: `input.instructions.${this.#assignment.runId}`,
          messageId: `message.instructions.${digest(this.#assignment.runId)}`,
          text: instructions,
        },
        null,
      );
    return true;
  }

  async observe(signal: AbortSignal): Promise<void> {
    await this.#readInputs();
    await this.#readControls();
    if (signal.aborted) {
      this.#sideband("session_exited", null, null);
      return;
    }
    const watcher = watch(this.#inputPath, () => {
      this.#queueRead();
    });
    const controlWatcher = watch(this.#controlPath, () => {
      this.#queueControlRead();
    });
    await Promise.race([aborted(signal), this.#completion]);
    watcher.close();
    controlWatcher.close();
    await this.#queue;
    this.#sideband("session_exited", null, null);
  }

  #queue: Promise<void> = Promise.resolve();
  #queueRead(): void {
    this.#queue = this.#queue
      .then(() => this.#readInputs())
      .catch(() => {
        this.#sideband("delivery_rejected", null, null);
      });
  }

  #queueControlRead(): void {
    this.#queue = this.#queue
      .then(() => this.#readControls())
      .catch(() => {
        this.#sideband("delivery_rejected", null, null);
      });
  }

  async #readInputs(): Promise<void> {
    const text = readFileSync(this.#inputPath, "utf8");
    const lines = text.split(/\r?\n/);
    if (!text.endsWith("\n")) lines.pop();
    else lines.pop();
    if (lines.length < this.#lines) this.#lines = 0;
    for (const line of lines.slice(this.#lines)) await this.#input(line);
    this.#lines = lines.length;
  }

  async #readControls(): Promise<void> {
    const text = readFileSync(this.#controlPath, "utf8");
    const lines = text.split(/\r?\n/);
    if (!text.endsWith("\n")) lines.pop();
    else lines.pop();
    if (lines.length < this.#controlLines) this.#controlLines = 0;
    for (const line of lines.slice(this.#controlLines)) await this.#control(line);
    this.#controlLines = lines.length;
  }

  async #control(line: string): Promise<void> {
    let raw: unknown;
    try {
      raw = JSON.parse(line);
    } catch {
      raw = null;
    }
    const command = ZapMockTestControlSchema.safeParse(raw);
    if (!command.success) {
      this.#sideband("delivery_rejected", null, null);
      return;
    }
    await this.#dispatch(command.data, null);
  }

  async #input(line: string): Promise<void> {
    let raw: unknown;
    try {
      raw = JSON.parse(line);
    } catch {
      raw = null;
    }
    const input = ZapMockManagedInputSchema.safeParse(raw);
    if (!input.success) {
      this.#sideband("delivery_rejected", null, null);
      return;
    }
    if (input.data.kind === "interrupt") {
      await this.#dispatch(
        {
          kind: "pause",
          inputId: `input.interrupt.${digest(`${input.data.reason}:${this.#assignment.expectedProcessEpoch}`)}`,
        },
        null,
      );
      return;
    }
    const step = this.#currentStep();
    if (step?.kind === "late_answer") {
      await this.#lateAnswer(input.data.deliveryId, step.questionId, step.acknowledgementId);
      return;
    }
    await this.#dispatch(
      {
        kind: "message",
        inputId: `input.offer.${input.data.deliveryId}`,
        messageId: `message.offer.${digest(input.data.deliveryId)}`,
        text: input.data.bodyMarkdown,
      },
      input.data.deliveryId,
    );
  }

  async #lateAnswer(
    deliveryCorrelation: string,
    logicalQuestionId: string,
    acknowledgementId: string,
  ): Promise<void> {
    const actualQuestionId = this.#questionGroups[logicalQuestionId];
    if (actualQuestionId === undefined) {
      this.#sideband("delivery_rejected", null, deliveryCorrelation);
      return;
    }
    const inbox = await this.#mcp.inbox(this.#inboxCursor);
    if (!inbox.ok) {
      this.#sideband("delivery_rejected", null, deliveryCorrelation);
      return;
    }
    this.#inboxCursor = inbox.value.observationCursor;
    const delivery = inbox.value.deliveries.find(
      (candidate) =>
        candidate.acknowledgedAt === null && candidate.message.correlationId === actualQuestionId,
    );
    if (delivery === undefined) {
      this.#sideband("delivery_rejected", null, deliveryCorrelation);
      this.#save();
      return;
    }
    const turnId = nativeId("turn", this.#assignment.sessionId, deliveryCorrelation);
    this.#activeTurnId = turnId;
    this.#activeCorrelation = deliveryCorrelation;
    this.#sideband("turn_started", turnId, deliveryCorrelation);
    const answerVersion = answerVersionFrom(delivery.message.payload, delivery.message.messageId);
    const answered = await this.#dispatch(
      {
        kind: "answer",
        inputId: `input.answer.${delivery.deliveryId}`,
        questionId: logicalQuestionId,
        answerVersion,
        text: JSON.stringify(delivery.message.payload),
      },
      deliveryCorrelation,
    );
    if (!answered) return;
    const acknowledged = await this.#mcp.acknowledge([delivery.deliveryId]);
    if (!acknowledged.ok) {
      this.#sideband("delivery_rejected", turnId, deliveryCorrelation);
      return;
    }
    const acked = await this.#dispatch(
      {
        kind: "ack",
        inputId: `input.ack.${delivery.deliveryId}`,
        acknowledgementId,
        answerVersion,
      },
      deliveryCorrelation,
    );
    if (!acked) return;
    await this.#reportIfRequired();
    this.#sideband("turn_settled", turnId, deliveryCorrelation);
    this.#activeTurnId = null;
    this.#activeCorrelation = null;
    this.#save();
  }

  async #reportIfRequired(): Promise<void> {
    const step = this.#currentStep();
    if (step?.kind !== "report") return;
    const run = await this.#mcp.readRun(this.#assignment.runId);
    if (!run.ok) return;
    const summary = `ZapMock deterministic report for ${this.#assignment.runId}`;
    const reported = await this.#mcp.report({
      runId: this.#assignment.runId,
      expectedRevision: run.value.revision,
      summaryMarkdown: summary,
    });
    if (!reported.ok) return;
    await this.#dispatch(
      {
        kind: "report",
        inputId: `input.report.${step.reportId}`,
        reportId: step.reportId,
        summary,
      },
      this.#activeCorrelation,
    );
  }

  async #dispatch(input: unknown, correlation: string | null): Promise<boolean> {
    if (this.#model === null) return false;
    const before = this.#model.snapshot();
    const result = this.#model.dispatch(input);
    if (!result.ok) {
      this.#sideband("delivery_rejected", this.#activeTurnId, correlation);
      return false;
    }
    if (!(await this.#effects(result.value, correlation))) {
      const restored = createZapMockModel({
        seed: this.#scenario.seed,
        scenario: this.#scenario.scenario,
        snapshot: before,
      });
      if (restored.ok) this.#model = restored.value;
      return false;
    }
    this.#save();
    if (this.#model.state().status === "complete") this.#complete();
    return true;
  }

  async #effects(transition: ZapMockTransition, correlation: string | null): Promise<boolean> {
    for (const effect of transition.effects) {
      if (effect.kind === "turn_started") {
        this.#activeTurnId = nativeId("turn", this.#assignment.sessionId, effect.effectId);
        this.#activeCorrelation = correlation;
        this.#sideband("turn_started", this.#activeTurnId, correlation);
      } else if (effect.kind === "assistant_message") {
        this.#output.write(`[ZapMockAgent] ${effect.text}\n`);
      } else if (effect.kind === "question") {
        const asked = await this.#mcp.ask(effect);
        if (!asked.ok) {
          this.#sideband("delivery_rejected", this.#activeTurnId, correlation);
          return false;
        }
        this.#questionGroups[effect.questionId] = asked.value;
        this.#output.write(`[ZapMockAgent] question sent; turn is idle\n`);
      } else if (effect.kind === "turn_settled") {
        this.#sideband("turn_settled", this.#activeTurnId, this.#activeCorrelation);
        this.#activeTurnId = null;
        this.#activeCorrelation = null;
      } else if (effect.kind === "paused") {
        this.#sideband("turn_settled", this.#activeTurnId, this.#activeCorrelation);
      } else if (effect.kind === "delivery" && effect.observation === "uncertain") {
        this.#sideband("delivery_rejected", this.#activeTurnId, effect.deliveryId);
      } else if (effect.kind === "work_reported") {
        this.#output.write(`[ZapMockAgent] report accepted for ${effect.reportId}\n`);
      }
    }
    return true;
  }

  #currentStep() {
    if (this.#model === null) return undefined;
    const snapshot = this.#model.snapshot();
    return snapshot.scenario.steps[snapshot.state.cursor];
  }

  #save(): void {
    if (this.#model === null) return;
    const snapshot = SnapshotEnvelopeSchema.parse({
      protocol: "zap-mock-process-snapshot/1",
      runId: this.#assignment.runId,
      sessionId: this.#assignment.sessionId,
      inboxCursor: this.#inboxCursor,
      questionGroups: this.#questionGroups,
      model: this.#model.snapshot(),
    });
    const temporary = `${this.#snapshotPath}.next`;
    writeFileSync(temporary, JSON.stringify(snapshot), { encoding: "utf8", mode: 0o600 });
    renameSync(temporary, this.#snapshotPath);
  }

  #sideband(
    kind: z.infer<typeof ZapMockManagedSidebandEventSchema>["kind"],
    providerTurnId: string | null,
    transportCorrelation: string | null,
  ): void {
    const event = ZapMockManagedSidebandEventSchema.parse({
      kind,
      providerSessionId: this.#assignment.sessionId,
      providerTurnId,
      transportCorrelation,
    });
    appendFileSync(this.#sidebandPath, `${JSON.stringify(event)}\n`, "utf8");
  }
}

function loadConfiguration(
  options: ZapMockManagedProcessOptions,
): { readonly ok: true; readonly value: ProcessConfiguration } | { readonly ok: false } {
  let scenarioRaw: unknown;
  let assignmentRaw: unknown = options.assignment;
  try {
    scenarioRaw = JSON.parse(readFileSync(options.scenarioPath, "utf8"));
    if (typeof options.assignment === "string") assignmentRaw = JSON.parse(options.assignment);
  } catch {
    return { ok: false };
  }
  const scenario = ZapMockScenarioFileSchema.safeParse(scenarioRaw);
  const assignment = ZapMockManagedAssignmentSchema.safeParse(assignmentRaw);
  if (!scenario.success || !assignment.success || assignment.data.sessionId !== options.sessionId)
    return { ok: false };
  const snapshotPath = `${options.sidebandPath}.snapshot.json`;
  let restored: z.infer<typeof SnapshotEnvelopeSchema> | null = null;
  if (options.resumeSessionId !== null) {
    try {
      const parsed: unknown = JSON.parse(readFileSync(snapshotPath, "utf8"));
      const checked = SnapshotEnvelopeSchema.safeParse(parsed);
      if (
        !checked.success ||
        checked.data.runId !== assignment.data.runId ||
        checked.data.sessionId !== assignment.data.sessionId
      )
        return { ok: false };
      restored = checked.data;
    } catch {
      return { ok: false };
    }
  }
  return {
    ok: true,
    value: { assignment: assignment.data, scenario: scenario.data, snapshotPath, restored },
  };
}

function answerVersionFrom(payload: unknown, fallback: string): string {
  const parsed = z
    .looseObject({ answerVersionId: z.string().min(3).optional() })
    .safeParse(payload);
  return parsed.success && parsed.data.answerVersionId !== undefined
    ? parsed.data.answerVersionId
    : fallback;
}

function nativeId(prefix: string, sessionId: string, basis: string): string {
  return `${prefix}.mock.${digest(`${sessionId}\u0000${basis}`)}`;
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 24);
}

function aborted(signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    signal.addEventListener(
      "abort",
      () => {
        resolve();
      },
      { once: true },
    );
  });
}
