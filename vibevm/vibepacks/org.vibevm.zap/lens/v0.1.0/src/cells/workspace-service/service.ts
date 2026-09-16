/** @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { randomUUID } from "node:crypto";
import {
  WorkspaceAccessContextSchema,
  type WorkspaceAccessContext,
  type WorkspaceClientPort,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceEventIngest,
  type WorkspaceResult,
  type AgentDescriptor,
  type AgentOutputItem,
  type AgentRelationship,
  AgentDescriptorSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";
import { NativeRefSchema } from "../workspace-model/index.ts";
import {
  CoordinatorEventSchema,
  type CoordinatorAdapter,
  type CoordinatorEvent,
} from "../agent-runtime/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type {
  WorkspaceService,
  WorkspaceServiceAuthorization,
  WorkspaceServiceOptions,
} from "./types.ts";
import type { AnnotationCommandRequest } from "../workspace-model/index.ts";
import { WorkspaceSubscriptionHub } from "./subscriptions.ts";
import { workspaceFailure } from "./errors.ts";
import { stateForAgent } from "./helpers.ts";
import { startWorkspace, type Launch, type LaunchActions } from "./launch.ts";
import { controlProject, settleLifecycleEvent } from "./control.ts";
import { dispatchNextChat, observeChatReply, postCoordinatorChat } from "./chat.ts";
import { updateModelPolicy } from "./model-policy.ts";
import { updateWorkspaceState } from "./service-state.ts";
import { commandTerminal, startManagedTerminal } from "./terminal.ts";
import { notifyOwnedAnswer } from "./answer-notice.ts";
import { readWorkspace } from "./reads.ts";
import { commandManagedWork } from "./managed-work.ts";
import {
  ChildObservationSchema,
  CompletedItemSchema,
  DeltaSchema,
  itemType,
} from "./event-model.ts";

export class InProcessWorkspaceService implements WorkspaceService {
  readonly #store: WorkspaceStore;
  readonly #adapters: WorkspaceServiceOptions["adapters"];
  readonly #modelPolicy: WorkspaceServiceOptions["modelPolicy"];
  readonly #coordinatorRouting: WorkspaceServiceOptions["coordinatorRouting"];
  readonly #interactions: WorkspaceServiceOptions["interactions"];
  readonly #terminals: WorkspaceServiceOptions["terminals"];
  readonly #ownedCoordinatorAgents: WorkspaceServiceOptions["ownedCoordinatorAgents"];
  readonly #planning: WorkspaceServiceOptions["planning"];
  readonly #managedWork: WorkspaceServiceOptions["managedWork"];
  readonly #annotations: WorkspaceServiceOptions["annotations"];
  readonly #clock: () => Date;
  readonly #idFactory: (kind: string) => string;
  readonly #launches = new Map<string, Launch>();
  readonly #starting = new Map<string, Promise<WorkspaceResult<WorkspaceCommandResponse>>>();
  readonly #startRequests = new Map<
    string,
    { readonly digest: string; readonly result: WorkspaceResult<WorkspaceCommandResponse> }
  >();
  readonly #subscriptions = new Map<CoordinatorAdapter, () => void>();
  readonly #subscriptionsHub: WorkspaceSubscriptionHub;
  #closed = false;

  constructor(options: WorkspaceServiceOptions) {
    this.#store = options.store;
    this.#adapters = options.adapters;
    this.#modelPolicy = options.modelPolicy;
    this.#coordinatorRouting = options.coordinatorRouting;
    this.#interactions = options.interactions;
    this.#terminals = options.terminals;
    this.#ownedCoordinatorAgents = options.ownedCoordinatorAgents;
    this.#planning = options.planning;
    this.#managedWork = options.managedWork;
    this.#annotations = options.annotations;
    this.#clock = options.clock ?? (() => new Date());
    this.#idFactory = options.idFactory ?? ((kind) => `${kind}.${randomUUID()}`);
    this.#subscriptionsHub = new WorkspaceSubscriptionHub(options.store);
  }

  bind(authorization: WorkspaceServiceAuthorization): WorkspaceClientPort {
    const parsed = WorkspaceAccessContextSchema.safeParse(authorization.access);
    const access = parsed.success ? parsed.data : null;
    const actions = new Set<string>(authorization.allowedActions);
    return {
      read: (request) =>
        access !== null && this.#allowed(actions, "read")
          ? this.#read(access, request)
          : workspaceFailure("unauthorized", "authenticated workspace access is malformed"),
      command: (request) =>
        access === null
          ? Promise.resolve(
              workspaceFailure("unauthorized", "authenticated workspace access is malformed"),
            )
          : this.#command(access, actions, request),
      events: (request) =>
        access !== null && this.#allowed(actions, "events")
          ? this.#store.events(access, request)
          : workspaceFailure("unauthorized", "authenticated workspace access is malformed"),
      subscribe: (request) =>
        access === null
          ? (async function* () {
              await Promise.resolve();
              yield workspaceFailure("unauthorized", "authenticated workspace access is malformed");
            })()
          : this.#subscribe(access, actions, request),
    };
  }

  observe(rawEvent: CoordinatorEvent): void {
    const parsed = CoordinatorEventSchema.safeParse(rawEvent);
    if (!parsed.success || this.#closed) return;
    const launch = this.#launches.get(parsed.data.coordinatorSessionId);
    if (launch === undefined) return;
    if (launch.started && parsed.data.processEpoch !== launch.processEpoch) return;
    if (!launch.started) {
      launch.pending.push(parsed.data);
      return;
    }
    this.#projectEvent(launch, parsed.data);
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    for (const [adapter, unsubscribe] of this.#subscriptions) {
      unsubscribe();
      adapter.close();
    }
    this.#subscriptions.clear();
    this.#subscriptionsHub.close();
  }

  #allowed(actions: Set<string>, action: string): boolean {
    return actions.has(action);
  }

  #read(
    access: WorkspaceAccessContext,
    request: Parameters<WorkspaceClientPort["read"]>[0],
  ): ReturnType<WorkspaceClientPort["read"]> {
    return readWorkspace(
      this.#store,
      this.#planning,
      this.#terminals,
      this.#modelPolicy,
      this.#managedWork,
      this.#annotations,
      access,
      request,
    );
  }

  async #command(
    access: WorkspaceAccessContext,
    actions: Set<string>,
    request: WorkspaceCommandRequest,
  ): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
    if (!this.#allowed(actions, request.operation)) {
      return Promise.resolve(
        workspaceFailure("forbidden", `command ${request.operation} is not granted`),
      );
    }
    if (request.operation === "session.start.v1") return this.#start(access, request);
    if (request.operation === "chat.post.v1") {
      return postCoordinatorChat(this.#launchActions(), access, request);
    }
    if (
      request.operation === "plan.intent.v1" ||
      request.operation === "plan.preview.v1" ||
      request.operation === "plan.apply.v1" ||
      request.operation === "plan.reconcile.v1" ||
      request.operation === "plan.decide.v1"
    ) {
      return this.#planning === undefined
        ? workspaceFailure("unavailable", "shared planning service is not configured")
        : this.#planning.command(access, request);
    }
    if (request.operation === "native-approval.respond.v1") {
      if (this.#interactions === undefined) {
        return workspaceFailure("unavailable", "native interaction service is not configured");
      }
      return this.#interactions.respondToApproval(
        access,
        request,
        this.#interactionDispatch(request.projectId, request.contextId),
      );
    }
    if (
      request.operation === "terminal.start.v1" ||
      request.operation === "terminal.acquire.v1" ||
      request.operation === "terminal.release.v1" ||
      request.operation === "terminal.input.v1" ||
      request.operation === "terminal.resize.v1" ||
      request.operation === "terminal.interrupt.v1" ||
      request.operation === "terminal.stop.v1"
    ) {
      if (request.operation !== "terminal.start.v1")
        return commandTerminal(this.#terminals, access, request);
      return startManagedTerminal(
        this.#terminals,
        this.#store,
        this.#launches,
        this.#clock,
        access,
        request,
      );
    }
    if (request.operation === "model-policy.update.v1") {
      return updateModelPolicy(this.#modelPolicy, access, request);
    }
    if (request.operation.startsWith("managed-work.")) {
      return commandManagedWork(this.#managedWork, this.#store, access, request);
    }
    if (isAnnotationCommand(request)) {
      return this.#annotations === undefined
        ? workspaceFailure("unsupported_operation", "annotation service is not configured")
        : this.#annotations.command(access, request);
    }
    if (
      request.operation === "project.pause.v1" ||
      request.operation === "project.stop.v1" ||
      request.operation === "project.continue.v1"
    ) {
      return controlProject(this.#launchActions(), access, request);
    }
    const result = this.#store.command(access, request);
    if (
      result.ok &&
      this.#interactions !== undefined &&
      (request.operation === "question.answer.v1" || request.operation === "question.amend.v1")
    ) {
      const observed = await this.#interactions.afterQuestionCommand(
        access,
        request,
        result.value,
        this.#interactionDispatch(request.projectId, request.contextId),
      );
      if (!observed.ok) return observed;
      const notified = await notifyOwnedAnswer({
        store: this.#store,
        ownedAgents: this.#ownedCoordinatorAgents,
        launches: this.#launches,
        actions: this.#launchActions(),
        access,
        response: result.value,
      });
      if (!notified.ok) return notified;
    }
    return result;
  }

  #start(
    access: WorkspaceAccessContext,
    request: Extract<WorkspaceCommandRequest, { operation: "session.start.v1" }>,
  ): Promise<WorkspaceResult<WorkspaceCommandResponse>> {
    return startWorkspace(this.#launchActions(), access, request);
  }

  #launchActions(): LaunchActions {
    return {
      store: this.#store,
      adapters: this.#adapters,
      clock: this.#clock,
      idFactory: this.#idFactory,
      launches: this.#launches,
      starting: this.#starting,
      startRequests: this.#startRequests,
      ensureSubscription: (adapter) => {
        this.#ensureSubscription(adapter);
      },
      drainPending: (launch) => {
        this.#drainPending(launch);
      },
      drainChatReplies: (launch) => {
        this.#drainChatReplies(launch);
      },
      dispatchQueued: (launch) => {
        dispatchNextChat(this.#launchActions(), launch).catch(() => undefined);
      },
      coordinatorRouting: this.#coordinatorRouting,
      stopManagedProject: async (access, projectId, contextId) => {
        if (this.#terminals === undefined) return "settled";
        const stopped = await this.#terminals.stopProject(access, projectId, contextId);
        return stopped.ok ? "settled" : "uncertain";
      },
      inspectManagedPause: (access, projectId, contextId) => {
        if (this.#terminals === undefined) return "settled";
        const listed = this.#terminals.list(access, projectId, contextId);
        if (!listed.ok) return "uncertain";
        return listed.value.some((terminal) => terminal.state === "running")
          ? "unsupported"
          : "settled";
      },
      ownedCoordinatorAgents: this.#ownedCoordinatorAgents,
    };
  }

  #ensureSubscription(adapter: CoordinatorAdapter): void {
    if (this.#subscriptions.has(adapter)) return;
    this.#subscriptions.set(
      adapter,
      adapter.subscribe((event) => {
        this.observe(event);
      }),
    );
  }

  #coordinatorActor(launch: Launch): AgentDescriptor {
    const actor = AgentDescriptorSchema.parse({
      actorId: launch.coordinatorActorId,
      sessionId: launch.scope.coordinatorSessionId,
      projectId: launch.projectId,
      contextId: launch.contextId,
      role: "coordinator",
      parentActorId: null,
      displayName: "Coordinator",
      executionMode: "native",
      hostId: launch.scope.hostId,
      nativeRef: launch.descriptor.nativeThreadRef,
      state: stateForAgent(launch.descriptor.state),
      revision: DecimalSchema.parse("1"),
    });
    launch.actors.set(launch.descriptor.nativeThreadRef.value, actor);
    launch.actors.set("__coordinator__", actor);
    return actor;
  }

  #drainPending(launch: Launch): void {
    for (const event of launch.pending.splice(0)) {
      if (event.processEpoch === launch.processEpoch) this.#projectEvent(launch, event);
    }
  }

  #drainChatReplies(launch: Launch): void {
    const pending = launch.pendingChatReplies.splice(0);
    for (const event of pending) {
      if (event.processEpoch !== launch.processEpoch) continue;
      const actor = this.#actorForThread(launch, event.nativeThreadId);
      const observed = observeChatReply(this.#launchActions(), launch, event, actor);
      if (!observed.ok && launch.pendingChatReplies.length < 256) {
        launch.pendingChatReplies.push(event);
      }
    }
  }

  #projectEvent(launch: Launch, event: CoordinatorEvent): void {
    const actor = this.#actorForThread(launch, event.nativeThreadId);
    settleLifecycleEvent(this.#launchActions(), launch, event);
    if (this.#interactions !== undefined) {
      const scope = {
        projectId: launch.projectId,
        contextId: launch.contextId,
        conversationId: launch.session.conversationId,
        originActorId: actor?.actorId ?? null,
        event,
      };
      if (event.kind === "host_request_pending") this.#interactions.observeNativeRequest(scope);
      if (event.kind === "host_request_resolved") this.#interactions.observeResolved(scope);
      if (event.kind === "session_continued") {
        const dispatch = this.#interactionDispatch(launch.projectId, launch.contextId);
        if (dispatch !== null) void this.#interactions.drain(dispatch);
      }
    }
    if (event.kind === "native_child_observed" || event.kind === "native_message_observed")
      this.#child(launch, event);
    if (
      (event.kind === "message_delta" || event.kind === "item_completed") &&
      !this.#output(launch, event, actor)
    )
      return;
    const chatReply = observeChatReply(this.#launchActions(), launch, event, actor);
    if (
      !chatReply.ok &&
      event.kind === "item_completed" &&
      actor?.role === "coordinator" &&
      launch.pendingChatReplies.length < 256 &&
      !launch.pendingChatReplies.some((pending) => pending.sourceEventId === event.sourceEventId)
    ) {
      launch.pendingChatReplies.push(event);
    }
    const history = this.#store.ingestObservedEvent(this.#historyInput(launch, event, actor));
    if (!history.ok || history.value === null) return;
    updateWorkspaceState({ launch, event, actor, store: this.#store, now: this.#clock });
    this.#subscriptionsHub.notify(history.value);
    if (
      (event.kind === "turn_completed" &&
        event.nativeThreadId === launch.descriptor.nativeThreadRef.value) ||
      event.kind === "session_continued"
    ) {
      dispatchNextChat(this.#launchActions(), launch).catch(() => undefined);
    }
  }

  #historyInput(
    launch: Launch,
    event: CoordinatorEvent,
    actor: AgentDescriptor | undefined,
  ): WorkspaceEventIngest {
    return {
      projectId: launch.projectId,
      contextId: launch.contextId,
      kind: `host.${event.kind}`,
      source: "host",
      actorId: actor?.actorId ?? null,
      occurrenceAt: this.#clock().toISOString(),
      sourceEventId: event.sourceEventId,
      correlationId: event.nativeTurnId,
      causationId: null,
      planProvenance: null,
      sourceSequence: null,
      payload: event.data,
    };
  }

  #actorForThread(launch: Launch, threadId: string | null): AgentDescriptor | undefined {
    return threadId === null ? undefined : launch.actors.get(threadId);
  }

  #child(launch: Launch, event: CoordinatorEvent): void {
    const observation = ChildObservationSchema.safeParse(event.data);
    if (!observation.success) return;
    const parent =
      observation.data.fromNativeThreadId === null
        ? undefined
        : (launch.actors.get(observation.data.fromNativeThreadId) ??
          (observation.data.fromNativeThreadId === launch.descriptor.nativeThreadRef.value
            ? this.#coordinatorActor(launch)
            : undefined));
    const existing = launch.actors.get(observation.data.toNativeThreadId);
    const child =
      existing ??
      AgentDescriptorSchema.parse({
        actorId: ActorIdSchema.parse(this.#idFactory("actor")),
        sessionId: launch.scope.coordinatorSessionId,
        projectId: launch.projectId,
        contextId: launch.contextId,
        role: "worker" as const,
        parentActorId:
          observation.data.relationship === "parent" ? (parent?.actorId ?? null) : null,
        displayName: `Native agent ${observation.data.toNativeThreadId}`,
        executionMode: "native" as const,
        hostId: launch.scope.hostId,
        nativeRef: NativeRefSchema.parse({
          namespace: "native.thread",
          value: observation.data.toNativeThreadId,
          incarnation: "1",
        }),
        state: "starting" as const,
        revision: DecimalSchema.parse("1"),
      });
    launch.actors.set(observation.data.toNativeThreadId, child);
    this.#store.upsertAgent(child);
    if (parent !== undefined) {
      const relationship: AgentRelationship = {
        projectId: launch.projectId,
        contextId: launch.contextId,
        fromActorId: parent.actorId,
        toActorId: child.actorId,
        kind: observation.data.relationship,
        provenance: "host",
        sourceEventId: event.sourceEventId,
        observedAt: this.#clock().toISOString(),
      };
      this.#store.recordAgentRelationship(relationship);
    }
  }

  #output(launch: Launch, event: CoordinatorEvent, actor: AgentDescriptor | undefined): boolean {
    if (actor === undefined) return false;
    let body: string | null = null;
    let kind: AgentOutputItem["kind"] = "text";
    if (event.kind === "message_delta") {
      const delta = DeltaSchema.safeParse(event.data);
      if (delta.success) body = delta.data.delta;
    } else {
      const item = CompletedItemSchema.safeParse(event.data);
      if (item.success) {
        body = item.data.text ?? item.data.bodyMarkdown ?? JSON.stringify(event.data);
        kind =
          item.data.type.toLowerCase().includes("tool") ||
          item.data.type.toLowerCase().includes("command")
            ? "tool"
            : "text";
      }
    }
    if (body === null) return false;
    const output = {
      projectId: launch.projectId,
      contextId: launch.contextId,
      actorId: actor.actorId,
      sessionId: actor.sessionId,
      runId: null,
      kind,
      bodyMarkdown: body,
      artifactRefs: [],
      nativeRef: actor.nativeRef,
      occurredAt: this.#clock().toISOString(),
    } satisfies Omit<AgentOutputItem, "sequence">;
    const sourceEventId =
      event.kind === "item_completed" &&
      event.nativeThreadId !== null &&
      event.nativeTurnId !== null &&
      event.nativeItemId !== null
        ? `completed:${event.nativeThreadId}:${event.nativeTurnId}:${event.nativeItemId}:${itemType(event.data)}`
        : event.sourceEventId;
    return this.#store.appendObservedAgentOutput({ sourceEventId, output }).ok;
  }

  #subscribe(
    access: WorkspaceAccessContext,
    actions: Set<string>,
    request: Parameters<WorkspaceClientPort["subscribe"]>[0],
  ): ReturnType<WorkspaceClientPort["subscribe"]> {
    const valid = this.#allowed(actions, "subscribe");
    return valid
      ? this.#subscriptionsHub.subscribe(access, request)
      : (async function* () {
          await Promise.resolve();
          yield workspaceFailure("forbidden", "subscribe action is not granted");
        })();
  }

  #interactionDispatch(projectId: string, contextId: string) {
    const launch = [...this.#launches.values()].find(
      (candidate) => candidate.projectId === projectId && candidate.contextId === contextId,
    );
    if (launch === undefined) return null;
    const execution = this.#store.readProjectExecution(launch.projectId, launch.contextId);
    return {
      projectId: launch.projectId,
      contextId: launch.contextId,
      coordinatorSessionId: launch.scope.coordinatorSessionId,
      adapter: launch.adapter,
      processEpoch: launch.processEpoch,
      executionEnabled: execution.ok && execution.value.state === "running",
    };
  }
}

function isAnnotationCommand(
  request: WorkspaceCommandRequest,
): request is AnnotationCommandRequest {
  return request.operation.startsWith("annotation.");
}

export function createWorkspaceService(options: WorkspaceServiceOptions): WorkspaceService {
  return new InProcessWorkspaceService(options);
}
