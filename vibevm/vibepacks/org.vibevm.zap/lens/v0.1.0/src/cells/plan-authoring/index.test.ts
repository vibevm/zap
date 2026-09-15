import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { z } from "zod";
import { createSpecificationWatch } from "../specification-watch/index.ts";
import {
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
  createZapClient,
  encodeCanonicalJson,
  protectedCommandDigest,
  type ZapHttpExchange,
} from "../zap-client/index.ts";
import {
  createPlanAuthoring,
  MilestonePrecursorAuthoringInputSchema,
  SuccessorPlanAuthoringInputSchema,
} from "./index.ts";

test("successor authoring input rejects noncanonical milestone order", () => {
  const parsed = SuccessorPlanAuthoringInputSchema.safeParse({
    intentBasis: {
      storeRef: "store:store.one",
      baseRef: "base:base.one",
      revision: "1",
      sourceBasisRef: `source-basis:${"0".repeat(64)}`,
    },
    operationId: "operation.one",
    plan: {
      key: { outcome_id: "outcome.one", generation: 2n },
      previous: { outcome_id: "outcome.one", generation: 1n },
      strategic_revision_id: "strategy.one",
      strategic_record_revision: 1n,
      strategic_semantic_digest: "1".repeat(64),
      outcome_revision: 1n,
      expected_plan_state_revision: 1n,
      content: {
        milestone_revision_ids: ["milestone.z", "milestone.a"],
        admission_work_ids: [],
        focus_milestone_revision_id: "milestone.a",
        frontier_milestone_revision_ids: ["milestone.a"],
        horizons: [],
        obligation_coverage: [],
        rationales: [],
      },
    },
    economics: economics(),
  });
  assert.equal(parsed.success, false);
});

