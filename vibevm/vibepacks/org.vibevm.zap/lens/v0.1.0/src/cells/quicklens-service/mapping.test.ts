/** @verifies spec://org.vibevm.zap/lens/PROP-002#semantic-map */
import assert from "node:assert/strict";
import test from "node:test";

import { SemanticObjectSchema } from "../quicklens-model/index.ts";
import { encodeCanonicalJson, parseCanonicalJson } from "../zap-client/index.ts";
import { mapCard, mapMilestonePlanProjection, mapRelationships } from "./mapping.ts";
import { MilestonePlanViewSchema, SemanticCardWireSchema } from "./wire.ts";

test("real Rust card shape preserves long text, exact assessment metrics and human meaning", () => {
  const huge = 9_007_199_254_740_993n;
  const object = { kind: "viewer", id: { kind: "work", id: "work.realistic" } };
  const raw = {
    object,
    semantic_type: "work",
    canonical_name: "T".repeat(4_096),
    description: { state: "available", value: "Detailed work description", source: "work_record" },
    purpose: { state: "available", value: "P".repeat(16_384), source: "task_contract" },
    expected_result: {
      state: "available",
      value: "A verified, integrated result",
      source: "task_contract",
    },
    source_state: { state: "materialized", record_revision: huge },
    acceptance: {
      kind: "work_record_state",
      state: "active",
      declared_criteria: ["Browser and Electron show the same exact semantic card"],
    },
    reasons: ["The active plan selected this work for the current milestone"],
    blockers: {
      state: "established",
      blockers: [{ kind: "viewer", id: { kind: "work", id: "work.blocker" } }],
    },
    sources: ["source.plan"],
    evidence: ["evidence.measurement"],
    work_kind: "atom",
    work_type: "integration",
    landmark: null,
    relationships: [
      {
        id: "a".repeat(64),
        from: object,
        to: { kind: "viewer", id: { kind: "work", id: "work.blocker" } },
        kind: "work_prerequisite",
        ownership_role: null,
        source: { kind: "work_record", work_id: "work.realistic", revision: huge },
      },
    ],
    relationship_gaps: [],
    relationships_complete: true,
    assessment_source_fingerprint: "b".repeat(64),
    assessment_state: { state: "available", freshness: "stale" },
    assessment: {
      freshness: "stale",
      record: {
        work_id: "work.realistic",
        source_fingerprint: "b".repeat(64),
        content: {
          display_label: "Measured delivery assessment",
          explanation: "Assessment is retained for comparison while its source is stale.",
          remaining_agent_hours: {
            range: { low: huge, high: huge + 1_000_000n },
            precision: "bounded_estimate",
            source: "Measured implementation ledger",
            assumptions: ["One engineer continues the current implementation"],
          },
          remaining_elapsed: {
            range: { low: 2_000_000n, high: 4_000_000n },
            precision: "bounded_estimate",
            source: "Critical-path estimate",
            assumptions: ["No new blocking dependency appears"],
          },
          remaining_passive_wait: {
            range: { low: 500_000n, high: 1_500_000n },
            precision: "measured",
            source: "Observed dependency wait",
            assumptions: ["Current remote response timing continues"],
          },
          complexity: { grade: "high", rationale: "Several bounded state machines interact." },
          difficulty: {
            grade: "medium",
            rationale: "The APIs are known but exact identity must be preserved.",
            executor_assumptions: ["A TypeScript implementer owns the shell"],
            knowledge_assumptions: ["The ZAP wire contract is available"],
          },
          uncertainty: {
            confidence: "low",
            rationale: "Only one external timing remains uncertain.",
            unknowns: ["Remote response latency"],
          },
          evidence_refs: ["evidence.measurement"],
        },
        revision: huge,
      },
    },
    observation_revision: huge,
    underlying: null,
  };
  const decoded = parseCanonicalJson(encodeCanonicalJson(raw));
  const card = SemanticCardWireSchema.parse(decoded);
  const mapped = SemanticObjectSchema.parse(mapCard(card));
  assert.equal(mapped.title.length, 4_096);
  assert.equal(mapped.purpose?.length, 16_384);
  assert.equal(mapped.description, "Detailed work description");
  assert.equal(mapped.expectedResult, "A verified, integrated result");
  assert.equal(mapped.status.label, "Active");
  assert.match(mapped.acceptance ?? "", /Criteria: Browser and Electron/);
  assert.equal(mapped.metrics.complexity.state, "known");
  assert.equal(
    mapped.metrics.complexity.state === "known" && mapped.metrics.complexity.value,
    "High",
  );
  assert.equal(
    mapped.metrics.effort.state === "known" && mapped.metrics.effort.value,
    "9007199254.740993–9007199255.740993",
  );
  assert.equal(mapped.metrics.waiting.state === "known" && mapped.metrics.waiting.value, "0.5–1.5");
  assert.match(
    mapped.metrics.difficulty.state === "known"
      ? (mapped.metrics.difficulty.explanation ?? "")
      : "",
    /^Stale assessment\./,
  );
  assert.equal(mapped.blockers[0]?.fallbackLabel, "Work blocker");
  assert.doesNotThrow(() => mapRelationships(card));
  assert.match(mapRelationships(card)[0]?.provenance[0]?.detail ?? "", /9007199254740993/);
});

