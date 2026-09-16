/** @verifies spec://org.vibevm.zap/lens/PROP-002#shared-client */
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test, { type TestContext } from "node:test";
import type { ZodType } from "zod";
import { openBroker } from "../broker/index.ts";
import {
  QuicklensRefSchema,
  type PlanBasis,
  type PlanOperationResult,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  WorkspaceIdSchema,
} from "../protocol/index.ts";
import { createSpecificationWatch } from "../specification-watch/index.ts";
import { createLocalPrincipalTransport } from "../transport/index.ts";
import {
  U64WireSchema,
  ZapIdSchema,
  encodeCanonicalJson,
  parseCanonicalJson,
  parseWireJson,
  type ActiveContextView,
  type CanonicalJsonInput,
  type ZapClient,
  type ZapClientResult,
  type ZapId,
  type ZapQueryPage,
} from "../zap-client/index.ts";
import { createQuicklensService } from "./index.ts";
import type { PlanWorkflowPort } from "./types.ts";

const workspaceId = WorkspaceIdSchema.parse("workspace.quicklens");
const conversationId = ConversationIdSchema.parse("conversation.quicklens");
const revision = u64("7");
const store = {
  store_id: ZapIdSchema.parse("store.quicklens"),
  campaign_id: ZapIdSchema.parse("campaign.quicklens"),
  base_id: ZapIdSchema.parse("base.quicklens"),
  store_epoch: "zap/2",
  codec_epoch: 2,
  reducer_epoch: 1,
};

test("live read maps real broker questions, agent targets and mock Rust query fixtures", async (context) => {
  const fixture = await setup(context);
  const read = await fixture.source.read({ signal: new AbortController().signal });
  assert.equal(
    read.ok,
    true,
    read.ok ? "" : JSON.stringify({ error: read.error, queries: fixture.zap.queries }),
  );
  if (!read.ok) return;
  assert.equal(read.value.sourceMode, "live");
  assert.equal(read.value.revision, "7");
  assert.equal(read.value.objects[0]?.semanticType, "work");
  assert.ok(read.value.objects.some((object) => object.category === "resource"));
  assert.ok(read.value.objects.some((object) => object.semanticType === "outcome"));
  assert.equal(read.value.navigation?.activeOutcomeRef, "outcome:outcome.quicklens");
  assert.equal(read.value.navigation?.dependencyDirection, "prerequisite_to_dependent");
  assert.deepEqual(read.value.navigation?.currentWorkRefs, []);
  assert.equal(read.value.relationships.length, 1);
  assert.equal(read.value.questions[0]?.state, "pending");
  assert.equal(read.value.agentTargets.filter((target) => target.eligible).length, 1);
  assert.equal(read.value.plan?.state, "current");
  assert.deepEqual(fixture.zap.queries, [
    "zap.milestone.plan",
    "zap.map.overview.v1",
    "zap.map.object.v1",
    "zap.map.object.v1",
    "zap.map.object.v1",
  ]);
});

test("answer and explicit plan intent use scoped broker authority", async (context) => {
  const fixture = await setup(context);
  const read = await fixture.source.read({ signal: new AbortController().signal });
  assert.equal(read.ok, true);
  if (!read.ok) return;
  const question = read.value.questions[0];
  const target = read.value.agentTargets.find((candidate) => candidate.eligible);
  assert.ok(question && target && read.value.plan);
  const answered = await fixture.source.answerQuestion({
    questionRef: question.ref,
    expectedRevision: question.revision,
    answer: "Proceed",
  });
  assert.equal(answered.ok && answered.value.state, "answered");

  const proposed = await fixture.source.proposePlanIntent({
    text: "Reassess the release plan",
    basis: read.value.plan.basis,
    targetActorRef: target.ref,
  });
  assert.equal(proposed.ok && proposed.value.state, "queued");
  const inbox = fixture.broker.inbox(fixture.rootAuth, {});
  assert.equal(inbox.ok, true);
  if (inbox.ok) {
    const payload = inbox.value.deliveries.at(-1)?.message.payload;
    assert.equal(typeof payload, "object");
    if (typeof payload === "object" && payload !== null && !Array.isArray(payload)) {
      assert.equal(payload["type"], "plan_intent");
    }
  }

  const foreign = fixture.broker.listActors(
    { principalToken: fixture.humanToken },
    {
      workspaceId: WorkspaceIdSchema.parse("workspace.foreign"),
      conversationId,
      limit: 10,
    },
  );
  assert.equal(foreign.ok, false);
  if (!foreign.ok) assert.equal(foreign.error.code, "forbidden");
});

