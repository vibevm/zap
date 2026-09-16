/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { createHash } from "node:crypto";
import { z } from "zod";
import {
  PlanBasisSchema,
  PlanChangeSummarySchema,
  QuicklensRefSchema,
  type PlanApplyInput,
  type PlanBasis,
  type PlanDecisionInput,
  type PlanOperationResult,
  type PlanPreviewInput,
  type PlanReconcileInput,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import {
  ConversationIdSchema,
  MessageIdSchema,
  WorkspaceIdSchema,
  type MessageEnvelope,
  type PublicConnection,
  type Result,
} from "../protocol/index.ts";
import {
  AdvanceChangeAdmissionInputSchema,
  LosslessJsonSchema,
  ProtectedSubmissionSchema,
  ZapDigestSchema,
  ZapIdSchema,
  encodeCanonicalJson,
  protectedCommandDigest,
  type PreparedEffectComparisonView,
  type ProtectedSubmission,
  type SubmissionStatus,
  type ZapClient,
} from "../zap-client/index.ts";
import type { HeldDecisionView, PlanWorkflowPort } from "./types.ts";
import { deriveVerifiedPlanChanges, verifiedPreparedProductMatches } from "./verified-preview.ts";
import { operation, qerror, recordResult, sameBasis } from "./workflow-view.ts";
import {
  PreparedAdmissionStepSchema,
  PreparedSuccessorSchema,
  type PlanAuthoringPort,
} from "../plan-authoring/index.ts";
export const AgentPlanProposalSchema = z
  .object({
    intentRef: QuicklensRefSchema,
    intentMessageId: MessageIdSchema,
    operationRef: QuicklensRefSchema,
    previewRef: QuicklensRefSchema,
    intentBasis: PlanBasisSchema,
    basis: PlanBasisSchema,
    admission: AdvanceChangeAdmissionInputSchema,
    preparedSuccessor: PreparedSuccessorSchema.nullable().default(null),
    preparedStep: PreparedAdmissionStepSchema.nullable().default(null),
  })
  .strict()
  .refine((value) => (value.preparedSuccessor === null) === (value.preparedStep === null));
export type AgentPlanProposal = z.infer<typeof AgentPlanProposalSchema>;
const IntentPayloadSchema = z
  .object({
    type: z.literal("plan_intent"),
    intentRef: QuicklensRefSchema,
    text: z.string().min(1).max(16_384),
    basis: PlanBasisSchema,
  })
  .strict();
export const WorkflowPhaseSchema = z.enum([
  "prepared",
  "step_ready",
  "admission_uncertain",
  "owner_decision_required",
  "owner_decision_uncertain",
  "product_uncertain",
  "completed",
  "rejected",
]);
export type WorkflowPhase = z.infer<typeof WorkflowPhaseSchema>;
const StoredU64Schema = z.string().regex(/^(0|[1-9][0-9]*)$/);
const StoredAdmissionViewSchema = z
  .object({
    status: z.literal("owner_decision_required"),
    operation_id: ZapIdSchema,
    assessment_id: ZapIdSchema,
    alternative_id: ZapIdSchema,
    observed_revision: StoredU64Schema,
    hold_id: ZapIdSchema,
    assessment_digest: ZapDigestSchema,
    adjudication: LosslessJsonSchema,
    decision: z
      .object({
        assessment_digest: ZapDigestSchema,
        forecast_id: ZapIdSchema.nullable(),
        forecast_digest: ZapDigestSchema.nullable(),
        policy_id: ZapIdSchema,
        policy_revision: StoredU64Schema,
        recommended_alternative_id: ZapIdSchema,
        effect_fingerprints: z.array(ZapDigestSchema),
        effect_preflight_digests: z.array(ZapDigestSchema),
        decision_revision: StoredU64Schema,
      })
      .strict(),
  })
  .strict();
export const WorkflowRecordSchema = z
  .object({
    proposal: AgentPlanProposalSchema,
    proposalDigest: ZapDigestSchema,
    actorId: z.string(),
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    phase: WorkflowPhaseSchema,
    productDigest: ZapDigestSchema.nullable(),
    admissionView: StoredAdmissionViewSchema.nullable(),
    ownerSubmission: ProtectedSubmissionSchema.nullable(),
    ownerChoice: z.enum(["approve", "reject", "revise", "defer"]).nullable(),
    continuation: AdvanceChangeAdmissionInputSchema.nullable(),
    verifiedChanges: z.array(PlanChangeSummarySchema).min(1).max(200),
    effectIndex: z.number().int().min(0),
    completedPrefix: z.array(ZapIdSchema),
    currentStep: PreparedAdmissionStepSchema.nullable(),
    message: z.string(),
  })
  .strict();
export type WorkflowRecord = z.infer<typeof WorkflowRecordSchema>;
export interface PlanWorkflowStore {
  getByIntent(intentRef: string): Result<WorkflowRecord | null>;
  getByOperation(operationRef: string): Result<WorkflowRecord | null>;
  create(record: WorkflowRecord): Result<WorkflowRecord>;
  transition(
    operationRef: string,
    proposalDigest: string,
    expected: readonly WorkflowPhase[],
    next: WorkflowRecord,
  ): Result<WorkflowRecord>;
  currentDecision(): Result<WorkflowRecord | null>;
}
export interface LivePlanWorkflow extends PlanWorkflowPort {
  submitProposal(
    actor: PublicConnection,
    intent: MessageEnvelope,
    input: unknown,
  ): Promise<QuicklensResult<PlanOperationResult>>;
}
export interface HumanOwnerWorkflowPort {
  decide(record: WorkflowRecord, input: PlanDecisionInput): Promise<Result<WorkflowRecord>>;
  reconcile(record: WorkflowRecord): Promise<Result<WorkflowRecord>>;
}
export interface LivePlanWorkflowOptions {
  readonly reader: ZapClient;
  readonly coordinator: ZapClient;
  readonly store: PlanWorkflowStore;
  readonly workspaceId: string;
  readonly conversationId: string;
  readonly owner?: HumanOwnerWorkflowPort;
  readonly authoring?: PlanAuthoringPort;
  readonly validateBasis: (basis: PlanBasis) => Promise<QuicklensResult<PlanBasis>>;
  readonly validateExecution: (
    basis: PlanBasis,
    expectedRevision: string,
  ) => Promise<QuicklensResult<PlanBasis>>;
  readonly validateAdmissionRetry: (
    basis: PlanBasis,
    expectedRevision: string,
  ) => Promise<QuicklensResult<PlanBasis>>;
}
export function createLivePlanWorkflow(options: LivePlanWorkflowOptions): LivePlanWorkflow {
  return new Workflow(options);
}
class Workflow implements LivePlanWorkflow {
  readonly available = true;
  readonly unavailableReason = null;
  readonly #options: LivePlanWorkflowOptions;
  constructor(options: LivePlanWorkflowOptions) {
    this.#options = options;
  }
  async submitProposal(actor: PublicConnection, intent: MessageEnvelope, input: unknown) {
    const proposal = AgentPlanProposalSchema.safeParse(input);
    const payload = IntentPayloadSchema.safeParse(intent.payload);
    if (!proposal.success || !payload.success || !this.authorized(actor, intent, proposal.data)) {
      return qerror("forbidden", "Proposal is not bound to this exact scoped plan intent");
    }
    if (!sameBasis(payload.data.basis, proposal.data.intentBasis)) {
      return qerror("stale_basis", "Proposal basis differs from its immutable plan intent");
    }
    const digest = proposalDigest(proposal.data);
    const existing = this.existing(proposal.data);
    if (!existing.ok) return existing;
    if (existing.value !== null) {
      return existing.value.proposalDigest === digest
        ? recordResult(existing.value)
        : qerror("forbidden", "Plan identities are already bound to a different proposal");
    }
    const basis = await this.#options.validateBasis(proposal.data.basis);
    if (!basis.ok) return basis;
    const prepared =
      proposal.data.preparedStep === null
        ? await this.#options.reader.prepareComparison(proposal.data.admission.comparison)
        : { ok: true as const, value: proposal.data.preparedStep.comparisonView };
    if (!prepared.ok)
      return qerror("invalid_data", `ZAP comparison preparation failed: ${prepared.error.kind}`);
    if (!preparedMatches(prepared.value, proposal.data)) {
      return qerror("stale_basis", "Prepared comparison does not match the proposal basis");
    }
    const changes = deriveVerifiedPlanChanges(
      prepared.value,
      proposal.data.admission.alternative_id,
    );
    if (!changes.ok) return changes;
    const record: WorkflowRecord = {
      proposal: proposal.data,
      proposalDigest: digest,
      actorId: actor.actor.actorId,
      workspaceId: actor.actor.workspaceId,
      conversationId: actor.actor.conversationId,
      phase: "prepared",
      productDigest: null,
      admissionView: null,
      ownerSubmission: null,
      ownerChoice: null,
      continuation: null,
      verifiedChanges: changes.value,
      effectIndex: proposal.data.preparedStep?.effectIndex ?? 0,
      completedPrefix: [],
      currentStep: proposal.data.preparedStep,
      message: "Agent proposal validated and prepared against the exact ZAP basis.",
    };
    const stored = this.#options.store.create(record);
    return stored.ok ? recordResult(stored.value) : qerror("forbidden", stored.error.message);
  }
  async preview(input: PlanPreviewInput) {
    const loaded = this.#options.store.getByIntent(input.intentRef);
    if (!loaded.ok) return qerror("unavailable", loaded.error.message);
    if (loaded.value === null) {
      const live = await this.#options.validateBasis(input.basis);
      return live.ok
        ? operation(
            input.intentRef,
            "requested",
            "Waiting for a validated agent proposal.",
            input.basis,
          )
        : live;
    }
    return this.validatedRecord(input.basis, loaded.value);
  }
  async apply(input: PlanApplyInput) {
    const loaded = this.#options.store.getByOperation(input.operationRef);
    if (!loaded.ok) return qerror("unavailable", loaded.error.message);
    const record = loaded.value;
    if (record === null || record.proposal.previewRef !== input.previewRef) {
      return qerror("stale_basis", "Prepared operation or preview identity is unavailable");
    }
    const checked = await this.validatedRecord(input.basis, record);
    if (!checked.ok) return checked;
    return record.phase === "prepared" || record.phase === "step_ready"
      ? this.advance(record)
      : recordResult(record);
  }
  async reconcile(input: PlanReconcileInput) {
    const loaded = this.#options.store.getByOperation(input.operationRef);
    if (!loaded.ok) return qerror("unavailable", loaded.error.message);
    const record = loaded.value;
    if (record === null) return qerror("unavailable", "Plan operation is unavailable");
    const checked = this.recordIdentity(input.basis, record);
    if (!checked.ok) return checked;
    if (record.phase === "admission_uncertain") return this.advance(record);
    if (record.phase === "owner_decision_uncertain" && this.#options.owner !== undefined) {
      const reconciled = await this.#options.owner.reconcile(record);
      if (!reconciled.ok) return qerror("unavailable", reconciled.error.message);
      return reconciled.value.phase === "admission_uncertain"
        ? this.advance(reconciled.value)
        : recordResult(reconciled.value);
    }
    if (record.phase === "product_uncertain" && record.productDigest !== null) {
      const status = await this.#options.reader.reconcile({
        command_id: effectiveRequest(record).product.frame.header.command_id,
        command_digest: record.productDigest,
      });
      if (!status.ok) return recordResult(record);
      if (status.value.status !== "not_committed") return this.finishProduct(record, status.value);
      const request = effectiveRequest(record);
      const fresh = await this.#options.validateExecution(
        record.currentStep?.executionBasis ?? record.proposal.basis,
        request.product.frame.header.expected_revision.toString(),
      );
      return fresh.ok ? this.submitProduct(record) : fresh;
    }
    return recordResult(record);
  }
  async decide(input: PlanDecisionInput) {
    const loaded = this.#options.store.getByOperation(input.operationRef);
    if (!loaded.ok) return qerror("unavailable", loaded.error.message);
    const record = loaded.value;
    const expectedBasis = record?.currentStep?.executionBasis ?? record?.proposal.basis;
    if (record === null || expectedBasis === undefined || !sameBasis(input.basis, expectedBasis)) {
      return qerror("stale_basis", "Held operation or basis is unavailable");
    }
    if (
      record.phase !== "owner_decision_required" ||
      record.admissionView?.status !== "owner_decision_required" ||
      input.holdRef !== `hold:${record.admissionView.hold_id}`
    ) {
      return qerror("forbidden", "Decision does not match the exact held operation");
    }
    const live = await this.#options.validateExecution(
      expectedBasis,
      record.admissionView.observed_revision,
    );
    if (!live.ok) return live;
    if (this.#options.owner === undefined) {
      return qerror("unsupported", "Authenticated human Owner adapter is not configured");
    }
    const decided = await this.#options.owner.decide(record, input);
    if (!decided.ok) return qerror("unavailable", decided.error.message);
    return decided.value.phase === "admission_uncertain"
      ? this.advance(decided.value)
      : recordResult(decided.value);
  }
  decision(): QuicklensResult<HeldDecisionView | null> {
    const current = this.#options.store.currentDecision();
    if (!current.ok) return qerror("unavailable", current.error.message);
    const record = current.value;
    const view = record?.admissionView;
    if (record !== null && view?.status === "owner_decision_required") {
      return {
        ok: true,
        value: {
          operationRef: record.proposal.operationRef,
          holdRef: QuicklensRefSchema.parse(`hold:${view.hold_id}`),
        },
      };
    }
    return { ok: true, value: null };
  }
  private authorized(
    connection: PublicConnection,
    intent: MessageEnvelope,
    proposal: AgentPlanProposal,
  ): boolean {
    const actor = connection.actor;
    const payload = IntentPayloadSchema.safeParse(intent.payload);
    return (
      actor.state === "active" &&
      actor.parentActorId === null &&
      actor.capabilities.includes("plan:propose") &&
      actor.workspaceId === this.#options.workspaceId &&
      actor.conversationId === this.#options.conversationId &&
      connection.handle.actorId === actor.actorId &&
      connection.handle.workspaceId === actor.workspaceId &&
      connection.handle.conversationId === actor.conversationId &&
      intent.messageId === proposal.intentMessageId &&
      intent.workspaceId === actor.workspaceId &&
      intent.conversationId === actor.conversationId &&
      intent.toActorId === actor.actorId &&
      intent.kind === "notice.created" &&
      payload.success &&
      payload.data.intentRef === proposal.intentRef
    );
  }
  private existing(proposal: AgentPlanProposal): QuicklensResult<WorkflowRecord | null> {
    const operation = this.#options.store.getByOperation(proposal.operationRef);
    if (!operation.ok) return qerror("unavailable", operation.error.message);
    const intent = this.#options.store.getByIntent(proposal.intentRef);
    if (!intent.ok) return qerror("unavailable", intent.error.message);
    if (
      operation.value !== null &&
      intent.value !== null &&
      operation.value.proposalDigest !== intent.value.proposalDigest
    ) {
      return qerror("forbidden", "Plan intent and operation identities are bound inconsistently");
    }
    return { ok: true, value: operation.value ?? intent.value };
  }
  private async validatedRecord(basis: PlanBasis, record: WorkflowRecord) {
    const identity = this.recordIdentity(basis, record);
    if (!identity.ok) return identity;
    const expectedBasis = record.currentStep?.executionBasis ?? record.proposal.basis;
    const live = await this.#options.validateBasis(expectedBasis);
    return live.ok ? recordResult(record) : live;
  }
  private recordIdentity(basis: PlanBasis, record: WorkflowRecord) {
    if (
      record.workspaceId !== this.#options.workspaceId ||
      record.conversationId !== this.#options.conversationId
    ) {
      return qerror("forbidden", "Saved operation is outside this configured lens scope");
    }
    const expectedBasis = record.currentStep?.executionBasis ?? record.proposal.basis;
    if (!sameBasis(basis, expectedBasis)) {
      return qerror("stale_basis", "Request basis differs from the saved proposal basis");
    }
    return { ok: true as const, value: null };
  }
  private async advance(record: WorkflowRecord): Promise<QuicklensResult<PlanOperationResult>> {
    let pending = record;
    if (record.phase === "prepared" || record.phase === "step_ready") {
      const changed = this.#options.store.transition(
        record.proposal.operationRef,
        record.proposalDigest,
        [record.phase],
        {
          ...record,
          phase: "admission_uncertain",
          message: "Admission request is durably pending exact reconciliation.",
        },
      );
      if (!changed.ok) return qerror("unavailable", changed.error.message);
      pending = changed.value;
    }
    const request = effectiveRequest(pending);
    const safe = await this.#options.validateAdmissionRetry(
      pending.currentStep?.executionBasis ?? pending.proposal.basis,
      request.expected_revision.toString(),
    );
    if (!safe.ok) return safe;
    const advanced = await this.#options.coordinator.advanceChangeAdmission(request);
    if (!advanced.ok) {
      if (advanced.error.kind === "uncertain_operation") return recordResult(pending);
      return this.transitionResult(
        pending,
        "rejected",
        `Admission refused: ${advanced.error.kind}.`,
      );
    }
    if (advanced.value.status === "owner_decision_required") {
      return this.transitionResult(
        pending,
        "owner_decision_required",
        `Owner decision required for hold ${advanced.value.hold_id}.`,
        {
          admissionView: {
            ...advanced.value,
            decision: {
              ...advanced.value.decision,
              effect_fingerprints: [...advanced.value.decision.effect_fingerprints],
              effect_preflight_digests: [...advanced.value.decision.effect_preflight_digests],
            },
          },
        },
      );
    }
    return this.prepareProduct(pending);
  }

  private async prepareProduct(record: WorkflowRecord) {
    const command = effectiveRequest(record).product;
    const fresh = await this.#options.validateExecution(
      record.currentStep?.executionBasis ?? record.proposal.basis,
      command.frame.header.expected_revision.toString(),
    );
    if (!fresh.ok) return fresh;
    const productDigest = ZapDigestSchema.parse(protectedCommandDigest(command));
    if (record.currentStep !== null && productDigest !== record.currentStep.productDigest) {
      return qerror("invalid_data", "Prepared effect product digest changed");
    }
    const changed = this.#options.store.transition(
      record.proposal.operationRef,
      record.proposalDigest,
      ["admission_uncertain"],
      {
        ...record,
        phase: "product_uncertain",
        productDigest,
        message: "Admitted product is durably pending exact reconciliation.",
      },
    );
    return changed.ok
      ? this.submitProduct(changed.value)
      : qerror("unavailable", changed.error.message);
  }

  private async submitProduct(record: WorkflowRecord) {
    if (record.productDigest === null)
      return qerror("invalid_data", "Product digest is unavailable");
    const command = effectiveRequest(record).product;
    const submission: ProtectedSubmission = {
      command,
      reconciliation: {
        command_id: command.frame.header.command_id,
        command_digest: record.productDigest,
      },
    };
    const submitted = await this.#options.coordinator.submit("command", submission);
    if (submitted.ok) return this.finishProduct(record, submitted.value);
    return submitted.error.kind === "http_refusal"
      ? this.transitionResult(
          record,
          "rejected",
          `Plan product refused: ${submitted.error.refusal.code}.`,
        )
      : recordResult(record);
  }

  private async finishProduct(record: WorkflowRecord, status: SubmissionStatus) {
    if (status.status === "unknown") return recordResult(record);
    if (status.status !== "committed") {
      return this.transitionResult(record, "rejected", "Plan product is not committed.");
    }
    const prepared = record.proposal.preparedSuccessor;
    if (prepared === null) {
      return this.transitionResult(record, "completed", "Plan product is committed.");
    }
    const effect = prepared.effects[record.effectIndex];
    if (effect === undefined)
      return qerror("invalid_data", "Committed effect index is unavailable");
    const completedPrefix = [...record.completedPrefix, effect.effectId];
    const nextIndex = record.effectIndex + 1;
    if (nextIndex === prepared.effects.length) {
      return this.transitionResult(record, "completed", "All ordered plan effects are committed.", {
        completedPrefix,
        effectIndex: nextIndex,
      });
    }
    if (this.#options.authoring === undefined) {
      return qerror("unsupported", "Ordered effect authoring is unavailable");
    }
    const next = await this.#options.authoring.prepareEffect(prepared, {
      effectIndex: nextIndex,
      completedPrefix,
      decisionId: null,
    });
    if (!next.ok) return qerror("unavailable", next.error.message);
    return this.transitionResult(
      record,
      "step_ready",
      "Prior effect committed; next effect is prepared.",
      {
        completedPrefix,
        effectIndex: nextIndex,
        currentStep: next.value,
        productDigest: null,
        continuation: null,
      },
    );
  }

  private transitionResult(
    record: WorkflowRecord,
    phase: WorkflowPhase,
    message: string,
    patch: Partial<WorkflowRecord> = {},
  ) {
    const changed = this.#options.store.transition(
      record.proposal.operationRef,
      record.proposalDigest,
      [record.phase],
      { ...record, ...patch, phase, message },
    );
    return changed.ok ? recordResult(changed.value) : qerror("unavailable", changed.error.message);
  }
}