test("milestone, outcome and obligation status use their declared lifecycle fields", () => {
  const cases = [
    {
      semanticType: "milestone",
      object: { kind: "milestone", id: "milestone.release" },
      acceptance: { kind: "milestone_state", lifecycle: "retired", latest_achievement_id: null },
      label: "Retired",
      tone: "neutral",
    },
    {
      semanticType: "outcome",
      object: { kind: "viewer", id: { kind: "outcome", id: "outcome.release" } },
      acceptance: { kind: "outcome_state", status: "active" },
      label: "Active",
      tone: "active",
    },
    {
      semanticType: "obligation",
      object: { kind: "viewer", id: { kind: "obligation", id: "obligation.release" } },
      acceptance: {
        kind: "obligation_state",
        status: "unattainable",
        disposition: "unattainable",
      },
      label: "Unattainable",
      tone: "danger",
    },
  ] as const;
  for (const entry of cases) {
    const card = SemanticCardWireSchema.parse(
      parseCanonicalJson(
        encodeCanonicalJson(minimalCard(entry.semanticType, entry.object, entry.acceptance)),
      ),
    );
    const mapped = SemanticObjectSchema.parse(mapCard(card));
    assert.equal(mapped.status.label, entry.label);
    assert.equal(mapped.status.tone, entry.tone);
  }
});

test("adopted plan projects focus and unsatisfied milestone refs without a current strategy", () => {
  const plan = MilestonePlanViewSchema.parse(
    parseCanonicalJson(
      encodeCanonicalJson({
        outcome_id: "outcome.release",
        adopted_state: null,
        plan: {
          key: { outcome_id: "outcome.release", generation: "3" },
          strategic_revision_id: "strategy.adopted",
          content: {
            admission_work_ids: ["work.admission"],
            milestone_revision_ids: ["milestone-revision.focus.1", "milestone-revision.other.1"],
            frontier_milestone_revision_ids: ["milestone-revision.focus.1"],
            focus_milestone_revision_id: "milestone-revision.focus.1",
          },
        },
        status: "adopted",
        focus: {
          milestone_id: "milestone.focus",
          revision_id: "milestone-revision.focus.1",
          definition: {
            name: "Verified release boundary",
            purpose: "Keep the adopted plan visible without a current strategy.",
            result_criterion: "The release proof is accepted.",
            lifecycle: "active",
          },
        },
        focus_achievement_validity: null,
        unsatisfied_milestone_revision_ids: [
          "milestone-revision.focus.1",
          "milestone-revision.other.1",
        ],
        distant_horizons: [],
        gaps: [],
        query_cost: {
          whole_plan_record_decoded: true,
          milestone_records_decoded: 1,
          proof_validity_evaluations: 0,
          store_wide_proof_cost: "none",
        },
      }),
    ),
  );
  const projection = mapMilestonePlanProjection(plan);
  assert.equal(projection.strategyId, "strategy.adopted");
  assert.equal(projection.objects.length, 2);
  assert.equal(projection.objects[0]?.title, "Verified release boundary");
  assert.equal(projection.objects[0]?.status.label, "Unsatisfied");
  assert.equal(projection.objects[1]?.title, "Unsatisfied milestone");
  assert.equal(projection.objects[1]?.title.includes("milestone-revision"), false);
  assert.deepEqual(projection.memberRevisionIds, [
    "milestone-revision.focus.1",
    "milestone-revision.other.1",
  ]);
  assert.deepEqual(projection.queryObjects, [
    { kind: "viewer", id: { kind: "outcome", id: "outcome.release" } },
    { kind: "milestone", id: "milestone.focus" },
    { kind: "viewer", id: { kind: "work", id: "work.admission" } },
  ]);
  assert.deepEqual(projection.navigation, {
    adoptedPlanRef: "milestone_plan:outcome.release:3",
    members: [],
    focusRef: "milestone:milestone.focus",
    currentWorkRefs: [],
  });
});

function minimalCard(semanticType: string, object: object, acceptance: object) {
  return {
    object,
    semantic_type: semanticType,
    canonical_name: `${semanticType} card`,
    description: { state: "missing", reason: "Not supplied" },
    purpose: { state: "available", value: "Readable purpose", source: "reference_identity" },
    expected_result: { state: "missing", reason: "Not supplied" },
    source_state: { state: "materialized", record_revision: 1n },
    acceptance,
    reasons: [],
    blockers: { state: "not_applicable", reason: "No blocker model for this card" },
    sources: [],
    evidence: [],
    work_kind: null,
    work_type: null,
    landmark: null,
    relationships: [],
    relationship_gaps: [],
    relationships_complete: true,
    assessment_source_fingerprint: null,
    assessment_state: { state: "not_applicable" },
    assessment: null,
    observation_revision: 1n,
    underlying: null,
  };
}
