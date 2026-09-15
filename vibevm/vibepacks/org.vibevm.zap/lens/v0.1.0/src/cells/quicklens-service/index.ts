/**
 * Trusted composition of ZAP, lens broker, source guard and browser-safe model.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shared-client
 */
import { randomUUID } from "node:crypto";
import {
  ExactDecimalSchema,
  PlanBasisSchema,
  PlanOperationResultSchema,
  QuicklensRefSchema,
  QuicklensSnapshotSchema,
  type AnswerQuestionInput,
  type InvalidationReason,
  type PlanApplyInput,
  type PlanDecisionInput,
  type PlanBasis,
  type PlanIntentInput,
  type PlanOperationResult,
  type PlanPreviewInput,
  type PlanReconcileInput,
  type QuicklensDataSource,
  type QuicklensError,
  type QuicklensResult,
  type QuicklensSnapshot,
} from "../quicklens-model/index.ts";
import {
  ConversationIdSchema,
  DecimalSchema,
  QuestionIdSchema,
  WorkspaceIdSchema,
  type ClientRequestIdSchema,
} from "../protocol/index.ts";
import type { PrincipalTransportPort } from "../transport/index.ts";
import {
  type ActiveContextView,
  type ZapClient,
  type ZapClientError,
  type ZapQueryPage,
} from "../zap-client/index.ts";
import type {
  SpecificationBasis,
  SpecificationChangeSignal,
  SpecificationWatch,
} from "../specification-watch/index.ts";
import { mapQuestion } from "./mapping.ts";
import { actorRef, agentTarget, planStatus, principalNotice, requestId } from "./service-model.ts";
import { readMap } from "./read-map.ts";
import { InvalidationMonitor } from "./invalidation.ts";
import type { PlanWorkflowPort } from "./types.ts";

export interface QuicklensServiceOptions {
  readonly zap: ZapClient;
  readonly broker: PrincipalTransportPort;
  readonly workflow: PlanWorkflowPort;
  readonly specifications: SpecificationWatch;
  readonly workspaceId: string;
  readonly conversationId: string;
  readonly sourceLabel: string;
  readonly clock?: () => Date;
  readonly pageLimit?: number;
  readonly maximumPages?: number;
  readonly invalidationPollMilliseconds?: number;
}

export interface LiveQuicklensDataSource extends QuicklensDataSource {
  proposePlanIntentWithIdentity(
    input: PlanIntentInput,
    identity: {
      readonly intentRef: ReturnType<typeof QuicklensRefSchema.parse>;
      readonly clientRequestId: ReturnType<typeof ClientRequestIdSchema.parse>;
    },
  ): Promise<QuicklensResult<PlanOperationResult>>;
  close(): void;
}

export function createQuicklensService(
  options: QuicklensServiceOptions,
): QuicklensResult<LiveQuicklensDataSource> {
  const workspace = WorkspaceIdSchema.safeParse(options.workspaceId);
  const conversation = ConversationIdSchema.safeParse(options.conversationId);
  const pageLimit = options.pageLimit ?? 100;
  const maximumPages = options.maximumPages ?? 5;
  const invalidationPollMilliseconds = options.invalidationPollMilliseconds ?? 1_000;
  if (
    !workspace.success ||
    !conversation.success ||
    options.sourceLabel.length === 0 ||
    options.sourceLabel.length > 512 ||
    !Number.isInteger(pageLimit) ||
    pageLimit < 1 ||
    pageLimit > 100 ||
    !Number.isInteger(maximumPages) ||
    maximumPages < 1 ||
    maximumPages > 20 ||
    invalidationPollMilliseconds < 25 ||
    invalidationPollMilliseconds > 60_000
  ) {
    return error("invalid_data", "Quicklens service configuration is invalid");
  }
  return {
    ok: true,
    value: new Service({
      ...options,
      workspaceId: workspace.data,
      conversationId: conversation.data,
      pageLimit,
      maximumPages,
      invalidationPollMilliseconds,
    }),
  };
}

interface CheckedOptions extends QuicklensServiceOptions {
  readonly workspaceId: ReturnType<typeof WorkspaceIdSchema.parse>;
  readonly conversationId: ReturnType<typeof ConversationIdSchema.parse>;
  readonly pageLimit: number;
  readonly maximumPages: number;
  readonly invalidationPollMilliseconds: number;
}

