import assert from "node:assert/strict";
import test from "node:test";
import { QuicklensSnapshotSchema } from "../quicklens-model/index.ts";
import { bindPlanningSourceIdentity, claimPlanningSourceIdentity } from "./index.ts";

test("dynamic planning attachment binds only an authoritative exact active context", () => {
  const snapshot = QuicklensSnapshotSchema.parse({
    sourceMode: "live",
    sourceLabel: "Plan workspace source",
    phase: "ready",
    phaseDetail: null,
    capturedAt: "2026-09-16T12:00:00.000Z",
    revision: "7",
    objects: [],
    relationships: [],
    questions: [],
    agentTargets: [],
    plan: {
      outcomeLabel: "Outcome",
      strategyLabel: "Strategy",
      planLabel: "Plan",
      basis: {
        storeRef: "store:store.fixture",
        baseRef: "base:base.fixture",
        revision: "7",
        sourceBasisRef: "source-basis:fixture",
      },
      state: "current",
      detail: null,
      actions: {
        propose: { enabled: true, reason: null },
        preview: { enabled: false, reason: "No intent" },
        apply: { enabled: false, reason: "No preview" },
        reconcile: { enabled: false, reason: "No operation" },
      },
    },
  });
  const identity = {
    storeId: "store.fixture",
    campaignId: "campaign.fixture",
    baseId: "base.fixture",
    snapshotRevision: "7",
    adoptedPlanKey: { outcomeId: "outcome.fixture", generation: 2 },
  };
  const bound = bindPlanningSourceIdentity(identity, snapshot);
  assert.equal(bound.ok, true);
  if (bound.ok) {
    assert.equal(bound.value.state, "bound");
    if (bound.value.state === "bound") assert.equal(bound.value.campaignId, "campaign.fixture");
  }
  assert.equal(
    bindPlanningSourceIdentity({ ...identity, campaignId: "campaign.other" }, snapshot).ok,
    true,
  );
  assert.equal(
    bindPlanningSourceIdentity({ ...identity, baseId: "base.other" }, snapshot).ok,
    false,
  );
  assert.equal(bindPlanningSourceIdentity(identity, { ...snapshot, phase: "partial" }).ok, false);
  const owners = new Map<string, string>();
  if (bound.ok) {
    assert.equal(claimPlanningSourceIdentity(owners, "project-a/context-a", bound.value).ok, true);
    assert.equal(claimPlanningSourceIdentity(owners, "project-b/context-b", bound.value).ok, false);
  }
});