test("fresh source checks pass unchanged and reject missed specification drift", async (context) => {
  const fixture = await setup(context);
  const read = await fixture.source.read({ signal: new AbortController().signal });
  assert.equal(read.ok, true);
  if (!read.ok || read.value.plan === null) return;
  const input = {
    intentRef: QuicklensRefSchema.parse("intent.preview"),
    basis: read.value.plan.basis,
  };
  const unchanged = await fixture.source.previewPlan(input);
  assert.equal(unchanged.ok && unchanged.value.state, "prepared");
  assert.equal(fixture.workflow.previews, 1);

  fixture.specifications.close();
  await writeFile(fixture.specPath, "<spec>changed without watcher event</spec>");
  const stale = await fixture.source.previewPlan(input);
  assert.equal(stale.ok, false);
  if (!stale.ok) assert.equal(stale.error.code, "stale_basis");
  assert.equal(fixture.workflow.previews, 1);
});

test("debounced specification change publishes reassessment notice to eligible root", async (context) => {
  const fixture = await setup(context);
  const read = await fixture.source.read({ signal: new AbortController().signal });
  assert.equal(read.ok, true);
  await writeFile(fixture.specPath, "<spec>advisory change</spec>");
  const deadline = Date.now() + 2_000;
  let found = false;
  while (Date.now() < deadline && !found) {
    const inbox = fixture.broker.inbox(fixture.rootAuth, {});
    if (inbox.ok) {
      found = inbox.value.deliveries.some((delivery) => {
        const payload = delivery.message.payload;
        return (
          typeof payload === "object" &&
          payload !== null &&
          !Array.isArray(payload) &&
          payload["type"] === "specification_reassessment"
        );
      });
    }
    if (!found) await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.equal(found, true);
});

test("external broker question invalidates subscribers and unsubscribe stops polling", async (context) => {
  const fixture = await setup(context);
  let questions = 0;
  const unsubscribe = fixture.source.subscribe?.((reason) => {
    if (reason === "questions") questions += 1;
  });
  await new Promise((resolve) => setTimeout(resolve, 60));
  fixture.broker.ask(fixture.rootAuth, {
    clientRequestId: ClientRequestIdSchema.parse("request.quicklens.external-question"),
    prompt: "External question?",
    answerMode: "free_text",
  });
  const deadline = Date.now() + 1_000;
  while (Date.now() < deadline && questions === 0) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.equal(questions, 1);
  unsubscribe?.();
  fixture.broker.ask(fixture.rootAuth, {
    clientRequestId: ClientRequestIdSchema.parse("request.quicklens.after-unsubscribe"),
    prompt: "No refresh?",
    answerMode: "free_text",
  });
  await new Promise((resolve) => setTimeout(resolve, 75));
  assert.equal(questions, 1);
});

async function setup(context: TestContext) {
  const broker = take(openBroker({ databasePath: ":memory:" }));
  context.after(() => broker.close());
  const agent = broker.enrollPrincipal({
    kind: "agent",
    workspaceIds: [workspaceId],
    conversationIds: [conversationId],
    capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "plan:propose"],
  });
  const human = broker.enrollPrincipal({
    kind: "human_responder",
    workspaceIds: [workspaceId],
    conversationIds: [conversationId],
    capabilities: ["message:emit", "question:answer", "question:amend", "events:read"],
  });
  const agentValue = take(agent);
  const humanValue = take(human);
  const connection = broker.connect({
    principalToken: agentValue.principalToken,
    clientRequestId: ClientRequestIdSchema.parse("request.quicklens.root"),
    workspaceId,
    conversationId,
    capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "plan:propose"],
    host: { kind: "test", sessionId: "root", provenance: "attested" },
    replyPolicy: { kind: "retain" },
  });
  const connectionValue = take(connection);
  const rootAuth = {
    principalToken: agentValue.principalToken,
    bindingToken: connectionValue.credentials.bindingToken,
  };
  const asked = broker.ask(rootAuth, {
    clientRequestId: ClientRequestIdSchema.parse("request.quicklens.question"),
    prompt: "Ship it?",
    answerMode: "free_text",
  });
  assert.equal(asked.ok, true);

  const root = await mkdtemp(join(tmpdir(), "quicklens-service-"));
  const specPath = join(root, "plan.xml");
  await writeFile(specPath, "<spec>current</spec>");
  const watched = await createSpecificationWatch({
    roots: [{ root, include: ["**/*.xml"] }],
    debounceMilliseconds: 25,
  });
  const specifications = take(watched);
  context.after(() => specifications.close());

  const zap = new MockZap();
  const workflow = new MockWorkflow();
  const created = createQuicklensService({
    zap,
    broker: createLocalPrincipalTransport(broker, humanValue.principalToken),
    workflow,
    specifications,
    workspaceId,
    conversationId,
    sourceLabel: "Configured ZAP fixture",
    clock: () => new Date("2026-09-15T00:00:00.000Z"),
    invalidationPollMilliseconds: 25,
  });
  const source = take(created);
  context.after(() => source.close());
  return {
    source,
    broker,
    rootAuth,
    humanToken: humanValue.principalToken,
    specifications,
    specPath,
    zap,
    workflow,
  };
}