class Service implements QuicklensDataSource {
  readonly #options: CheckedOptions;
  readonly #listeners = new Set<(reason: InvalidationReason) => void>();
  readonly #monitor: InvalidationMonitor;
  readonly #unsubscribeSpecifications: () => void;
  #latestBasis: PlanBasis | undefined;

  constructor(options: CheckedOptions) {
    this.#options = options;
    this.#monitor = new InvalidationMonitor({
      broker: options.broker,
      zap: options.zap,
      workspaceId: options.workspaceId,
      conversationId: options.conversationId,
      intervalMilliseconds: options.invalidationPollMilliseconds,
    });
    this.#unsubscribeSpecifications = options.specifications.subscribe((signal) => {
      for (const listener of this.#listeners) listener("plan");
      void this.publishSpecificationSignal(signal);
    });
  }

  subscribe(listener: (reason: InvalidationReason) => void): () => void {
    this.#listeners.add(listener);
    this.#monitor.start((reason) => {
      for (const target of this.#listeners) target(reason);
    });
    return () => {
      this.#listeners.delete(listener);
      if (this.#listeners.size === 0) this.#monitor.stop();
    };
  }

  close(): void {
    this.#monitor.stop();
    this.#unsubscribeSpecifications();
    this.#listeners.clear();
  }

  async read(input: { readonly signal: AbortSignal }): Promise<QuicklensResult<QuicklensSnapshot>> {
    if (input.signal.aborted) return error("cancelled", "Quicklens read was cancelled");
    const source = await this.#options.specifications.capture();
    if (!source.ok) return error("unavailable", source.error.message);
    const [capabilities, active, actors, questions] = await Promise.all([
      this.#options.zap.capabilities(input.signal),
      this.#options.zap.activeContext(input.signal),
      this.#options.broker.listActors(this.scope()),
      this.#options.broker.listQuestions(this.scope()),
    ]);
    if (!capabilities.ok) return zapError(capabilities.error);
    if (!active.ok) return zapError(active.error);
    if (!actors.ok) return brokerError(actors.error.message);
    if (!questions.ok) return brokerError(questions.error.message);
    const context = active.value.items[0];
    if (context === undefined) return error("invalid_data", "active context item is missing");
    const basis = planBasis(active.value, context, source.value);
    this.#latestBasis = basis;
    const map = await readMap({
      zap: this.#options.zap,
      capabilities: capabilities.value,
      activePage: active.value,
      active: context,
      signal: input.signal,
      pageLimit: this.#options.pageLimit,
      maximumPages: this.#options.maximumPages,
    });
    if (!map.ok) return map;
    const plan = planStatus(
      context,
      map.value.milestone,
      basis,
      actors.value,
      this.#options.workflow,
      this.#options.workflow.decision(),
    );
    const partial =
      map.value.partial ||
      actors.value.hasMore ||
      questions.value.hasMore ||
      context.current_strategy.state === "absent";
    return {
      ok: true,
      value: QuicklensSnapshotSchema.parse({
        sourceMode: "live",
        sourceLabel: this.#options.sourceLabel,
        phase: plan?.state === "reassessment" ? "stale" : partial ? "partial" : "ready",
        phaseDetail: partial
          ? "One or more bounded sources have additional or unavailable data."
          : null,
        capturedAt: (this.#options.clock ?? (() => new Date()))().toISOString(),
        revision: basis.revision,
        objects: map.value.objects,
        relationships: map.value.relationships,
        navigation: map.value.navigation,
        questions: questions.value.questions.map(mapQuestion),
        agentTargets: actors.value.actors.map(agentTarget),
        plan,
      }),
    };
  }

  async answerQuestion(input: AnswerQuestionInput) {
    const questionId = parseRef(input.questionRef, "question", QuestionIdSchema);
    if (!questionId.ok) return questionId;
    const answered = await this.#options.broker.answer({
      clientRequestId: requestId("answer"),
      workspaceId: this.#options.workspaceId,
      conversationId: this.#options.conversationId,
      questionId: questionId.value,
      expectedRevision: DecimalSchema.parse(input.expectedRevision),
      answer: input.answer,
    });
    if (!answered.ok) return brokerError(answered.error.message);
    const listed = await this.#options.broker.listQuestions(this.scope());
    if (!listed.ok) return brokerError(listed.error.message);
    const row = listed.value.questions.find(
      (candidate) => candidate.question.questionId === answered.value.questionId,
    );
    return row === undefined
      ? error("invalid_data", "answered question is absent from its scoped view")
      : { ok: true as const, value: mapQuestion(row) };
  }

  async proposePlanIntent(input: PlanIntentInput) {
    return this.proposePlanIntentWithIdentity(input, {
      intentRef: QuicklensRefSchema.parse(`intent:${randomUUID().replaceAll("-", "")}`),
      clientRequestId: requestId("plan-intent"),
    });
  }

  async proposePlanIntentWithIdentity(
    input: PlanIntentInput,
    identity: {
      readonly intentRef: ReturnType<typeof QuicklensRefSchema.parse>;
      readonly clientRequestId: ReturnType<typeof ClientRequestIdSchema.parse>;
    },
  ) {
    const textBytes = new TextEncoder().encode(input.text).length;
    if (textBytes === 0 || textBytes > 16_384) {
      return error("invalid_data", "plan intent text must be 1..=16384 UTF-8 bytes");
    }
    const fresh = await this.guardBasis(input.basis);
    if (!fresh.ok) return fresh;
    const actors = await this.#options.broker.listActors(this.scope());
    if (!actors.ok) return brokerError(actors.error.message);
    const eligible = actors.value.actors.filter((row) => row.eligiblePlanTarget);
    const selected = eligible.find((row) => actorRef(row.actor.actorId) === input.targetActorRef);
    if (selected === undefined) {
      return error(
        "unavailable",
        eligible.length === 0
          ? "No active root agent is eligible for plan intent."
          : "Selected actor is not an eligible root planning agent.",
      );
    }
    const emitted = await this.#options.broker.emit(
      principalNotice(
        selected.actor.actorId,
        input.text,
        input.basis,
        identity.intentRef,
        this.#options.workspaceId,
        this.#options.conversationId,
        identity.clientRequestId,
      ),
    );
    if (!emitted.ok) return brokerError(emitted.error.message);
    return {
      ok: true as const,
      value: PlanOperationResultSchema.parse({
        operationRef: identity.intentRef,
        previewRef: null,
        preview: null,
        state: "queued",
        message: `Plan intent queued for ${selected.label}.`,
        nextBasis: input.basis,
      }),
    };
  }

  async previewPlan(input: PlanPreviewInput) {
    const fresh = await this.guardBasis(input.basis);
    return fresh.ok ? this.#options.workflow.preview(input) : fresh;
  }

  async applyPlan(input: PlanApplyInput) {
    const fresh = await this.guardBasis(input.basis);
    return fresh.ok ? this.#options.workflow.apply(input) : fresh;
  }

  reconcilePlan(input: PlanReconcileInput) {
    return this.#options.workflow.reconcile(input);
  }

  decidePlan(input: PlanDecisionInput) {
    return this.#options.workflow.decide(input);
  }

  private scope() {
    return {
      workspaceId: this.#options.workspaceId,
      conversationId: this.#options.conversationId,
      limit: this.#options.pageLimit,
    };
  }

  private async guardBasis(basis: PlanBasis): Promise<QuicklensResult<PlanBasis>> {
    const parsed = PlanBasisSchema.safeParse(basis);
    if (
      !parsed.success ||
      this.#latestBasis === undefined ||
      !sameBasis(parsed.data, this.#latestBasis)
    ) {
      return error("stale_basis", "plan basis is not the latest Quicklens snapshot");
    }
    const source = await this.#options.specifications.verify(sourceDigest(parsed.data));
    if (!source.ok || source.value.digest !== sourceDigest(parsed.data)) {
      return error("stale_basis", "project specifications changed since this plan basis");
    }
    const active = await this.#options.zap.activeContext();
    if (!active.ok) return zapError(active.error);
    const item = active.value.items[0];
    return item !== undefined && sameBasis(planBasis(active.value, item, source.value), parsed.data)
      ? { ok: true, value: parsed.data }
      : error("stale_basis", "ZAP active context changed since this plan basis");
  }

  private async publishSpecificationSignal(signal: SpecificationChangeSignal): Promise<void> {
    const actors = await this.#options.broker.listActors(this.scope());
    if (!actors.ok) return;
    for (const target of actors.value.actors.filter((row) => row.eligiblePlanTarget)) {
      await this.#options.broker.emit({
        clientRequestId: requestId("spec-change"),
        workspaceId: this.#options.workspaceId,
        conversationId: this.#options.conversationId,
        toActorId: target.actor.actorId,
        payload: {
          type: "specification_reassessment",
          previousSourceBasis: signal.previous.digest,
          currentSourceBasis: signal.current.digest,
        },
      });
    }
  }
}

