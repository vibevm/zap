/** Composition of the reader, data and coordinator authoring ports. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#root */
import type { ZodType } from "zod";
import type {
  CanonicalJsonInput,
  PrepareBundleRequest,
  PrepareComparisonRequest,
  PrepareProjectedRecordRequest,
  ZapId,
  ZapQueryPage,
} from "../zap-client/index.ts";
import { discover } from "./context.ts";
import { prepareCompositeAssessment, prepareCompositeSuccessor } from "./composite.ts";
import { prepareEffect } from "./effect.ts";
import { finishSuccessor, prepareAssessment, prepareSuccessor } from "./metadata.ts";
import { prepareMilestonePrecursors } from "./milestones.ts";
import {
  PreparedAssessmentProposalSchema,
  PreparedCommandSchema,
  PreparedCompositePlanProposalSchema,
  PreparedMilestonePrecursorsSchema,
  PreparedPlanProposalSchema,
  MilestonePrecursorAuthoringInputSchema,
  SuccessorPlanAuthoringInputSchema,
} from "./schemas.ts";
import type {
  MetadataReceipt,
  MilestonePrecursorAuthoringInput,
  PlanAuthoringOptions,
  PlanAuthoringPort,
  PlanAuthoringResult,
  PrepareEffectInput,
  PreparedAdmissionStep,
  PreparedAssessmentProposal,
  PreparedCommand,
  PreparedCompositePlanProposal,
  PreparedMilestonePrecursors,
  PreparedPlanProposal,
  PreparedSuccessor,
  SuccessorPlanAuthoringInput,
} from "./types.ts";
import { fail } from "./wire.ts";

export function createPlanAuthoring(options: PlanAuthoringOptions): PlanAuthoringPort {
  return new Authoring(options);
}

class Authoring implements PlanAuthoringPort {
  readonly #options: PlanAuthoringOptions;

  constructor(options: PlanAuthoringOptions) {
    this.#options = options;
  }