class MockWorkflow implements PlanWorkflowPort {
  readonly available = true;
  readonly unavailableReason = null;
  previews = 0;

  async preview(input: { readonly intentRef: string; readonly basis: PlanBasis }) {
    this.previews += 1;
    return operation("prepared", input.basis);
  }
  async apply(input: { readonly basis: PlanBasis }) {
    return operation("completed", input.basis);
  }
  async reconcile(input: { readonly basis: PlanBasis }) {
    return operation("completed", input.basis);
  }
  async decide(input: { readonly basis: PlanBasis }) {
    return operation("held", input.basis);
  }
  decision() {
    return { ok: true as const, value: null };
  }
}

class MockZap implements ZapClient {
  readonly queries: string[] = [];

  async capabilities() {
    return ok({
      schema: "zap-machine-capabilities/1",
      read_operations: ["query"],
      command_operations: [],
      query_ids: [
        ZapIdSchema.parse("zap.planning.active-context.v1"),
        ZapIdSchema.parse("zap.map.overview.v1"),
        ZapIdSchema.parse("zap.map.object.v1"),
        ZapIdSchema.parse("zap.milestone.plan"),
        ZapIdSchema.parse("zap.milestone.revision"),
      ],
      unavailable_operations: [],
      max_page_items: 100,
      local_bind_default: true,
    });
  }
  async activeContext(): Promise<ZapClientResult<ZapQueryPage<ActiveContextView>>> {
    return ok(page(activeContext()));
  }
  async query<T>(
    queryId: ZapId,
    input: CanonicalJsonInput,
    schema: ZodType<T>,
  ): Promise<ZapClientResult<ZapQueryPage<T>>> {
    this.queries.push(queryId);
    const raw =
      queryId === "zap.map.overview.v1"
        ? overview(false)
        : queryId === "zap.map.object.v1"
          ? {
              card: JSON.stringify(input).includes('"kind":"resource"')
                ? resourceCard()
                : JSON.stringify(input).includes('"kind":"outcome"')
                  ? outcomeCard()
                  : overview(true).cards[0],
              examined_relationships: 1,
              emitted_relationships: 1,
              examined_index_rows: 1n,
              next: null,
              through_revision: 7n,
            }
          : milestone();
    const parsed = schema.safeParse(parseCanonicalJson(encodeCanonicalJson(raw)));
    return parsed.success
      ? ok(page(parsed.data))
      : unavailable<ZapQueryPage<T>>(`fixture query shape: ${parsed.error.message}`);
  }
  snapshot(): ReturnType<ZapClient["snapshot"]> {
    return Promise.resolve(unavailable("unused snapshot"));
  }
  events(): ReturnType<ZapClient["events"]> {
    return Promise.resolve(unavailable("unused events"));
  }
  streamEvents(): ReturnType<ZapClient["streamEvents"]> {
    return Promise.resolve(unavailable("unused stream"));
  }
  prepareBundle(): ReturnType<ZapClient["prepareBundle"]> {
    return Promise.resolve(unavailable("unused preparation"));
  }
  prepareComparison(): ReturnType<ZapClient["prepareComparison"]> {
    return Promise.resolve(unavailable("unused preparation"));
  }
  prepareProjectedRecord(): ReturnType<ZapClient["prepareProjectedRecord"]> {
    return Promise.resolve(unavailable("unused preparation"));
  }
  prepareCompositeSuccessor(): ReturnType<ZapClient["prepareCompositeSuccessor"]> {
    return Promise.resolve(unavailable("unused composite preparation"));
  }
  recordCompositeSuccessor(): ReturnType<ZapClient["recordCompositeSuccessor"]> {
    return Promise.resolve(unavailable("unused composite recording"));
  }
  submit(): ReturnType<ZapClient["submit"]> {
    return Promise.resolve(unavailable("unused submission"));
  }
  reconcile(): ReturnType<ZapClient["reconcile"]> {
    return Promise.resolve(unavailable("unused reconciliation"));
  }
  advanceChangeAdmission(): ReturnType<ZapClient["advanceChangeAdmission"]> {
    return Promise.resolve(unavailable("unused change admission"));
  }
}

