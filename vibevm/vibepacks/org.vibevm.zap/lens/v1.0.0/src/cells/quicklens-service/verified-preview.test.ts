/** @verifies spec://org.vibevm.zap/lens/PROP-002#plan-control */
import assert from "node:assert/strict";
import test from "node:test";

import {
  U64WireSchema,
  LosslessJsonSchema,
  ZapDigestSchema,
  ZapIdSchema,
  canonicalQueryInput,
  encodeCanonicalJson,
  parseWireJson,
  type LosslessJsonValue,
  type CanonicalJsonInput,
  type PreparedEffectComparisonView,
} from "../zap-client/index.ts";
import { deriveVerifiedPlanChanges, verifiedPreparedProductMatches } from "./verified-preview.ts";

const alternative = ZapIdSchema.parse("alternative.verified");
const digest = (character: string) => ZapDigestSchema.parse(character.repeat(64));

test("verified preview derives exact work and milestone changes from selected prepared effects", () => {
  const prepared = fixture([
    effect(
      "effect.work",
      "domain.work-transitioned",
      {
        schema: "zap-domain/work-transitioned/1",
        work_id: "work.release",
        from_state: "ready",
        to_state: "accepted",
        successor_ids: ["work.follow-up"],
      },
      [{ kind: "work", id: "work.release" }],
    ),
    effect(
      "effect.plan",
      "milestone.plan-adopted",
      {
        schema: "zap-domain/milestone-plan-adopted/1",
        plan: {
          key: { outcome_id: "outcome.release", generation: 3n },
          content: { milestone_revision_ids: ["milestone-revision.release.3"] },
        },
        expected_plan_state_revision: 2n,
      },
      [{ kind: "outcome", id: "outcome.release" }],
    ),
  ]);
  const result = deriveVerifiedPlanChanges(prepared, alternative);
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.deepEqual(result.value[0], {
    subjectLabel: "Work work.release",
    changeKind: "domain.work-transitioned",
    beforeSummary: "State: ready.",
    afterSummary: "State: accepted; successors: work.follow-up.",
  });
  assert.deepEqual(result.value[1], {
    subjectLabel: "Outcome outcome.release",
    changeKind: "milestone.plan-adopted",
    beforeSummary: "Expected adopted plan state revision: 2.",
    afterSummary: "Adopt milestone plan generation 3 for outcome outcome.release.",
  });
});

test("generic verified effect shows exact payload and declared subjects without invented meaning", () => {
  const prepared = fixture([
    effect(
      "effect.generic",
      "domain.supported-custom",
      { count: 9_007_199_254_740_993n, target: "resource.release" },
      [{ kind: "resource", id: "resource.release" }],
    ),
  ]);
  const result = deriveVerifiedPlanChanges(prepared, alternative);
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.value[0]?.subjectLabel, "resource:resource.release");
  assert.match(
    result.value[0]?.beforeSummary ?? "",
    /declared subjects: resource:resource\.release/,
  );
  assert.match(result.value[0]?.afterSummary ?? "", /"count":9007199254740993/);
  assert.match(result.value[0]?.afterSummary ?? "", /"target":"resource\.release"/);
});

test("missing or ambiguous selected backend alternative is refused", () => {
  const prepared = fixture([effect("effect.one", "domain.supported-custom", { value: true }, [])]);
  const result = deriveVerifiedPlanChanges(prepared, ZapIdSchema.parse("alternative.other"));
  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.error.code, "invalid_data");
});

test("executed product must equal the backend-returned selected effect", () => {
  const payload = { value: "prepared" };
  const prepared = fixture([
    effect("effect.product", "domain.supported-custom", payload, []),
    effect("effect.later", "domain.supported-later", { value: "later" }, []),
  ]);
  const product = {
    frame: {
      header: {
        protocol: 1,
        store_id: ZapIdSchema.parse("store.verified"),
        campaign_id: ZapIdSchema.parse("campaign.verified"),
        base_id: ZapIdSchema.parse("base.verified"),
        command_id: ZapIdSchema.parse("command.verified"),
        event_id: ZapIdSchema.parse("event.effect.product"),
        expected_revision: 8n,
        kind: ZapIdSchema.parse("domain.supported-custom"),
        causes: [],
        basis: { kind: "exact" as const, digest: digest("2") },
      },
      reason: { summary: "Apply", evidence: [], decision: null, change: null },
      payload: canonicalQueryInput(payload),
    },
  };
  assert.equal(verifiedPreparedProductMatches(prepared, alternative, product), true);
  const preview = deriveVerifiedPlanChanges(prepared, alternative);
  assert.equal(preview.ok && preview.value.length, 2);
  assert.equal(
    verifiedPreparedProductMatches(prepared, alternative, {
      ...product,
      frame: { ...product.frame, payload: canonicalQueryInput({ value: "different" }) },
    }),
    false,
  );
});

function effect(
  effectId: string,
  kind: string,
  payload: CanonicalJsonInput,
  subjects: readonly { readonly kind: string; readonly id: string }[],
) {
  return {
    effect_id: effectId,
    index: 0,
    kind,
    payload: canonicalQueryInput(payload),
    predecessors: [],
    product_event_id: `event.${effectId}`,
    basis: { kind: "exact", digest: digest("1") },
    declared_subjects: subjects,
    relevant_before: digest("2"),
    declared_relevant_after: digest("3"),
  };
}

function fixture(effects: readonly ReturnType<typeof effect>[]): PreparedEffectComparisonView {
  const store = {
    store_id: ZapIdSchema.parse("store.verified"),
    campaign_id: ZapIdSchema.parse("campaign.verified"),
    base_id: ZapIdSchema.parse("base.verified"),
    store_epoch: "zap/2",
    codec_epoch: 2,
    reducer_epoch: 1,
  };
  return {
    store,
    observed_revision: u64("7"),
    alternatives: [
      {
        store,
        observed_revision: u64("7"),
        request: normalized({
          alternative_id: alternative,
          committed_prefix: [],
          initial_basis: digest("2"),
          effects,
          no_op_basis: null,
          request_digest: digest("4"),
        }),
        preflight: normalized({ effects: [] }),
        affected_scopes: effects.map(() => null),
      },
    ],
    basis_request: normalized({ kind: "exact", digest: digest("2") }),
    relevant_basis: digest("2"),
    affected_scopes: [],
  };
}

function normalized(value: object): LosslessJsonValue {
  return LosslessJsonSchema.parse(parseWireJson(encodeCanonicalJson(value)));
}

function u64(value: string) {
  return U64WireSchema.parse(parseWireJson(new TextEncoder().encode(value)));
}
