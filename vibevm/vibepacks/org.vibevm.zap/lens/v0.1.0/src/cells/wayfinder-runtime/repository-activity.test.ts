import assert from "node:assert/strict";
import test from "node:test";
import type { ManagedWorkClaim } from "../managed-work/index.ts";
import { RunIdSchema } from "../workspace-model/index.ts";
import { classifyRepositoryWriterActivity } from "./repository-activity.ts";

type Claim = Pick<ManagedWorkClaim, "runId" | "state" | "managedControl">;

test("writer activity distinguishes settled pause from live and unowned writers", () => {
  const running = claim("run.running", "running", false, "busy");
  assert.equal(classifyRepositoryWriterActivity([running], []), "active");
  const paused = claim("run.paused", "paused", true, "idle");
  assert.equal(
    classifyRepositoryWriterActivity([paused], [{ runId: paused.runId, state: "running" }]),
    "idle",
  );
  assert.equal(
    classifyRepositoryWriterActivity([], [{ runId: "run.human", state: "running" }]),
    "active",
  );
  assert.equal(classifyRepositoryWriterActivity([], []), "idle");
});

function claim(
  runId: string,
  state: Claim["state"],
  pauseRequested: boolean,
  readiness: NonNullable<Claim["managedControl"]>["readiness"],
): Claim {
  return {
    runId: RunIdSchema.parse(runId),
    state,
    managedControl: {
      processEpoch: "epoch.fixture",
      readiness,
      providerSessionId: "provider.fixture",
      providerTurnId: null,
      observationId: "observation.fixture",
      automationControlEpoch: "1",
      pauseRequested,
      continuation: "live",
    },
  };
}
