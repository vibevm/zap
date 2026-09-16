/** Managed wake and coordinator-free lifecycle proof. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import assert from "node:assert/strict";
import test from "node:test";
import { ActorIdSchema, PrincipalIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  ProjectIdSchema,
  WorkContextIdSchema,
  ClientIdSchema,
  WorkspaceCommandRequestSchema,
} from "../workspace-model/index.ts";
import {
  ManagedWakeClaimSchema,
  ManagedWakeNoticeSchema,
  TrustedProjectRegistrationSchema,
  openWorkspaceStore,
} from "../workspace-store/index.ts";
import { createCoordinatorAdapterRegistry, createWorkspaceService } from "./index.ts";
import { dispatchManagedWake } from "./managed-wake.ts";
import type { ManagedWakeEvent, ManagedWakePort, ManagedWakeTarget } from "./types.ts";

const projectId = ProjectIdSchema.parse("project.managed-wake");
const contextId = WorkContextIdSchema.parse("context.managed-wake");
const workerActor = ActorIdSchema.parse("actor.managed-worker");
const target: ManagedWakeTarget = {
  actorId: workerActor,
  runId: "run.managed-wake",
  attemptId: "attempt.managed-wake",
  adapterSessionId: "adapter.managed-wake",
  processEpoch: "epoch.managed-wake",
  leaseId: "lease.managed-wake",
};

class WakeFixture implements ManagedWakePort {
  offers = 0;
  target: ManagedWakeTarget = { ...target };
  observation: "host_accepted" | "uncertain" | "busy" | "refused" = "host_accepted";
  #listener: ((event: ManagedWakeEvent) => void) | null = null;
  pauseObservation: "settled" | "requested" | "unsupported" | "uncertain" = "settled";
  continueObservation: "settled" | "unsupported" | "uncertain" = "settled";

  pauseOwned(): Promise<"settled" | "requested" | "unsupported" | "uncertain"> {
    return Promise.resolve(this.pauseObservation);
  }
  stopOwned(): Promise<"settled" | "uncertain"> {
    return Promise.resolve("settled");
  }
  continueOwned(): Promise<"settled" | "unsupported" | "uncertain"> {
    this.target = {
      ...this.target,
      processEpoch: "epoch.managed-wake.2",
      leaseId: "lease.managed-wake.2",
    };
    return Promise.resolve(this.continueObservation);
  }
  resolveRecipient(): Promise<ManagedWakeTarget> {
    return Promise.resolve(this.target);
  }
  offerWake(): Promise<"host_accepted" | "uncertain" | "busy" | "refused"> {
    this.offers += 1;
    return Promise.resolve(this.observation);
  }
  subscribe(listener: (event: ManagedWakeEvent) => void): () => void {
    this.#listener = listener;
    return () => {
      this.#listener = null;
    };
  }
  emit(event: ManagedWakeEvent): void {
    this.#listener?.(event);
  }
}

test("managed-only project lifecycle accepts null session and retains wake while paused", async () => {
  const opened = openWorkspaceStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const store = opened.value;
  const registration = TrustedProjectRegistrationSchema.parse({
    registrationId: "registration.managed-wake",
    projectId,
    displayName: "Managed wake",
    repositoryRootRefs: ["repository.managed-wake"],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId,
      displayName: "Managed context",
      workspaceRef: "workspace.managed-wake",
      branchLabel: "main",
      revisionBinding: "revision.managed-wake",
      planning: { state: "unavailable", reason: "Synthetic fixture" },
      coordinatorConversationId: "conversation.managed-wake",
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.managed-wake",
        label: "Managed wake",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: { cwd: "C:\\fixtures\\managed-wake", launchProfileRef: "profile.managed-wake" },
  });
  assert.equal(store.registerProject(registration).ok, true);
  const access = {
    principalId: PrincipalIdSchema.parse("principal.managed-wake"),
    actorId: ActorIdSchema.parse("actor.owner-managed-wake"),
    clientId: ClientIdSchema.parse("client.managed-wake"),
    authorizedProjectIds: [projectId],
  };
  const wake = new WakeFixture();
  const service = createWorkspaceService({
    store,
    adapters: createCoordinatorAdapterRegistry([]),
    managedWake: wake,
  });
  const client = service.bind({
    access,
    allowedActions: ["project.pause.v1", "project.continue.v1", "project.stop.v1", "read"],
  });
  const paused = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "project.pause.v1",
      clientRequestId: "request.managed-pause",
      projectId,
      contextId,
      sessionId: null,
      expectedRevision: DecimalSchema.parse("1"),
      reasonMarkdown: "Pause owned managed work",
    }),
  );
  assert.equal(paused.ok, true);
  if (!paused.ok || paused.value.operation !== "project.pause.v1") return;
  assert.equal(paused.value.execution.state, "paused");
  wake.target = { ...wake.target, processEpoch: null, leaseId: null };
  const notice = ManagedWakeNoticeSchema.parse({
    wakeId: "request.managed-notice",
    projectId,
    contextId,
    actorId: workerActor,
    runId: target.runId,
    attemptId: target.attemptId,
    adapterSessionId: target.adapterSessionId,
    kind: "answer",
    questionGroupId: "question.managed-wake",
    answerVersionId: "answer.managed-wake",
    bodyMarkdown: "The answer is ready.",
    sourceEventId: "question:managed-wake:answer.managed-wake",
    state: "queued",
    updatedAt: new Date().toISOString(),
  });
  assert.equal(store.queueManagedWake(notice).ok, true);
  assert.equal(
    ManagedWakeClaimSchema.safeParse({
      wakeId: notice.wakeId,
      projectId: notice.projectId,
      contextId: notice.contextId,
      actorId: notice.actorId,
      runId: notice.runId,
      attemptId: notice.attemptId,
      adapterSessionId: notice.adapterSessionId,
      processEpoch: target.processEpoch,
      leaseId: target.leaseId,
    }).success,
    true,
  );
  const retained = await dispatchManagedWake({
    store,
    managedWake: wake,
    projectId,
    contextId,
    actorId: workerActor,
  });
  assert.deepEqual(retained, { ok: true, value: null });
  assert.equal(wake.offers, 0);
  const continued = await client.command(
    WorkspaceCommandRequestSchema.parse({
      operation: "project.continue.v1",
      clientRequestId: "request.managed-continue",
      projectId,
      contextId,
      sessionId: null,
      expectedRevision: paused.value.execution.revision,
      reasonMarkdown: "Continue owned managed work",
    }),
  );
  assert.equal(continued.ok, true);
  assert.equal(wake.offers, 1);
  const drained = store.nextManagedWake(projectId, contextId, workerActor);
  assert.equal(drained.ok, true);
  if (drained.ok) assert.equal(drained.value, null);
  service.close();
  store.close();
});

test("uncertain managed wake remains explicit and is never replayed", async () => {
  const opened = openWorkspaceStore({ databasePath: ":memory:" });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const store = opened.value;
  const registration = TrustedProjectRegistrationSchema.parse({
    registrationId: "registration.managed-uncertain",
    projectId: "project.managed-uncertain",
    displayName: "Managed uncertain",
    repositoryRootRefs: ["repository.managed-uncertain"],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId: "context.managed-uncertain",
      displayName: "Managed context",
      workspaceRef: "workspace.managed-uncertain",
      branchLabel: "main",
      revisionBinding: "revision.managed-uncertain",
      planning: { state: "unavailable", reason: "Synthetic fixture" },
      coordinatorConversationId: "conversation.managed-uncertain",
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.managed-uncertain",
        label: "Managed uncertain",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: {
      cwd: "C:\\fixtures\\managed-uncertain",
      launchProfileRef: "profile.managed-uncertain",
    },
  });
  assert.equal(store.registerProject(registration).ok, true);
  const wake = new WakeFixture();
  wake.observation = "uncertain";
  const notice = ManagedWakeNoticeSchema.parse({
    wakeId: "request.managed-uncertain",
    projectId: "project.managed-uncertain",
    contextId: "context.managed-uncertain",
    actorId: workerActor,
    runId: target.runId,
    attemptId: target.attemptId,
    adapterSessionId: target.adapterSessionId,
    kind: "cancel",
    questionGroupId: "question.managed-uncertain",
    answerVersionId: null,
    bodyMarkdown: "Cancel this question.",
    sourceEventId: "question:managed-uncertain:cancel",
    state: "queued",
    updatedAt: new Date().toISOString(),
  });
  assert.equal(store.queueManagedWake(notice).ok, true);
  assert.equal(
    ManagedWakeClaimSchema.safeParse({
      wakeId: notice.wakeId,
      projectId: notice.projectId,
      contextId: notice.contextId,
      actorId: notice.actorId,
      runId: notice.runId,
      attemptId: notice.attemptId,
      adapterSessionId: notice.adapterSessionId,
      processEpoch: target.processEpoch,
      leaseId: target.leaseId,
    }).success,
    true,
  );
  const sent = await dispatchManagedWake({
    store,
    managedWake: wake,
    projectId: notice.projectId,
    contextId: notice.contextId,
    actorId: notice.actorId,
  });
  assert.equal(sent.ok, true, sent.ok ? "" : sent.error.message);
  assert.equal(wake.offers, 1);
  const replay = await dispatchManagedWake({
    store,
    managedWake: wake,
    projectId: notice.projectId,
    contextId: notice.contextId,
    actorId: notice.actorId,
  });
  assert.deepEqual(replay, { ok: true, value: null });
  assert.equal(wake.offers, 1);
  store.close();
});