const liveConfig = process.env["ZAP_AUTHORING_FIXTURE"];
test(
  "real Rust HTTP fixture authors and applies one successor milestone plan",
  { skip: liveConfig === undefined },
  async () => {
    if (liveConfig === undefined) return;
    const fixture = FixtureSchema.parse(JSON.parse(await readFile(liveConfig, "utf8")));
    const endpoint = EndpointSchema.parse(
      JSON.parse(await readFile(fixture.endpoint_file, "utf8")),
    );
    const client = async (role: "reader" | "data" | "coordinator") => {
      const config = fixture.credentials[role];
      const opened = createZapClient({
        endpoint: new URL(`http://${endpoint.address}/`),
        credential: {
          id: ZapIdSchema.parse(config.id),
          bearer: await readFile(config.file, "utf8"),
        },
        exchange,
      });
      if (!opened.ok) throw new Error(`ZAP client failed: ${opened.error.kind}`);
      return opened.value;
    };
    const root = await mkdtemp(join(tmpdir(), "lens-authoring-spec-"));
    await writeFile(join(root, "goal.xml"), "<spec><goal>successor</goal></spec>", "utf8");
    const specifications = await createSpecificationWatch({
      roots: [{ root, include: ["**/*.xml"] }],
    });
    if (!specifications.ok) throw new Error(specifications.error.message);
    const reader = await client("reader");
    const data = await client("data");
    const coordinator = await client("coordinator");
    const options = {
      reader,
      data,
      coordinator,
      specifications: specifications.value,
    };
    const authoring = createPlanAuthoring(options);
    try {
      const context = await authoring.discover();
      assert.equal(context.ok, true);
      if (!context.ok) return;
      const overview = await authoring.query(
        ZapIdSchema.parse("zap.map.overview.v1"),
        {
          strategy_id: context.value.currentStrategyId,
          filter: "all",
          cursor: null,
          limit: 8,
          operation_budget: 64,
        },
        OverviewSchema,
      );
      assert.equal(overview.ok, true);
      if (!overview.ok) return;
      const strategy = overview.value.items[0];
      if (strategy === undefined) {
        throw new Error(
          "violates REQ spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#context: strategy overview missing; fix surface: configuration",
        );
      }
      const sourceDigest = ZapDigestSchema.parse(
        createHash("sha256").update("successor milestone fixture source").digest("hex"),
      );
      const input = SuccessorPlanAuthoringInputSchema.parse({
        intentBasis: context.value.intentBasis,
        operationId: "operation.node-successor",
        plan: {
          key: {
            outcome_id: context.value.activeOutcomeId,
            generation: context.value.adoptedPlan.generation + 1n,
          },
          previous: context.value.adoptedPlan,
          strategic_revision_id: context.value.currentStrategyId,
          strategic_record_revision: BigInt(strategy.strategy_revision),
          strategic_semantic_digest: strategy.strategy_semantic_digest,
          outcome_revision: 1n,
          expected_plan_state_revision: context.value.adoptedPlanStateRevision,
          content: {
            milestone_revision_ids: ["milestone-revision.http-ready.1"],
            admission_work_ids: ["work.change-admission"],
            focus_milestone_revision_id: "milestone-revision.http-ready.1",
            frontier_milestone_revision_ids: ["milestone-revision.http-ready.1"],
            horizons: [],
            obligation_coverage: [
              {
                obligation_id: "obligation.http-ready",
                milestone_revision_ids: ["milestone-revision.http-ready.1"],
              },
            ],
            rationales: [
              {
                milestone_revision_id: "milestone-revision.http-ready.1",
                kind: "consumer_outcome",
                sources: [{ source_id: "source.http-ready", digest: sourceDigest }],
                explanation: "Retain the verified boundary in the Node-authored successor",
              },
            ],
          },
        },
        economics: economics(),
      });
      const mcpInput = SuccessorPlanAuthoringInputSchema.parse(
        JSON.parse(
          JSON.stringify(input, (_key, value: unknown) =>
            typeof value === "bigint" ? value.toString() : value,
          ),
        ),
      );
      const inputCapture = process.env["ZAP_AUTHORING_INPUT_CAPTURE"];
      if (inputCapture !== undefined) {
        await writeFile(
          inputCapture,
          JSON.stringify(mcpInput, (_key, value: unknown) =>
            typeof value === "bigint" ? value.toString() : value,
          ),
          "utf8",
        );
      }
      const plan = await authoring.prepareSuccessor(mcpInput);
      assert.equal(plan.ok, true, plan.ok ? undefined : plan.error.message);
      if (!plan.ok) return;
      const commandCapture = process.env["ZAP_AUTHORING_COMMAND_CAPTURE"];
      if (commandCapture !== undefined) {
        await writeFile(commandCapture, encodeCanonicalJson({ prepared: plan.value.command }));
      }
      const planSubmitted = await authoring.submitMetadata(plan.value.command);
      assert.equal(planSubmitted.ok, true);
      if (!planSubmitted.ok) return;
      const reopenedAfterPlan = createPlanAuthoring(options);
      const planReceipt = await reconcileInChild(liveConfig, root, plan.value.command);
      const assessment = await reopenedAfterPlan.prepareAssessment(plan.value, planReceipt);
      assert.equal(assessment.ok, true, assessment.ok ? undefined : assessment.error.message);
      if (!assessment.ok) return;
      const assessmentSubmitted = await reopenedAfterPlan.submitMetadata(assessment.value.command);
      assert.equal(assessmentSubmitted.ok, true);
      if (!assessmentSubmitted.ok) return;
      const reopenedAfterAssessment = createPlanAuthoring(options);
      const assessmentReceipt = await reconcileInChild(liveConfig, root, assessment.value.command);
      const prepared = await reopenedAfterAssessment.finishSuccessor(
        assessment.value,
        assessmentReceipt,
      );
      assert.equal(prepared.ok, true, prepared.ok ? undefined : prepared.error.message);
      if (!prepared.ok) return;
      assert.notEqual(prepared.value.intentBasis.revision, prepared.value.preparedBasis.revision);
      const step = await reopenedAfterAssessment.prepareEffect(prepared.value, {
        effectIndex: 0,
        completedPrefix: [],
        decisionId: null,
      });
      assert.equal(step.ok, true, step.ok ? undefined : step.error.message);
      if (!step.ok) return;
      const admitted = await coordinator.advanceChangeAdmission(step.value.advanceRequest);
      assert.equal(
        admitted.ok,
        true,
        admitted.ok
          ? undefined
          : admitted.error.kind === "http_refusal"
            ? admitted.error.refusal.message
            : admitted.error.kind,
      );
      if (!admitted.ok) return;
      assert.equal(admitted.value.status, "ready");
      const product = step.value.advanceRequest.product;
      const submitted = await coordinator.submit("command", {
        command: product,
        reconciliation: {
          command_id: product.frame.header.command_id,
          command_digest: step.value.productDigest,
        },
      });
      assert.equal(submitted.ok, true);
      if (submitted.ok) assert.equal(submitted.value.status, "committed");
      assert.equal(protectedCommandDigest(product), step.value.productDigest);

      const dynamicAuthoring = createPlanAuthoring(options);
      const dynamicContext = await dynamicAuthoring.discover();
      assert.equal(dynamicContext.ok, true);
      if (!dynamicContext.ok) return;
      const dynamicOperation = ZapIdSchema.parse("operation.node-dynamic-milestone");
      const precursor = await dynamicAuthoring.prepareMilestonePrecursors(
        MilestonePrecursorAuthoringInputSchema.parse({
          intentBasis: dynamicContext.value.intentBasis,
          operationId: dynamicOperation,
          changes: [
            {
              kind: "create",
              affected_work_ids: ["work.change-admission"],
              definition: {
                name: "Dynamically authored milestone",
                purpose: "Prove an MCP agent can add a milestone",
                result_criterion: "The new revision is included in an adopted successor",
                consumers: [{ kind: "outcome", id: dynamicContext.value.activeOutcomeId }],
                required_obligation_ids: ["obligation.http-ready"],
                contributions: [{ kind: "work", work_id: "work.change-admission" }],
                dependencies: [],
                lifecycle: "active",
                retirement_reason: null,
              },
            },
          ],
        }),
      );
      assert.equal(precursor.ok, true, precursor.ok ? undefined : precursor.error.message);
      if (!precursor.ok) return;
      const dynamicRevision = precursor.value.changes[0]?.revisionId;
      if (dynamicRevision === undefined) {
        throw new Error(
          "violates REQ spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#effects: dynamic revision missing; fix surface: implementation",
        );
      }
      const dynamicInput = SuccessorPlanAuthoringInputSchema.parse({
        intentBasis: dynamicContext.value.intentBasis,
        operationId: dynamicOperation,
        plan: {
          key: {
            outcome_id: dynamicContext.value.activeOutcomeId,
            generation: dynamicContext.value.adoptedPlan.generation + 1n,
          },
          previous: dynamicContext.value.adoptedPlan,
          strategic_revision_id: dynamicContext.value.currentStrategyId,
          strategic_record_revision: dynamicContext.value.currentStrategyRecordRevision,
          strategic_semantic_digest: dynamicContext.value.currentStrategySemanticDigest,
          outcome_revision: dynamicContext.value.activeOutcomeRevision,
          expected_plan_state_revision: dynamicContext.value.adoptedPlanStateRevision,
          content: {
            milestone_revision_ids: [dynamicRevision],
            admission_work_ids: ["work.change-admission"],
            focus_milestone_revision_id: dynamicRevision,
            frontier_milestone_revision_ids: [dynamicRevision],
            horizons: [],
            obligation_coverage: [
              {
                obligation_id: "obligation.http-ready",
                milestone_revision_ids: [dynamicRevision],
              },
            ],
            rationales: [
              {
                milestone_revision_id: dynamicRevision,
                kind: "consumer_outcome",
                sources: [{ source_id: "source.http-ready", digest: sourceDigest }],
                explanation: "Bind the dynamically created milestone to the verified source",
              },
            ],
          },
        },
        economics: { ...economics(), change_id: "change.node-dynamic-milestone" },
      });
      const composite = await dynamicAuthoring.prepareCompositeSuccessor(
        dynamicInput,
        precursor.value,
      );
      assert.equal(composite.ok, true, composite.ok ? undefined : composite.error.message);
      if (!composite.ok) return;
      const recorded = await dynamicAuthoring.recordCompositeSuccessor(composite.value);
      assert.equal(recorded.ok, true, recorded.ok ? undefined : recorded.error.message);
      if (!recorded.ok || recorded.value.submission.status !== "committed") return;
      const compositeReceipt = {
        commandId: composite.value.composite.reconciliation.command_id,
        commandDigest: composite.value.composite.reconciliation.command_digest,
        revision: String(recorded.value.submission.receipt.revision),
      };
      const dynamicAssessment = await dynamicAuthoring.prepareCompositeAssessment(
        composite.value,
        compositeReceipt,
      );
      assert.equal(
        dynamicAssessment.ok,
        true,
        dynamicAssessment.ok ? undefined : dynamicAssessment.error.message,
      );
      if (!dynamicAssessment.ok) return;
      const dynamicAssessmentSubmit = await dynamicAuthoring.submitMetadata(
        dynamicAssessment.value.command,
      );
      assert.equal(dynamicAssessmentSubmit.ok, true);
      if (!dynamicAssessmentSubmit.ok) return;
      const dynamicAssessmentReceipt = await reconcileInChild(
        liveConfig,
        root,
        dynamicAssessment.value.command,
      );
      const dynamicPrepared = await dynamicAuthoring.finishSuccessor(
        dynamicAssessment.value,
        dynamicAssessmentReceipt,
      );
      assert.equal(
        dynamicPrepared.ok,
        true,
        dynamicPrepared.ok ? undefined : dynamicPrepared.error.message,
      );
      if (!dynamicPrepared.ok) return;
      const completedPrefix: ReturnType<typeof ZapIdSchema.parse>[] = [];
      for (let effectIndex = 0; effectIndex < dynamicPrepared.value.effects.length; effectIndex++) {
        const dynamicStep = await dynamicAuthoring.prepareEffect(dynamicPrepared.value, {
          effectIndex,
          completedPrefix,
          decisionId: null,
        });
        assert.equal(dynamicStep.ok, true, dynamicStep.ok ? undefined : dynamicStep.error.message);
        if (!dynamicStep.ok) return;
        const advanced = await coordinator.advanceChangeAdmission(dynamicStep.value.advanceRequest);
        assert.equal(advanced.ok, true);
        if (!advanced.ok || advanced.value.status !== "ready") return;
        const dynamicProduct = dynamicStep.value.advanceRequest.product;
        const applied = await coordinator.submit("command", {
          command: dynamicProduct,
          reconciliation: {
            command_id: dynamicProduct.frame.header.command_id,
            command_digest: dynamicStep.value.productDigest,
          },
        });
        assert.equal(applied.ok, true);
        const completed = dynamicPrepared.value.effects[effectIndex];
        if (completed !== undefined) completedPrefix.push(completed.effectId);
      }
      const finalContext = await dynamicAuthoring.discover();
      assert.equal(finalContext.ok, true);
      if (finalContext.ok) assert.equal(finalContext.value.adoptedPlan.generation, 3n);
    } finally {
      specifications.value.close();
    }
  },
);