function effectiveRequest(record: WorkflowRecord) {
  return record.continuation ?? record.currentStep?.advanceRequest ?? record.proposal.admission;
}

function preparedMatches(prepared: PreparedEffectComparisonView, proposal: AgentPlanProposal) {
  const selected = proposal.admission.comparison.draft.alternatives.find(
    (alternative) => alternative.alternative_id === proposal.admission.alternative_id,
  );
  const effect = selected?.effects[0];
  const product = proposal.admission.product.frame;
  return (
    prepared.store.store_id === proposal.admission.store.store_id &&
    prepared.store.campaign_id === proposal.admission.store.campaign_id &&
    prepared.store.base_id === proposal.admission.store.base_id &&
    String(prepared.observed_revision) === String(proposal.basis.revision) &&
    prepared.relevant_basis === proposal.admission.relevant_basis &&
    (selected?.effects.length ?? 0) > 0 &&
    effect?.kind === product.header.kind &&
    effect.product_event_id === product.header.event_id &&
    effect.payload.canonical_json.length === product.payload.canonical_json.length &&
    effect.payload.canonical_json.every(
      (value, index) => value === product.payload.canonical_json[index],
    ) &&
    verifiedPreparedProductMatches(
      prepared,
      proposal.admission.alternative_id,
      proposal.admission.product,
    )
  );
}

function proposalDigest(proposal: AgentPlanProposal) {
  return ZapDigestSchema.parse(
    createHash("sha256").update(encodeCanonicalJson(proposal)).digest("hex"),
  );
}
