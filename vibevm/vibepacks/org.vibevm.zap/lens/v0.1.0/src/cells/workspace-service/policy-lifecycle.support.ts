/** @scope spec://org.vibevm.zap/lens/PROP-008#verification */
import { resolve } from "node:path";
import type { AgentRuntimeResult, CoordinatorAdapter } from "../agent-runtime/index.ts";
import {
  createCodexCoordinatorAdapter,
  CodexWireMessageSchema,
  type CodexCoordinatorProfile,
  type CodexProcessFactory,
  type CodexProcessProfile,
  type CodexProcessResult,
  type CodexRpcProcess,
  type CodexWireMessage,
} from "../codex-coordinator/index.ts";
import { ActorIdSchema, PrincipalIdSchema, type JsonValue } from "../protocol/index.ts";
import {
  ClientIdSchema,
  AgentSessionIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import { TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";

export const policyCwd = resolve("fixture-policy-workspace");
export const projectId = ProjectIdSchema.parse("project.policy-lifecycle");
export const contextId = WorkContextIdSchema.parse("context.policy-lifecycle");
export const sessionId = AgentSessionIdSchema.parse("session.policy-lifecycle-1");

export interface RecordedRequest {
  readonly method: string;
  readonly params: JsonValue;
}

export class RecordingProcess implements CodexRpcProcess {
  readonly requests: RecordedRequest[] = [];
  readonly notifications: RecordedRequest[] = [];
  readonly epoch: string;
  readonly #reportedModel: string | null;
  terminateCalls = 0;
  readonly #messages = new Set<(message: CodexWireMessage) => void>();
  readonly #exits = new Set<(exit: { code: number | null; diagnostic: string }) => void>();
  #turns = 0;

  constructor(epoch: string, reportedModel: string | null = null) {
    this.epoch = epoch;
    this.#reportedModel = reportedModel;
  }

  request(method: string, params: JsonValue): Promise<CodexProcessResult<JsonValue>> {
    this.requests.push({ method, params });
    if (method === "thread/start") {
      return Promise.resolve({
        ok: true,
        value: { thread: thread("thread-policy", policyCwd, this.#reportedModel) },
      });
    }
    if (method === "thread/resume") {
      return Promise.resolve({
        ok: true,
        value: {
          thread: thread(
            stringField(params, "threadId"),
            stringField(params, "cwd"),
            this.#reportedModel,
          ),
        },
      });
    }
    if (method === "thread/read") {
      return Promise.resolve({
        ok: true,
        value: { thread: thread("thread-policy", policyCwd, this.#reportedModel) },
      });
    }
    if (method === "turn/start") {
      this.#turns += 1;
      return Promise.resolve({
        ok: true,
        value: {
          turn: turn(this.#turns === 1 ? "turn-bootstrap" : `turn-chat-${String(this.#turns)}`),
        },
      });
    }
    if (method === "turn/interrupt") return Promise.resolve({ ok: true, value: {} });
    return Promise.resolve({ ok: true, value: {} });
  }

  notify(method: string, params: JsonValue): CodexProcessResult<void> {
    this.notifications.push({ method, params });
    return { ok: true, value: undefined };
  }

  respond(): CodexProcessResult<void> {
    return { ok: true, value: undefined };
  }

  subscribe(listener: (message: CodexWireMessage) => void): () => void {
    this.#messages.add(listener);
    return () => this.#messages.delete(listener);
  }

  onExit(listener: (exit: { code: number | null; diagnostic: string }) => void): () => void {
    this.#exits.add(listener);
    return () => this.#exits.delete(listener);
  }

  terminate(): Promise<CodexProcessResult<{ code: number | null }>> {
    this.terminateCalls += 1;
    for (const listener of this.#exits) listener({ code: 0, diagnostic: "" });
    return Promise.resolve({ ok: true, value: { code: 0 } });
  }

  emitTurnCompleted(turnId: string): void {
    this.emit({
      method: "turn/completed",
      params: { threadId: "thread-policy", turn: completedTurn(turnId) },
    });
  }

  emit(value: unknown): void {
    const message = CodexWireMessageSchema.parse(value);
    for (const listener of this.#messages) listener(message);
  }

  close(): void {
    this.#messages.clear();
    this.#exits.clear();
  }
}

export class RecordingFactory implements CodexProcessFactory {
  readonly profiles: CodexProcessProfile[] = [];
  readonly #processes: RecordingProcess[];

  constructor(processes: readonly RecordingProcess[]) {
    this.#processes = [...processes];
  }

  start(profile: CodexProcessProfile): Promise<CodexProcessResult<CodexRpcProcess>> {
    this.profiles.push(profile);
    const process = this.#processes.shift();
    return Promise.resolve(
      process === undefined
        ? { ok: false, error: { kind: "spawn_failed", message: "No recording process" } }
        : { ok: true, value: process },
    );
  }
}

export function codexAdapter(
  factory: RecordingFactory,
  selectedDefault: { readonly model: string; readonly effort: "low" | "medium" },
): CoordinatorAdapter {
  const created = createCodexCoordinatorAdapter({
    profiles: [legacyProfile(), selectedProfile(selectedDefault)],
    processFactory: factory,
  });
  if (!created.ok) throw new Error(created.error.message);
  return created.value;
}

export function registration() {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: "registration.policy-lifecycle",
    projectId,
    displayName: "Policy lifecycle",
    repositoryRootRefs: ["repository.policy-lifecycle"],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId,
      displayName: "Policy lifecycle context",
      workspaceRef: "workspace.policy-lifecycle",
      branchLabel: "main",
      revisionBinding: "revision.policy-lifecycle",
      planning: { state: "unavailable", reason: "Synthetic fixture" },
      coordinatorConversationId: "conversation.policy-lifecycle",
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex-default",
        label: "Legacy protected launch",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: { cwd: policyCwd, launchProfileRef: "legacy.protected" },
  });
}

export function access(clientId: string): WorkspaceAccessContext {
  return {
    principalId: PrincipalIdSchema.parse("principal.policy-lifecycle"),
    actorId: ActorIdSchema.parse("actor.policy-lifecycle"),
    clientId: ClientIdSchema.parse(clientId),
    authorizedProjectIds: [projectId],
  };
}

export function host(adapter: CoordinatorAdapter, openedProfiles: string[]) {
  return {
    hostId: ExecutionHostIdSchema.parse("host.policy-lifecycle"),
    profileIds: ["legacy.protected", "codex.big"],
    openCoordinator: (profileRef: string) => {
      openedProfiles.push(profileRef);
      const result: AgentRuntimeResult<CoordinatorAdapter> = { ok: true, value: adapter };
      return Promise.resolve(result);
    },
  };
}

function legacyProfile(): CodexCoordinatorProfile {
  return {
    profileId: "legacy.protected",
    executablePath: resolve("legacy-codex.exe"),
    requestTimeoutMs: 5_000,
    model: "gpt-legacy-default",
    effort: "low",
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "legacy-protected",
  };
}

function selectedProfile(input: {
  readonly model: string;
  readonly effort: "low" | "medium";
}): CodexCoordinatorProfile {
  return {
    profileId: "codex.big",
    executablePath: resolve("selected-codex.exe"),
    requestTimeoutMs: 5_000,
    model: input.model,
    effort: input.effort,
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "policy-selected",
  };
}

function stringField(value: JsonValue, key: string): string {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`Expected object parameters for ${key}`);
  }
  const field = value[key];
  if (typeof field !== "string") throw new Error(`Expected string parameter ${key}`);
  return field;
}

function turn(id: string): JsonValue {
  return { id, status: "inProgress", items: [], error: null };
}

function completedTurn(id: string): JsonValue {
  return { id, status: "completed", items: [], error: null };
}

function thread(id: string, cwd: string, reportedModel: string | null): JsonValue {
  const base = {
    id,
    sessionId: id,
    cwd,
    status: { type: "idle" },
    turns: [],
    parentThreadId: null,
    canAcceptDirectInput: true,
    cliVersion: "0.152.1",
    ephemeral: false,
    modelProvider: "openai",
    preview: "policy fixture",
    createdAt: 1,
    updatedAt: 1,
  };
  return reportedModel === null ? base : { ...base, model: reportedModel };
}