function planBasis(
  page: ZapQueryPage<ActiveContextView>,
  context: ActiveContextView,
  source: SpecificationBasis,
): PlanBasis {
  return PlanBasisSchema.parse({
    storeRef: QuicklensRefSchema.parse(`store:${context.snapshot.store_id}`),
    baseRef: QuicklensRefSchema.parse(`base:${context.snapshot.base_id}`),
    revision: ExactDecimalSchema.parse(page.revision),
    sourceBasisRef: QuicklensRefSchema.parse(`source-basis:${source.digest}`),
  });
}

function parseRef<T>(
  ref: string,
  prefix: string,
  schema: { safeParse(value: string): { success: true; data: T } | { success: false } },
): QuicklensResult<T> {
  const value = ref.startsWith(`${prefix}:`) ? ref.slice(prefix.length + 1) : "";
  const parsed = schema.safeParse(value);
  return parsed.success
    ? { ok: true, value: parsed.data }
    : error("invalid_data", `${prefix} reference is invalid`);
}

function sourceDigest(basis: PlanBasis): string {
  return basis.sourceBasisRef.startsWith("source-basis:") ? basis.sourceBasisRef.slice(13) : "";
}

function sameBasis(left: PlanBasis, right: PlanBasis): boolean {
  return (
    left.storeRef === right.storeRef &&
    left.baseRef === right.baseRef &&
    left.revision === right.revision &&
    left.sourceBasisRef === right.sourceBasisRef
  );
}

