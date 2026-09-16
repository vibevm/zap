/** Shared managed-work integration fixture. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { z } from "zod";
import { openBroker } from "../broker/index.ts";
import { createAgentHttpClient } from "../http/index.ts";
import { ModelSelectionSchema } from "../model-policy/index.ts";
import {
  CredentialSchema,
  EnrollPrincipalInputSchema,
  PrincipalIdSchema,
  type Credential,
} from "../protocol/index.ts";
import type {
  ManagedParentPort,
  ManagedSelectionPort,
  WorkAttachmentPort,
} from "../managed-work/index.ts";
import { ManagedAgentProfileSchema } from "../managed-work/index.ts";
import {
  ManagedTerminalKernel,
  type ManagedTerminalFactory,
  type ManagedTerminalProcess,
} from "../managed-terminal/index.ts";
import { createManagedTerminalService } from "../managed-terminal-service/index.ts";
import { openManagedTerminalOutputStore } from "../managed-terminal-store/index.ts";
import {
  ExecutionHostIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import {
  createAnnotationWorkAttachmentPort,
  openAnnotationStore,
} from "../workspace-annotations/index.ts";

export function enroll(databasePath: string): Credential {
  const broker = openBroker({ databasePath });
  assert.ok(broker.ok);
  const enrolled = broker.value.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: ["workspace.managed"],
      conversationIds: ["conversation.managed"],
      capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack"],
    }),
  );
  broker.value.close();
  if (!enrolled.ok) throw new Error(enrolled.error.message);
  return enrolled.value.principalToken;
}

export function scriptedTerminalService() {
  let pid = 1000;
  const launches: string[][] = [];
  const factory: ManagedTerminalFactory = {
    spawn(spec) {
      launches.push([...spec.args]);
      let output: ((data: string) => void) | undefined;
      let exited: ((code: number | null) => void) | undefined;
      let exitTimer: ReturnType<typeof setTimeout> | undefined;
      const process: ManagedTerminalProcess = {
        processId: ++pid,
        onData(listener) {
          output = listener;
          return () => undefined;
        },
        onExit(listener) {
          exited = listener;
          exitTimer = setTimeout(() => {
            output?.("MANAGED_PTY_READY\r\n");
            exited?.(0);
          }, 250);
          return () => undefined;
        },
        write(data) {
          output?.(`INPUT:${data}`);
        },
        resize() {},
        interrupt() {},
        stop() {
          if (exitTimer !== undefined) clearTimeout(exitTimer);
          exited?.(0);
        },
      };
      return Promise.resolve({ ok: true as const, value: process });
    },
  };
  const output = openManagedTerminalOutputStore(":memory:");
  return {
    service: createManagedTerminalService(new ManagedTerminalKernel(factory), [], output),
    launches,
  };
}

export function selectionPort(): ManagedSelectionPort {
  return {
    resolve: (_access, _request, profiles) => {
      const profile = profiles[0];
      return Promise.resolve(
        profile === undefined
          ? {
              ok: false as const,
              error: { code: "unavailable" as const, message: "fixture profile is unavailable" },
            }
          : {
              ok: true as const,
              value: {
                profile,
                modelSelection: selection(),
                executionSelection: null,
              },
            },
      );
    },
  };
}

function selection() {
  return ModelSelectionSchema.parse({
    protocol: "lens-model-selection/1",
    selectionRef: "selection.managed.synthetic",
    policyId: "policy.managed.synthetic",
    policyRevision: "1",
    ruleId: "rule.managed.synthetic",
    overrideRef: null,
    selectionReason: "synthetic managed selection",
    overrideReason: null,
    purpose: "development_implementation",
    taskClass: "integration",
    role: "worker",
    executionMode: "managed",
    invocationScope: "managed_agent",
    requestedTier: "small",
    requestedEffort: { mode: "explicit", value: "low" },
    profileId: "profile.codex.managed",
    productId: "codex",
    productVersion: "synthetic",
    providerId: "codex",
    modelId: "selection-model",
    capabilityId: null,
    effortCapability: { mode: "configurable", allowedValues: ["low"], defaultValue: "low" },
    extendedThinking: "unsupported",
    effectiveEffort: { state: "explicit", value: "low" },
    actualObservation: null,
    observationMatchesSelection: null,
    application: "future_attempt",
  });
}

export function parentPort(): ManagedParentPort {
  return {
    validate: (_access, request) => ({
      ok: true,
      value: {
        parentTaskId: request.parentTaskId,
        parentRunId: request.parentRunId,
        parentActorId: request.projectedParentActorId,
        depth: request.depth,
      },
    }),
  };
}

export function attachments(acknowledged: string[]): WorkAttachmentPort {
  return {
    prepareBeforeWork: () =>
      Promise.resolve({
        ok: true as const,
        value: {
          state: "ready" as const,
          instructions: [
            {
              attachmentId: "attachment.managed.note",
              version: "1",
              bodyMarkdown: "Exact deferred instruction.",
            },
          ],
        },
      }),
    acknowledge: (input) => {
      acknowledged.push(`${input.attemptId}:${input.attachmentId}:${input.version}`);
      return Promise.resolve({ ok: true as const, value: null });
    },
  };
}

export function foundationAgent(address: { host: string; port: number }, token: string) {
  return createAgentHttpClient({
    baseUrl: new URL(`http://${address.host}:${String(address.port)}`),
    principalToken: CredentialSchema.parse(token),
  });
}

export function runtimeConfig(root: string) {
  return {
    version: 1 as const,
    state: { databasePath: join(root, "runtime.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "managed",
      pairingToken: "pairing.managed.runtime.synthetic.0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
    },
    agentGateway: {
      databasePath: join(root, "broker.sqlite"),
      host: "127.0.0.1" as const,
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.managed.runtime.secondary.0001",
      scopes: [],
    },
    profiles: [
      {
        profileId: "profile.codex.native",
        executablePath: "C:/placeholder/codex.exe",
        requestTimeoutMs: 30_000,
        model: "synthetic",
        effort: "low",
        approvalPolicy: "on-request",
        sandbox: "workspace-write",
        personality: "pragmatic",
        serviceName: "zap-wayfinder",
      },
    ],
    projects: [projectRegistration()],
  };
}

export function projectRegistration() {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: "request.project.managed.runtime",
    projectId: "project.managed",
    displayName: "Managed runtime",
    repositoryRootRefs: ["repository.managed"],
    actions: { startCoordinator: { state: "available" as const } },
    context: {
      contextId: "context.managed",
      displayName: "Managed context",
      workspaceRef: "workspace.managed",
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable" as const, reason: "fixture" },
      coordinatorConversationId: "conversation.managed",
      brokerScope: { workspaceId: "workspace.managed", conversationId: "conversation.managed" },
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex.native",
        label: "Synthetic",
        interactionKind: "structured" as const,
        availability: { state: "available" as const },
      },
    ],
    protected: { cwd: "C:/Windows", launchProfileRef: "profile.codex.native" },
  });
}

export function fakeHost() {
  return {
    hostId: ExecutionHostIdSchema.parse("host.synthetic"),
    profileIds: ["profile.codex.native"],
    openCoordinator: () =>
      Promise.resolve({
        ok: false as const,
        error: { code: "unsupported" as const, message: "synthetic", retry: "never" as const },
      }),
  };
}

export function createRequest() {
  return WorkspaceCommandRequestSchema.parse({
    operation: "managed-work.create.v1" as const,
    clientRequestId: "request.managed.create",
    projectId: ProjectIdSchema.parse("project.managed"),
    contextId: WorkContextIdSchema.parse("context.managed"),
    selection: {
      mode: "profile_override",
      profileId: "profile.codex.managed",
      reasonMarkdown: "Exercise the exact synthetic managed profile.",
    },
    goal: "Run a scripted worker",
    expectedResult: "Typed report",
    targetRefs: [
      {
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        domain: "work_task" as const,
        ref: "task.managed",
      },
    ],
    contextRefs: [],
    parentTaskId: null,
    parentRunId: null,
    sourceBasisRef: "plan.managed",
    planRevision: null,
    depth: 0,
    budgets: { maximumTurns: 2, wallTimeMs: 10_000 },
  });
}

export function managedProfile(root: string) {
  return ManagedAgentProfileSchema.parse({
    profileId: "profile.codex.managed",
    projectId: "project.managed",
    contextId: "context.managed",
    provider: "codex",
    executablePath: "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
    cwd: "C:/Windows",
    modelId: "profile-model",
    effort: "high",
    environmentRef: null,
    mcpConfigPath: join(root, "mcp", "worker.json"),
    capabilities: {
      provider: "codex",
      observedVersion: "synthetic",
      installed: true,
      launchable: true,
      authenticated: "not_observed",
      structuredConversation: "unsupported",
      nativeChildren: "unsupported",
      nativeQuestions: "supported",
      nativeApprovals: "unsupported",
      interrupt: "supported",
      resume: "unsupported",
      interactiveTerminal: "supported",
      evidence: ["synthetic PTY factory"],
    },
  });
}

export function managedAnnotationFixture(root: string) {
  const store = openAnnotationStore({
    databasePath: join(root, "annotations.sqlite"),
    idFactory: (kind) => `${kind}.managed-self`,
  });
  assert.equal(store.ok, true);
  const access = WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.managed.annotation"),
    actorId: null,
    clientId: ClientIdSchema.parse("client.managed.annotation"),
    authorizedProjectIds: [ProjectIdSchema.parse("project.managed")],
  });
  const attachments = createAnnotationWorkAttachmentPort({
    store: store.value,
    resolver: {
      resolve: () =>
        Promise.resolve({
          state: "present" as const,
          snapshot: {
            basisRef: "basis.managed.self",
            capturedAt: "2026-09-16T00:00:00.000Z",
            value: { state: "prepared" },
          },
        }),
    },
    idFactory: (kind) => `${kind}.managed-self`,
    clock: () => new Date("2026-09-16T00:01:00.000Z"),
  });
  return { store: store.value, access, attachments };
}

export function managedCredential(path: string) {
  const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
  return CredentialSchema.parse(
    z
      .object({
        protocol: z.literal("lens/1"),
        agent: z.object({ principalToken: z.string() }).strict(),
      })
      .strict()
      .parse(raw).agent.principalToken,
  );
}