  discover() {
    return discover(this.#options.reader, this.#options.specifications);
  }

  async query<T>(
    queryId: ZapId,
    input: CanonicalJsonInput,
    schema: ZodType<T>,
  ): Promise<PlanAuthoringResult<ZapQueryPage<T>>> {
    const result = await this.#options.reader.query(queryId, input, schema);
    return result.ok
      ? { ok: true, value: result.value }
      : fail("unavailable", `Registered ZAP query failed: ${result.error.kind}`);
  }

  prepareBundle(request: PrepareBundleRequest) {
    return this.#options.reader.prepareBundle(request);
  }

  prepareComparison(request: PrepareComparisonRequest) {
    return this.#options.reader.prepareComparison(request);
  }

  prepareProjectedRecord(request: PrepareProjectedRecordRequest) {
    return this.#options.reader.prepareProjectedRecord(request);
  }

  async prepareSuccessor(
    input: SuccessorPlanAuthoringInput,
  ): Promise<PlanAuthoringResult<PreparedPlanProposal>> {
    const parsed = SuccessorPlanAuthoringInputSchema.safeParse(input);
    if (!parsed.success) return fail("invalid_input", "Successor semantic input is invalid");
    const context = await this.discover();
    return context.ok ? prepareSuccessor(this.#options, context.value, parsed.data) : context;
  }

  async prepareMilestonePrecursors(input: MilestonePrecursorAuthoringInput) {
    const parsed = MilestonePrecursorAuthoringInputSchema.safeParse(input);
    if (!parsed.success) return fail("invalid_input", "Milestone precursor input is invalid");
    const context = await this.discover();
    return context.ok
      ? prepareMilestonePrecursors(this.#options, context.value, parsed.data)
      : context;
  }

  async prepareCompositeSuccessor(
    input: SuccessorPlanAuthoringInput,
    precursors: PreparedMilestonePrecursors,
  ) {
    const parsedInput = SuccessorPlanAuthoringInputSchema.safeParse(input);
    const parsedPrecursors = PreparedMilestonePrecursorsSchema.safeParse(precursors);
    if (!parsedInput.success || !parsedPrecursors.success) {
      return fail("invalid_input", "Composite successor input is invalid");
    }
    const context = await this.discover();
    return context.ok
      ? prepareCompositeSuccessor(
          this.#options,
          context.value,
          parsedInput.data,
          parsedPrecursors.data,
        )
      : context;
  }

  async recordCompositeSuccessor(prepared: PreparedCompositePlanProposal) {
    const parsed = PreparedCompositePlanProposalSchema.safeParse(prepared);
    if (!parsed.success) return fail("invalid_input", "Prepared composite successor is invalid");
    const result = await this.#options.data.recordCompositeSuccessor(parsed.data.composite);
    return result.ok
      ? success(result.value)
      : fail(
          result.error.kind === "uncertain_submission" ? "uncertain" : "refused",
          `Composite successor recording failed: ${
            result.error.kind === "http_refusal"
              ? `${result.error.refusal.code}: ${result.error.refusal.message}`
              : result.error.kind
          }`,
        );
  }

  async submitMetadata(command: PreparedCommand) {
    const parsed = PreparedCommandSchema.safeParse(command);
    if (!parsed.success) return fail("invalid_input", "Prepared metadata command is invalid");
    const result = await this.#options.data.submit("agent", {
      command: parsed.data.command,
      reconciliation: {
        command_id: parsed.data.command.frame.header.command_id,
        command_digest: parsed.data.commandDigest,
      },
    });
    return result.ok
      ? success(result.value)
      : fail(
          result.error.kind === "uncertain_submission" ? "uncertain" : "refused",
          `Metadata submission failed: ${result.error.kind}`,
        );
  }

  async reconcileMetadata(request: Parameters<PlanAuthoringPort["reconcileMetadata"]>[0]) {
    const result = await this.#options.reader.reconcile(request);
    return result.ok
      ? success(result.value)
      : fail(
          "unavailable",
          `Metadata reconciliation failed: ${
            result.error.kind === "http_refusal" ? result.error.refusal.message : result.error.kind
          }`,
        );
  }

  async prepareAssessment(prepared: PreparedPlanProposal, receipt: MetadataReceipt) {
    const parsed = PreparedPlanProposalSchema.safeParse(prepared);
    if (!parsed.success) return fail("invalid_input", "Prepared plan proposal is invalid");
    const context = await this.discover();
    return context.ok
      ? prepareAssessment(this.#options, context.value, parsed.data, receipt)
      : context;
  }

  async prepareCompositeAssessment(
    prepared: PreparedCompositePlanProposal,
    receipt: MetadataReceipt,
  ) {
    const parsed = PreparedCompositePlanProposalSchema.safeParse(prepared);
    if (!parsed.success) return fail("invalid_input", "Prepared composite successor is invalid");
    const context = await this.discover();
    return context.ok
      ? prepareCompositeAssessment(this.#options, context.value, parsed.data, receipt)
      : context;
  }

  async finishSuccessor(prepared: PreparedAssessmentProposal, receipt: MetadataReceipt) {
    const parsed = PreparedAssessmentProposalSchema.safeParse(prepared);
    if (!parsed.success) return fail("invalid_input", "Prepared assessment proposal is invalid");
    const context = await this.discover();
    return context.ok
      ? finishSuccessor(this.#options, context.value, parsed.data, receipt)
      : context;
  }

  async prepareEffect(
    prepared: PreparedSuccessor,
    input: PrepareEffectInput,
  ): Promise<PlanAuthoringResult<PreparedAdmissionStep>> {
    const context = await this.discover();
    return context.ok ? prepareEffect(this.#options, context.value, prepared, input) : context;
  }
}

function success<T>(value: T): PlanAuthoringResult<T> {
  return { ok: true, value };
}