function activeContext(): ActiveContextView {
  return {
    snapshot: {
      store_id: store.store_id,
      campaign_id: store.campaign_id,
      base_id: store.base_id,
      revision,
    },
    active_outcome: {
      state: "present",
      outcome_id: ZapIdSchema.parse("outcome.quicklens"),
      record_revision: u64("1"),
    },
    current_strategy: {
      state: "present",
      strategic_revision_id: ZapIdSchema.parse("strategy.quicklens"),
      record_revision: u64("1"),
    },
    adopted_milestone_plan: {
      state: "present",
      plan_key: { outcome_id: ZapIdSchema.parse("outcome.quicklens"), generation: u64("1") },
      plan_state_revision: u64("1"),
    },
  };
}

function overview(complete: boolean) {
  const object = { kind: "viewer", id: { kind: "work", id: "work.quicklens" } };
  const relationship = {
    id: "b".repeat(64),
    from: object,
    to: { kind: "resource", id: "resource.quicklens" },
    kind: "work_prerequisite",
    ownership_role: null,
    source: { kind: "work_record", work_id: "work.quicklens", revision: 1n },
  };
  const card = {
    object,
    semantic_type: "work",
    canonical_name: "Quicklens work",
    description: { state: "available", value: "A live task", source: "work_record" },
    purpose: { state: "available", value: "Show the plan", source: "work_record" },
    expected_result: { state: "missing", reason: "Not projected" },
    source_state: { state: "materialized", record_revision: 1n },
    acceptance: { kind: "work_record_state", state: "active", declared_criteria: [] },
    reasons: [],
    blockers: { state: "not_evaluated", reason: "Not requested" },
    sources: [],
    evidence: [],
    work_kind: "atom",
    work_type: "change",
    landmark: null,
    relationships: complete ? [relationship] : [],
    relationship_gaps: complete ? [] : [{ kind: "object_relationship_query_required" }],
    relationships_complete: complete,
    assessment_source_fingerprint: null,
    assessment_state: { state: "unavailable", reason: "No assessment" },
    assessment: null,
    observation_revision: 7n,
    underlying: null,
  };
  return {
    strategy_id: "strategy.quicklens",
    strategy_revision: 1n,
    strategy_semantic_digest: "c".repeat(64),
    strategy_state: "current",
    source_plan_nodes: 1n,
    source_plan_encoded_bytes: 100n,
    examined: 1,
    emitted: 1,
    examined_index_rows: 1n,
    emitted_relationships: complete ? 1n : 0n,
    cards: [card],
    missing_work_ids: [],
    next: null,
    through_revision: 7n,
  };
}

function milestone() {
  return {
    outcome_id: "outcome.quicklens",
    adopted_state: {},
    plan: {},
    status: "adopted",
    focus: null,
    focus_achievement_validity: null,
    unsatisfied_milestone_revision_ids: [],
    distant_horizons: [],
    gaps: [],
    query_cost: {
      whole_plan_record_decoded: true,
      milestone_records_decoded: 0,
      proof_validity_evaluations: 0,
      store_wide_proof_cost: "none",
    },
  };
}

function resourceCard() {
  const card = overview(true).cards[0];
  assert.ok(card);
  return {
    ...card,
    object: { kind: "resource", id: "resource.quicklens" },
    semantic_type: "execution_resource",
    canonical_name: "Quicklens worker",
    relationships: [],
  };
}

function outcomeCard() {
  const card = overview(true).cards[0];
  assert.ok(card);
  return {
    ...card,
    object: { kind: "viewer", id: { kind: "outcome", id: "outcome.quicklens" } },
    semantic_type: "outcome",
    canonical_name: "Quicklens outcome",
    acceptance: { kind: "outcome_state", status: "active" },
    relationships: [],
  };
}

function page<T>(item: T): ZapQueryPage<T> {
  return { store, revision, query_epoch: 1, items: [item], completeness: "complete" };
}

function operation(
  state: "prepared" | "completed" | "held",
  basis: PlanBasis,
): QuicklensResult<PlanOperationResult> {
  return {
    ok: true,
    value: {
      operationRef: QuicklensRefSchema.parse("operation.fixture"),
      previewRef: state === "prepared" ? QuicklensRefSchema.parse("preview.fixture") : null,
      preview:
        state === "prepared"
          ? {
              basis,
              changes: [
                {
                  subjectLabel: "Plan",
                  changeKind: "update",
                  beforeSummary: "Before",
                  afterSummary: "After",
                },
              ],
            }
          : null,
      state,
      message: "Fixture workflow result",
      nextBasis: basis,
    },
  };
}

function ok<T>(value: T): ZapClientResult<T> {
  return { ok: true, value };
}

function unavailable<T>(message: string): ZapClientResult<T> {
  return { ok: false, error: { kind: "configuration", message } };
}

function take<T>(
  result:
    | { readonly ok: true; readonly value: T }
    | { readonly ok: false; readonly error: unknown },
): T {
  if (result.ok) return result.value;
  assert.fail();
}

function u64(value: string) {
  return U64WireSchema.parse(parseWireJson(new TextEncoder().encode(value)));
}