const FixtureSchema = z
  .object({
    endpoint_file: z.string(),
    credentials: z.object({
      reader: z.object({ id: z.string(), file: z.string() }).strict(),
      data: z.object({ id: z.string(), file: z.string() }).strict(),
      coordinator: z.object({ id: z.string(), file: z.string() }).strict(),
    }),
  })
  .passthrough();
const EndpointSchema = z.object({ address: z.string() }).passthrough();
const OverviewSchema = z
  .object({
    strategy_revision: U64WireSchema,
    strategy_semantic_digest: ZapDigestSchema,
  })
  .passthrough();

const exchange: ZapHttpExchange = {
  request: async (input) => {
    const response = await fetch(input.url, {
      method: input.method,
      headers: input.headers,
      ...(input.body === undefined ? {} : { body: Buffer.from(input.body) }),
      ...(input.signal === undefined ? {} : { signal: input.signal }),
    });
    const headers: Record<string, string> = {};
    response.headers.forEach((value, key) => {
      headers[key] = value;
    });
    return { status: response.status, headers, body: new Uint8Array(await response.arrayBuffer()) };
  },
};

function economics() {
  const zero = { low: 0n, high: 0n };
  const one = { low: 1_000_000n, high: 1_000_000n };
  const categories = [
    "implementation",
    "verification",
    "migration",
    "documentation",
    "proof_revalidation",
    "dependencies_consumers",
    "operations_maintenance",
    "passive_wait_external",
    "fog_uncertainty",
  ].map((category) => ({
    category,
    applicability: "included",
    agent_hours: zero,
    elapsed: zero,
    consequence: "negligible",
    basis: "No extra fixture category cost",
    evidence_refs: [],
  }));
  const utility = (band: "high" | "low") => ({
    overall: band,
    owner_benefit: band,
    risk_reduction: band === "high" ? "moderate" : "low",
    urgency: band === "high" ? "moderate" : "low",
    strategic_optionality: band === "high" ? "moderate" : "low",
    reversibility: "high",
    confidence: "high",
    basis: `${band} fixture utility`,
    evidence_refs: [],
  });
  const cost = (interval: typeof zero) => ({
    expected_elapsed: interval.low,
    elapsed_interval: interval,
    expected_passive_wait: 0n,
    passive_wait_interval: zero,
    total_agent_hours: interval.low,
    agent_hours_interval: interval,
    precision: "bounded_estimate",
    consequence: "negligible",
    categories,
    unknowns: [],
    excluded_costs: [],
    attribution_summary: "One prepared effect",
  });
  return {
    change_id: "change.node-successor",
    summary: "Adopt one Node-authored successor milestone plan",
    necessity: {
      class: "optional_improvement",
      obligation_ids: [],
      constraint_refs: [],
      problem: "Exercise the public authoring seam",
      basis: "Current strategy remains applicable",
      evidence_refs: [],
    },
    team_model: {
      model_id: "team.node-successor",
      profile_digest: createHash("sha256").update("team.node-successor").digest("hex"),
      executor_classes: [{ class_id: "test", capability_ids: ["typescript"], nominal_capacity: 1 }],
      nominal_parallelism: 1,
      resource_capacities: [],
      scheduling_assumptions: ["One test executor"],
      evidence_refs: [],
    },
    proposal: {
      summary: "Adopt the successor plan",
      solves_mandatory_problem: true,
      preserved_obligations: [],
      sacrificed_obligations: [],
      utility: utility("high"),
      cost: cost(one),
      feasibility: "feasible",
      basis: "Prepared against current state",
      evidence_refs: [],
    },
    no_op: {
      summary: "Keep the current milestone plan",
      utility: utility("low"),
      cost: cost(zero),
      feasibility: "feasible",
      basis: "Current plan remains available",
      evidence_refs: [],
    },
    comparison_reasons: ["Bounded useful successor"],
    estimation: {
      elapsed: 100_000n,
      agent_hours: 100_000n,
      stopped_because: "sufficient",
      assumptions: ["Stable fixture"],
      evidence_refs: [],
    },
  };
}

async function reconcileInChild(
  fixture: string,
  root: string,
  prepared: {
    command: { frame: { header: { command_id: ReturnType<typeof ZapIdSchema.parse> } } };
    commandDigest: ReturnType<typeof ZapDigestSchema.parse>;
  },
) {
  const input = join(root, `${prepared.command.frame.header.command_id}.json`);
  await writeFile(
    input,
    JSON.stringify({
      fixture,
      reconciliation: {
        command_id: prepared.command.frame.header.command_id,
        command_digest: prepared.commandDigest,
      },
    }),
    "utf8",
  );
  const child = spawnSync(
    process.execPath,
    [fileURLToPath(new URL("./fixtures/reconcile-process.ts", import.meta.url)), input],
    { encoding: "utf8" },
  );
  if (child.status !== 0) {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#reconciliation: child process could not reconcile metadata; fix surface: retry_after_reconcile",
    );
  }
  const result = z
    .object({
      ok: z.literal(true),
      commandId: ZapIdSchema,
      revision: z.string().regex(/^(0|[1-9][0-9]*)$/),
    })
    .strict()
    .parse(JSON.parse(child.stdout));
  return {
    commandId: result.commandId,
    commandDigest: prepared.commandDigest,
    revision: result.revision,
  };
}