function zapError(value: ZapClientError): QuicklensResult<never> {
  if (value.kind === "http_refusal" && value.refusal.code === "unavailable") {
    return error("unavailable", value.refusal.message);
  }
  return error(
    value.kind === "stale_context" ? "stale_basis" : "invalid_data",
    `ZAP ${value.kind}`,
  );
}

function brokerError(message: string): QuicklensResult<never> {
  return error(message.includes("forbidden") ? "forbidden" : "unavailable", message);
}

function error(code: QuicklensError["code"], message: string): QuicklensResult<never> {
  return {
    ok: false,
    error: {
      code,
      message: message.slice(0, 2_000),
      recovery: "Refresh the configured Quicklens source and retry against its exact basis.",
    },
  };
}

export type { PlanWorkflowPort } from "./types.ts";
export {
  createQuicklensGateway,
  type QuicklensGateway,
  type QuicklensGatewayOptions,
  type WorkspaceSessionIdentity,
  type WorkspaceSourceFactory,
} from "./gateway.ts";
export {
  AgentPlanProposalSchema,
  createLivePlanWorkflow,
  type AgentPlanProposal,
  type LivePlanWorkflow,
  type LivePlanWorkflowOptions,
  type PlanWorkflowStore,
} from "./workflow.ts";
export { createAgentPlanProposalPort } from "./workflow-agent.ts";
export { openSqlitePlanWorkflowStore } from "./workflow-store.ts";
export {
  AgentPlanRuntimeConfigSchema,
  QuicklensRuntimeConfigSchema,
  QuicklensSourceRuntimeConfigSchema,
  openQuicklensSourceRuntime,
  openAgentPlanRuntime,
  startQuicklensRuntime,
  type AgentPlanRuntime,
  type AgentPlanRuntimeConfig,
  type QuicklensRuntime,
  type QuicklensRuntimeConfig,
  type QuicklensSourceRuntime,
  type QuicklensSourceRuntimeConfig,
} from "./runtime.ts";
