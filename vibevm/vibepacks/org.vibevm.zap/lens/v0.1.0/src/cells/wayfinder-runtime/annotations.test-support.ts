/** Deterministic notes runtime fixtures. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { join } from "node:path";

import {
  PlanOperationResultSchema,
  QuicklensRefSchema,
  QuicklensSnapshotSchema,
  type QuicklensSnapshot,
} from "../quicklens-model/index.ts";
import {
  createWorkspaceHttpConnection,
  type WorkspaceHttpConnection,
} from "../workspace-client/index.ts";
import { WorkspaceCommandResponseSchema } from "../workspace-model/index.ts";
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import { TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import type { WayfinderRuntime } from "./index.ts";

export function runtimeConfig(root: string) {
  return {
    version: 1 as const,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1" as const,
      port: 0,
      namespace: "annotations",
      pairingToken: randomBytes(32).toString("base64url"),
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://127.0.0.1:4174"],
    },
    agentGateway: {
      databasePath: join(root, "agent.sqlite"),
      host: "127.0.0.1" as const,
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.annotations.synthetic.0001",
      scopes: [],
    },
    profiles: [
      {
        profileId: "profile.annotations.fixture",
        executablePath: "C:/fixture/no-provider.exe",
        requestTimeoutMs: 30_000,
        model: "no-model",
        effort: "low",
        approvalPolicy: "on-request" as const,
        sandbox: "workspace-write" as const,
        personality: "pragmatic" as const,
        serviceName: "annotations-test",
      },
    ],
    productProviders: [
      {
        profileId: "profile.annotations.fixture",
        provider: "codex" as const,
        displayName: "Fixture profile",
        modelId: "no-model",
        interactionKind: "structured" as const,
        installed: true,
        configured: true,
        authenticated: "not_observed" as const,
        launchable: true,
        evidence: ["No provider process is started by this notes test"],
      },
    ],
    projects: [],
    modelPolicies: [],
  };
}

export function httpClient(
  runtime: WayfinderRuntime,
  receipt: { host: string; port: number; basePath: string },
): WorkspaceHttpConnection {
  const ticket = runtime.issuePairingTicket();
  assert.equal(ticket.ok, true);
  const connection = createWorkspaceHttpConnection({
    baseUrl: `http://${receipt.host}:${String(receipt.port)}${receipt.basePath}`,
    origin: "http://127.0.0.1:4174",
    pairingToken: ticket.value.ticket,
  });
  if (connection === null) assert.fail();
  return connection;
}

export function trustedRegistration() {
  return TrustedProjectRegistrationSchema.parse({
    registrationId: "request.project.annotations.bindings",
    projectId: "project.annotations.bindings",
    displayName: "Annotation bindings",
    repositoryRootRefs: ["repository.annotations.bindings"],
    actions: { startCoordinator: { state: "available" as const } },
    context: {
      contextId: "context.annotations.bindings",
      displayName: "Annotation context",
      workspaceRef: "workspace.annotations.bindings",
      branchLabel: null,
      revisionBinding: null,
      planning: { state: "unavailable" as const, reason: "synthetic planning source" },
      coordinatorConversationId: "conversation.annotations.bindings",
      brokerScope: {
        workspaceId: "workspace.annotations.bindings",
        conversationId: "conversation.annotations.bindings",
      },
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.annotations.fixture",
        label: "Synthetic",
        interactionKind: "structured" as const,
        availability: { state: "available" as const },
      },
    ],
    protected: { cwd: "C:/fixture", launchProfileRef: "profile.annotations.fixture" },
  });
}

export function planningSnapshot(
  includeTask: boolean,
  phase: "ready" | "partial" | "stale" = "ready",
): QuicklensSnapshot {
  return QuicklensSnapshotSchema.parse({
    sourceMode: "live",
    sourceLabel: "Synthetic authoritative planning source",
    phase,
    phaseDetail: phase === "ready" ? null : "Synthetic incomplete planning source",
    capturedAt: includeTask ? "2026-09-16T00:00:00.000Z" : "2026-09-16T00:02:00.000Z",
    revision: includeTask ? "1" : "2",
    objects: includeTask
      ? [
          {
            ref: "object.annotations.task",
            category: "task",
            semanticType: "work",
            title: "Annotation task",
            purpose: null,
            acceptance: null,
            status: { code: "planned", label: "Planned", tone: "neutral" },
            metrics: {
              complexity: { state: "unknown", reason: "Not assessed" },
              difficulty: { state: "unknown", reason: "Not assessed" },
              effort: { state: "unknown", reason: "Not assessed" },
              waiting: { state: "unknown", reason: "Not assessed" },
              uncertainty: { state: "unknown", reason: "Not assessed" },
            },
            provenance: [],
            position: null,
          },
        ]
      : [],
    relationships: [],
    questions: [],
    agentTargets: [],
    plan: {
      outcomeLabel: "Annotation outcome",
      strategyLabel: "Annotation strategy",
      planLabel: "Annotation plan",
      basis: {
        storeRef: "store.annotations",
        baseRef: "base.annotations",
        revision: includeTask ? "1" : "2",
        sourceBasisRef: includeTask ? "basis.annotations.1" : "basis.annotations.2",
      },
      state: "current",
      detail: null,
      decision: null,
      actions: {
        propose: { enabled: true, reason: null },
        preview: { enabled: true, reason: null },
        apply: { enabled: true, reason: null },
        reconcile: { enabled: true, reason: null },
        decide: { enabled: false, reason: "No decision is held" },
      },
    },
  });
}

export function planningFeature(snapshot: () => QuicklensSnapshot): WorkspacePlanningFeature {
  return {
    snapshot: () =>
      Promise.resolve({
        ok: true,
        value: {
          operation: "project.snapshot.v1",
          snapshot: { state: "ready", snapshot: snapshot() },
        },
      }),
    command: (_access, request) =>
      Promise.resolve({
        ok: true,
        value: WorkspaceCommandResponseSchema.parse({
          operation: request.operation,
          result: PlanOperationResultSchema.parse({
            operationRef: QuicklensRefSchema.parse("operation.annotations.restore"),
            previewRef: null,
            preview: null,
            state: "queued",
            message: "Queued fixture restore intent",
            nextBasis: null,
          }),
        }),
      }),
    agent: () => ({
      ok: false,
      error: { code: "unavailable", message: "synthetic agent planning is unavailable" },
    }),
    close() {},
  };
}

export function unavailableNotifications() {
  return {
    enqueue: () =>
      Promise.resolve({
        ok: false as const,
        error: { code: "unavailable" as const, message: "synthetic notification is unavailable" },
      }),
  };
}

export function deterministicIds(): (kind: string) => string {
  let sequence = 0;
  return (kind) => `${kind}.annotations.${String(++sequence)}`;
}
