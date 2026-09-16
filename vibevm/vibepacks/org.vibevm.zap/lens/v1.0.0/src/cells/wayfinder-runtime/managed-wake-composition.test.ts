/** Aggregate managed lifecycle and exact wake fencing. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import assert from "node:assert/strict";
import test from "node:test";
import { ModelSelectionSchema } from "../model-policy/index.ts";
import {
  ManagedSessionControlEventSchema,
  ManagedWorkClaimSchema,
  type ManagedAgentBackend,
  type ManagedControlTarget,
  type ManagedSessionControlEvent,
  type ManagedSessionControlPort,
  type ManagedSessionReadiness,
  type ManagedWorkClaim,
  type ManagedWorkResult,
} from "../managed-work/index.ts";
import { ActorIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  AttemptIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  TaskIdSchema,
  TerminalIdSchema,
  WorkContextIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { ManagedWakeDeliverySchema } from "../workspace-store/index.ts";
import { createWayfinderManagedWake } from "./managed-wake-composition.ts";

const projectId = ProjectIdSchema.parse("project.aggregate");
const contextId = WorkContextIdSchema.parse("context.aggregate");

test("pause aggregates two workers and handles a settlement emitted before return", async () => {
  const first = claim("first", "busy");
  const second = claim("second", "idle");
  const fixture = managedFixture([first, second]);
  const wake = createWayfinderManagedWake({
    backend: fixture.backend,
    projects: [{ projectId, contextId }],
  });
  const events: ManagedSessionControlEvent[] = [];
  const unsubscribe = wake.subscribe((event) => {
    if (event.action === "pause" && event.state === "paused")
      events.push(fixture.control.lastEvent);
  });
  const requested = await wake.pauseOwned(access(), projectId, contextId);
  assert.equal(requested, "requested", "the later idle worker cannot erase a prior request");
  fixture.control.settle(first.runId);
  await nextTurn();
  assert.equal(events.length, 1);

  fixture.control.reset(first.runId, "busy");
  fixture.control.reset(second.runId, "idle");
  fixture.control.emitInsideInterrupt = true;
  const settled = await wake.pauseOwned(access(), projectId, contextId);
  assert.equal(settled, "settled");
  unsubscribe();
});

test("continue proves every worker ready, observes late scopes and fences an old claimed epoch", async () => {
  const first = claim("continue-a", "stopped", "paused");
  const second = claim("continue-b", "stopped", "paused");
  const late = claim("late", "idle");
  const scopes = [{ projectId, contextId }];
  const fixture = managedFixture([first, second]);
  const wake = createWayfinderManagedWake({ backend: fixture.backend, projects: () => scopes });
  const seen: string[] = [];
  const unsubscribe = wake.subscribe((event) => {
    seen.push(`${event.contextId}:${event.action}:${event.state}`);
  });
  const continued = await wake.continueOwned(access(), projectId, contextId);
  assert.equal(continued, "settled");
  await nextTurn();
  assert.equal(
    seen.some((value) => value.endsWith(":continue:idle")),
    true,
  );

  const lateContext = WorkContextIdSchema.parse("context.late");
  scopes.push({ projectId, contextId: lateContext });
  fixture.add({
    ...late,
    packet: { ...late.packet, contextId: lateContext },
  });
  fixture.control.publish(late.runId, "turn_settled");
  await nextTurn();
  assert.equal(
    seen.some((value) => value.startsWith(`${lateContext}:wake:idle`)),
    true,
  );

  const current = fixture.get(first.runId);
  assert.ok(current !== undefined && current.managedControl !== null);
  if (current === undefined || current.managedControl === null) return;
  const refused = await wake.offerWake({
    notice: ManagedWakeDeliverySchema.parse({
      wakeId: "request.old-epoch",
      projectId,
      contextId,
      actorId: current.actorId,
      runId: current.runId,
      attemptId: current.attemptId,
      adapterSessionId: current.adapterSessionId,
      kind: "answer",
      questionGroupId: "question.old-epoch",
      answerVersionId: "answer.old-epoch",
      bodyMarkdown: "Old claimed delivery",
      sourceEventId: "question:old-epoch",
      state: "offered",
      updatedAt: new Date().toISOString(),
      processEpoch: "process.continue-a.old",
      leaseId: "lease.continue-a.old",
    }),
    leaseId: "lease.continue-a.old",
  });
  assert.equal(refused, "refused");
  assert.equal(fixture.control.offers, 0);
  unsubscribe();
});

interface ControlState {
  claim: ManagedWorkClaim;
  readiness: ManagedSessionReadiness;
  pauseRequested: boolean;
  automationControlEpoch: string | null;
  providerSessionId: string | null;
}

class ControlFixture implements ManagedSessionControlPort {
  readonly states = new Map<string, ControlState>();
  readonly listeners = new Set<(event: ManagedSessionControlEvent) => void>();
  offers = 0;
  emitInsideInterrupt = false;
  lastEvent = eventFor(claim("event", "idle"), "turn_settled");

  inspect(target: ManagedControlTarget) {
    const state = this.states.get(target.runId);
    return state === undefined ||
      state.claim.managedControl?.processEpoch !== target.expectedProcessEpoch
      ? unavailable("stale control target")
      : {
          ok: true as const,
          value: {
            readiness: state.readiness,
            providerSessionId: state.providerSessionId,
            providerTurnId: null,
            observationId: `observation.${target.runId}`,
            automationControlEpoch: state.automationControlEpoch,
            pauseRequested: state.pauseRequested,
          },
        };
  }
  async interrupt(target: ManagedControlTarget) {
    const state = this.states.get(target.runId);
    if (state === undefined) return unavailable("missing control target");
    state.pauseRequested = true;
    if (state.readiness === "idle")
      return { ok: true as const, value: { observation: "already_idle" as const } };
    if (this.emitInsideInterrupt) {
      this.settle(target.runId);
      return { ok: true as const, value: { observation: "requested" as const } };
    }
    state.readiness = "interrupting";
    return { ok: true as const, value: { observation: "requested" as const } };
  }
  continueSession(target: ManagedControlTarget) {
    const state = this.states.get(target.runId);
    if (state === undefined) return unavailable("missing control target");
    state.pauseRequested = false;
    return { ok: true as const, value: { readiness: state.readiness } };
  }
  async offer() {
    this.offers += 1;
    return {
      ok: true as const,
      value: { observation: "host_accepted" as const, transportCorrelation: null },
    };
  }
  subscribe(listener: (event: ManagedSessionControlEvent) => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  close() {
    this.listeners.clear();
  }
  reset(runId: string, readiness: ManagedSessionReadiness) {
    const state = this.states.get(runId);
    if (state === undefined) return;
    state.readiness = readiness;
    state.pauseRequested = false;
  }
  settle(runId: string) {
    const state = this.states.get(runId);
    if (state === undefined) return;
    state.readiness = "idle";
    this.publish(runId, "turn_settled");
  }
  publish(runId: string, kind: ManagedSessionControlEvent["kind"]) {
    const state = this.states.get(runId);
    if (state === undefined) return;
    const event = eventFor(state.claim, kind);
    this.lastEvent = event;
    for (const listener of this.listeners) listener(event);
  }
}

function managedFixture(initial: readonly ManagedWorkClaim[]) {
  const claims = new Map(initial.map((item) => [item.runId, item]));
  const control = new ControlFixture();
  for (const item of initial)
    control.states.set(item.runId, {
      claim: item,
      readiness: item.managedControl?.readiness ?? "unknown",
      pauseRequested: item.managedControl?.pauseRequested ?? false,
      automationControlEpoch: item.managedControl?.automationControlEpoch ?? null,
      providerSessionId: item.managedControl?.providerSessionId ?? null,
    });
  const backend: ManagedAgentBackend = {
    capabilities: [],
    control,
    registerProfile: () => unavailable("unused"),
    prepare: async () => unavailable("unused"),
    get: (_access, runId) => result(claims.get(runId)),
    list: (_access, scopedProject, scopedContext) => ({
      ok: true,
      value: [...claims.values()].filter(
        (item) =>
          item.packet.projectId === scopedProject && item.packet.contextId === scopedContext,
      ),
    }),
    profiles: () => ({ ok: true, value: [] }),
    start: async () => unavailable("unused"),
    interrupt: async () => unavailable("unused"),
    stop: async (_access, runId) => update(claims, runId, "stopped"),
    continueRun: async (_access, runId) => {
      const current = claims.get(runId);
      if (current === undefined) return unavailable("missing claim");
      const suffix = current.runId.split(".").at(-1) ?? "continued";
      const next = ManagedWorkClaimSchema.parse({
        ...current,
        state: "running",
        controlLeaseId: `lease.${suffix}.2`,
        controlEpoch: "2",
        managedControl: {
          ...current.managedControl,
          processEpoch: `process.${suffix}.2`,
          readiness: "starting",
          providerSessionId: `provider-session.${suffix}.2`,
          automationControlEpoch: "2",
          pauseRequested: false,
        },
        revision: String(BigInt(current.revision) + 1n),
      });
      claims.set(runId, next);
      control.states.set(runId, {
        claim: next,
        readiness: "starting",
        pauseRequested: false,
        automationControlEpoch: "2",
        providerSessionId: `provider-session.${suffix}.2`,
      });
      control.publish(runId, "session_ready");
      return { ok: true, value: next };
    },
    reconcile: async (_access, runId) => result(claims.get(runId)),
    report: async () => unavailable("unused"),
    review: async () => unavailable("unused"),
    acknowledgeAttachment: async () => ({ ok: true, value: null }),
  };
  return {
    backend,
    control,
    add(item: ManagedWorkClaim) {
      claims.set(item.runId, item);
      control.states.set(item.runId, {
        claim: item,
        readiness: item.managedControl?.readiness ?? "unknown",
        pauseRequested: item.managedControl?.pauseRequested ?? false,
        automationControlEpoch: item.managedControl?.automationControlEpoch ?? null,
        providerSessionId: item.managedControl?.providerSessionId ?? null,
      });
    },
    get(runId: ManagedWorkClaim["runId"]) {
      return claims.get(runId);
    },
  };
}

function claim(
  suffix: string,
  readiness: ManagedSessionReadiness,
  state: ManagedWorkClaim["state"] = "running",
) {
  const taskId = TaskIdSchema.parse(`task.${suffix}`);
  return ManagedWorkClaimSchema.parse({
    taskId,
    runId: RunIdSchema.parse(`run.${suffix}`),
    attemptId: AttemptIdSchema.parse(`attempt.${suffix}`),
    actorId: ActorIdSchema.parse(`actor.${suffix}`),
    adapterSessionId: `adapter.${suffix}`,
    sessionId: AgentSessionIdSchema.parse(`session.${suffix}`),
    terminalId: TerminalIdSchema.parse(`terminal.${suffix}`),
    controlLeaseId: `lease.${suffix}`,
    controlEpoch: "1",
    provider: "zap_mock",
    profileId: "profile.zap-mock",
    packet: {
      taskId,
      parentTaskId: null,
      projectId,
      contextId,
      goal: `Worker ${suffix}`,
      contextRefs: [],
      expectedResult: "Synthetic result",
      targetRefs: [],
      sourceBasisRef: "basis.aggregate",
      planRevision: null,
      capabilities: [],
      depth: 0,
      budgets: { maximumTurns: 2, wallTimeMs: 10_000 },
      routing: { preferredProduct: "zap_mock", executionMode: "managed" },
    },
    targetRefs: [],
    modelSelection: selection(),
    managedControl: {
      processEpoch: `process.${suffix}`,
      readiness,
      providerSessionId: `provider-session.${suffix}`,
      providerTurnId: null,
      observationId: `observation.${suffix}`,
      automationControlEpoch: "1",
      pauseRequested: false,
      continuation: "live",
    },
    state,
    processExit: null,
    report: null,
    review: null,
    revision: "1",
  });
}

function selection() {
  return ModelSelectionSchema.parse({
    protocol: "lens-model-selection/1",
    selectionRef: "selection.aggregate",
    policyId: "policy.aggregate",
    policyRevision: "1",
    ruleId: "rule.aggregate",
    overrideRef: null,
    selectionReason: "Synthetic aggregate test",
    overrideReason: null,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    requestedTier: "small",
    requestedEffort: { mode: "unspecified" },
    profileId: "profile.zap-mock",
    productId: "zap_mock",
    productVersion: "synthetic",
    providerId: "zap_mock",
    modelId: "zap-mock/deterministic-v1",
    capabilityId: null,
    effortCapability: { mode: "unsupported" },
    extendedThinking: "unsupported",
    effectiveEffort: { state: "unsupported" },
    actualObservation: null,
    observationMatchesSelection: null,
    application: "future_attempt",
  });
}

function eventFor(claimValue: ManagedWorkClaim, kind: ManagedSessionControlEvent["kind"]) {
  return ManagedSessionControlEventSchema.parse({
    eventId: `event.${claimValue.runId}.${kind}`,
    runId: claimValue.runId,
    actorId: claimValue.actorId,
    sessionId: claimValue.sessionId,
    terminalId: claimValue.terminalId,
    provider: claimValue.provider,
    processEpoch: claimValue.managedControl?.processEpoch,
    sourceSequence: DecimalSchema.parse("1"),
    kind,
    providerSessionId: claimValue.managedControl?.providerSessionId ?? null,
    providerTurnId: null,
    transportCorrelation: null,
    occurredAt: new Date().toISOString(),
  });
}

function access() {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.aggregate"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.aggregate"),
    authorizedProjectIds: [projectId],
  });
}

function result(value: ManagedWorkClaim | undefined): ManagedWorkResult<ManagedWorkClaim> {
  return value === undefined ? unavailable("missing claim") : { ok: true, value };
}
function update(
  claims: Map<string, ManagedWorkClaim>,
  runId: string,
  state: ManagedWorkClaim["state"],
): ManagedWorkResult<ManagedWorkClaim> {
  const current = claims.get(runId);
  if (current === undefined) return unavailable("missing claim");
  const next = ManagedWorkClaimSchema.parse({ ...current, state });
  claims.set(runId, next);
  return { ok: true, value: next };
}
function unavailable(message: string): ManagedWorkResult<never> {
  return { ok: false, error: { code: "unavailable", message } };
}
function nextTurn(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}
