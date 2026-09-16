/** Real-product ZapMock managed integration fixture. @scope spec://org.vibevm.zap/lens/PROP-013#verification */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import { z } from "zod";
import { ZapMockScenarioFileSchema } from "../mock-agent/index.ts";

const mockEntry = fileURLToPath(new URL("../../zap-mock-agent.ts", import.meta.url));
const mcpEntry = fileURLToPath(new URL("../../mcp.ts", import.meta.url));

export const MOCK_PROJECT_ID = "project.mock.product";
export const MOCK_CONTEXT_ID = "context.mock.product";
export const MOCK_PROFILE_ID = "profile.zap-mock.product";

const BehaviorSchema = z
  .object({
    protocol: z.literal("zap-mock-behavior/1"),
    kind: z.literal("managed_product"),
    scenarioId: z.string().min(3).max(160),
    runnerId: z.literal("wayfinder.mock-managed"),
    tags: z.array(z.string().min(1).max(80)).min(1).max(32),
    coverage: z.array(z.string().min(3).max(160)).min(1).max(64),
    inputs: z
      .object({
        answerOptionIndex: z.number().int().min(0).max(31),
        pauseBeforeAnswer: z.literal(true),
        answerNote: z.string().min(1).max(8_000),
      })
      .strict(),
    scenarioFile: ZapMockScenarioFileSchema,
    expected: z
      .object({
        provider: z.literal("zap_mock"),
        modelId: z.literal("zap-mock/deterministic-v1"),
        questionPrompt: z.string().min(1),
        pausedExecutionState: z.literal("paused"),
        heldWorkStateMustNotBe: z.literal("reported"),
        reportIncludes: z.string().min(1),
        reviewedWorkState: z.literal("accepted"),
        zeroLlmInference: z.literal(true),
      })
      .strict(),
  })
  .strict();

export function writeMockProductScenario(root: string, seedOverride?: string) {
  const fixturePath = fileURLToPath(new URL("./mock-managed.simulation.json", import.meta.url));
  const fixture = BehaviorSchema.parse(JSON.parse(readFileSync(fixturePath, "utf8")));
  const path = join(root, "mock-scenario.json");
  const scenarioFile =
    seedOverride === undefined
      ? fixture.scenarioFile
      : ZapMockScenarioFileSchema.parse({ ...fixture.scenarioFile, seed: seedOverride });
  writeFileSync(path, JSON.stringify(scenarioFile), "utf8");
  return {
    path,
    expected: fixture.expected,
    inputs: fixture.inputs,
    seed: scenarioFile.seed,
    scenarioId: fixture.scenarioId,
  };
}

export function mockProductConfig(root: string, scenarioPath: string) {
  return {
    version: 1 as const,
    state: { databasePath: join(root, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "mockproduct",
      pairingToken: "pairing.mock.product.synthetic.0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://mock-product.test"],
    },
    agentGateway: {
      databasePath: join(root, "agent.sqlite"),
      host: "127.0.0.1" as const,
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.mock.product.synthetic.0001",
      scopes: [],
    },
    managedTerminals: {
      enabled: true,
      databasePath: join(root, "terminals.sqlite"),
      profiles: [],
      outputHistoryLimit: 256,
    },
    managedAgents: [
      {
        profileId: MOCK_PROFILE_ID,
        tier: "small" as const,
        projectId: MOCK_PROJECT_ID,
        contextId: MOCK_CONTEXT_ID,
        provider: "zap_mock" as const,
        executablePath: process.execPath,
        argumentPrefix: ["--experimental-strip-types", mockEntry],
        cwd: root,
        modelId: "zap-mock/deterministic-v1",
        effort: null,
        effortSupported: false,
        environmentRef: null,
        mcpConfigPath: join(root, "mcp", "mock.json"),
        mcpCommandPath: process.execPath,
        mcpArgs: ["--experimental-strip-types", mcpEntry],
        mockScenarioPath: scenarioPath,
        capabilities: {
          provider: "zap_mock" as const,
          observedVersion: "deterministic-v1",
          installed: true,
          launchable: true,
          authenticated: "not_observed" as const,
          structuredConversation: "supported" as const,
          nativeChildren: "unsupported" as const,
          nativeQuestions: "supported" as const,
          nativeApprovals: "unsupported" as const,
          interrupt: "supported" as const,
          resume: "supported" as const,
          interactiveTerminal: "supported" as const,
          evidence: ["Explicit deterministic ZapMockAgent product gate"],
        },
      },
    ],
    profiles: [
      {
        profileId: "profile.mock.coordinator.unused",
        executablePath: process.execPath,
        requestTimeoutMs: 30_000,
        model: "unused",
        effort: "low" as const,
        approvalPolicy: "on-request" as const,
        sandbox: "workspace-write" as const,
        personality: "pragmatic" as const,
        serviceName: "mock-product-unused",
      },
    ],
    projects: [
      {
        registrationId: "request.project.mock.product",
        projectId: MOCK_PROJECT_ID,
        displayName: "ZapMock product gate",
        repositoryRootRefs: ["repository.mock.product"],
        actions: { startCoordinator: { state: "available" as const } },
        context: {
          contextId: MOCK_CONTEXT_ID,
          displayName: "Mock product context",
          workspaceRef: "workspace.mock.product",
          branchLabel: "synthetic",
          revisionBinding: "synthetic",
          planning: { state: "unavailable" as const, reason: "mock product gate" },
          coordinatorConversationId: "conversation.mock.product",
          brokerScope: {
            workspaceId: "workspace.mock.product",
            conversationId: "conversation.mock.product",
          },
        },
        coordinatorLaunchOptions: [
          {
            profileId: "profile.mock.coordinator.unused",
            label: "Unused mock coordinator",
            interactionKind: "structured" as const,
            availability: { state: "available" as const },
          },
        ],
        protected: { cwd: root, launchProfileRef: "profile.mock.coordinator.unused" },
      },
    ],
    modelPolicies: [
      { projectId: MOCK_PROJECT_ID, contextId: MOCK_CONTEXT_ID, policyId: "policy.mock.product" },
    ],
  };
}

export function mockUnusedHost() {
  return {
    hostId: ExecutionHostIdSchema.parse("host.mock.product.unused"),
    profileIds: ["profile.mock.coordinator.unused"],
    openCoordinator: () =>
      Promise.resolve({
        ok: false as const,
        error: { code: "unsupported" as const, message: "unused", retry: "never" as const },
      }),
  };
}
