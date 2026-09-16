/** Validated zero-LLM simulation corpus contracts. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import { z } from "zod";
import {
  ZapMockInputSchema,
  ZapMockScenarioSchema,
  ZapMockStateSchema,
} from "../mock-model/index.ts";
import { RepositoryGitSimulationSchema } from "../repository-workspaces/index.ts";
import {
  ExecutionCatalogProductSimulationSchema,
  ExecutionCatalogRuntimeSimulationSchema,
} from "./execution-catalog-schema.ts";
import { SourceInstallProductSimulationSchema } from "./source-install-schema.ts";

export const MockSimulationRunnerIdSchema = z.enum([
  "model.reducer",
  "wayfinder.mock-managed",
  "coordinator_http",
  "workspace.store-plan-context",
  "repository.git",
  "quicklens.portfolio-repository",
  "workspace.annotations-product",
  "workspace.model-policy-product",
  "wayfinder.repository-product",
  "wayfinder.repository-managed-product",
  "provider-coordinator.public-projection",
  "execution-catalog.public-routing",
  "execution-catalog.managed-runtime",
  "source-install.public-lifecycle",
]);
export type MockSimulationRunnerId = z.infer<typeof MockSimulationRunnerIdSchema>;

const CommonSchema = z.object({
  protocol: z.literal("zap-mock-behavior/1"),
  scenarioId: z.string().min(3).max(160),
  runnerId: MockSimulationRunnerIdSchema,
  tags: z.array(z.string().min(1).max(80)).min(1).max(32),
  coverage: z.array(z.string().min(3).max(160)).min(1).max(64),
});
const EffectKindSchema = z.enum([
  "session_ready",
  "turn_started",
  "assistant_message",
  "turn_settled",
  "busy",
  "question",
  "answer_received",
  "answer_acknowledged",
  "work_reported",
  "paused",
  "continued",
  "restarted",
  "delivery",
]);
const ScenarioFileSchema = z
  .object({
    protocol: z.literal("zap-mock-scenario/1"),
    seed: z.string().min(1).max(512),
    scenario: ZapMockScenarioSchema,
  })
  .strict();

export const ModelReducerSimulationSchema = CommonSchema.extend({
  kind: z.literal("model_reducer"),
  runnerId: z.literal("model.reducer"),
  scenarioFile: ScenarioFileSchema,
  inputTape: z.array(ZapMockInputSchema).min(1).max(10_000),
  expected: z
    .object({
      effectKindsByInput: z.array(z.array(EffectKindSchema).max(32)).min(1).max(10_000),
      finalState: z
        .object({
          status: ZapMockStateSchema.shape.status,
          cursor: z.number().int().min(0),
          logicalTick: z.number().int().min(0),
          generation: z.number().int().min(1),
        })
        .strict(),
      traceLength: z.number().int().min(1).max(10_000),
    })
    .strict(),
})
  .strict()
  .superRefine((document, context) => {
    if (document.inputTape.length !== document.expected.effectKindsByInput.length)
      context.addIssue({
        code: "custom",
        path: ["expected", "effectKindsByInput"],
        message: "expected effect rows must match the input tape length",
      });
  });

export const ManagedProductSimulationSchema = CommonSchema.extend({
  kind: z.literal("managed_product"),
  runnerId: z.literal("wayfinder.mock-managed"),
  scenarioFile: ScenarioFileSchema,
  inputs: z
    .object({
      answerOptionIndex: z.number().int().min(0).max(31),
      pauseBeforeAnswer: z.literal(true),
      answerNote: z.string().min(1).max(8_000),
    })
    .strict(),
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
}).strict();

export const CoordinatorHttpSimulationSchema = CommonSchema.extend({
  kind: z.literal("coordinator_http"),
  runnerId: z.literal("coordinator_http"),
  scenarioFile: ScenarioFileSchema,
  inputs: z
    .object({
      initialMessage: z.string().min(1).max(64_000),
      answerText: z.string().min(1).max(32_000),
      pauseBeforeAnswer: z.literal(true),
      pauseReason: z.string().min(1).max(8_000),
      continueReason: z.string().min(1).max(8_000),
    })
    .strict(),
  expected: z
    .object({
      questionPrompt: z.string().min(1).max(16_000),
      pausedExecutionState: z.literal("paused"),
      acknowledgementsBeforeContinue: z.literal(0),
      acknowledgementsAfterContinue: z.literal(1),
      finalModelStatus: z.literal("complete"),
      sameSession: z.literal(true),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const WorkspaceStoreSimulationSchema = CommonSchema.extend({
  kind: z.literal("workspace_store"),
  runnerId: z.literal("workspace.store-plan-context"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      baseProjectId: z.string().min(3).max(160),
      baseContextId: z.string().min(3).max(160),
      planId: z.string().min(3).max(160),
      planContextId: z.string().min(3).max(160),
      repositoryId: z.string().min(3).max(160),
      rootWorktreeId: z.string().min(3).max(160),
    })
    .strict(),
  expected: z
    .object({
      contextCount: z.literal(2),
      defaultContextPreserved: z.literal(true),
      exactContextLaunches: z.literal(true),
      contextScopedCoordinator: z.literal(true),
      idempotentReplay: z.literal(true),
      legacyDigestStable: z.literal(true),
      productCatalogRehydrates: z.literal(true),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

const PortfolioWorktreeSchema = z
  .object({
    contextId: z.string().min(3).max(160),
    worktreeId: z.string().min(3).max(160),
    branchRef: z.string().min(1).max(512),
    headCommit: z.string().min(1).max(160).optional(),
  })
  .strict();
export const PortfolioRepositorySimulationSchema = CommonSchema.extend({
  kind: z.literal("portfolio_repository"),
  runnerId: z.literal("quicklens.portfolio-repository"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      projectId: z.string().min(3).max(160),
      repositoryId: z.string().min(3).max(160),
      registered: PortfolioWorktreeSchema.required({ headCommit: true }),
      plans: z
        .array(
          PortfolioWorktreeSchema.omit({ headCommit: true, worktreeId: true }).extend({
            planId: z.string().min(3).max(160),
            displayName: z.string().min(1).max(256),
            rootWorktreeId: z.string().min(3).max(160),
          }),
        )
        .min(1)
        .max(64),
      worker: z
        .object({
          worktreeId: z.string().min(3).max(160),
          parentWorktreeId: z.string().min(3).max(160),
          branchRef: z.string().min(1).max(512),
          actorId: z.string().min(3).max(160),
          taskId: z.string().min(3).max(160),
          runId: z.string().min(3).max(160),
        })
        .strict(),
      integration: z
        .object({
          integrationId: z.string().min(3).max(160),
          sourceWorktreeId: z.string().min(3).max(160),
          targetWorktreeId: z.string().min(3).max(160),
          integrationWorktreeId: z.string().min(3).max(160),
          conflictPaths: z.array(z.string().min(1).max(4_096)).max(2_048),
        })
        .strict(),
    })
    .strict(),
  expected: z
    .object({
      planCount: z.number().int().min(1).max(64),
      sharedRegisteredRoot: z.literal(true),
      workerAssignmentVisible: z.literal(true),
      conflictVisible: z.literal(true),
      contextFrameCount: z.number().int().min(1).max(10_000),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const AnnotationsProductSimulationSchema = CommonSchema.extend({
  kind: z.literal("annotations_product"),
  runnerId: z.literal("workspace.annotations-product"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      projectId: z.string().min(3).max(160),
      contextId: z.string().min(3).max(160),
      targetDomain: z.literal("work_task"),
      targetRef: z.string().min(3).max(160),
      recipientActorId: z.string().min(3).max(160),
      firstAttemptId: z.string().min(3).max(160),
      restoredAttemptId: z.string().min(3).max(160),
      title: z.string().min(1).max(512),
      bodyMarkdown: z.string().min(1).max(8_000),
      sourceBasisRef: z.string().min(1).max(512),
    })
    .strict(),
  expected: z
    .object({
      initialInstructionCount: z.literal(1),
      archivedInstructionCount: z.literal(0),
      restoredInstructionCount: z.literal(1),
      trashEntryCount: z.literal(1),
      restoredVersion: z.string().regex(/^(0|[1-9][0-9]*)$/),
      wrongActorAckRefused: z.literal(true),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const ModelPolicyProductSimulationSchema = CommonSchema.extend({
  kind: z.literal("model_policy_product"),
  runnerId: z.literal("workspace.model-policy-product"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      projectId: z.string().min(3).max(160),
      contextId: z.string().min(3).max(160),
      otherProjectId: z.string().min(3).max(160),
      otherContextId: z.string().min(3).max(160),
      runId: z.string().min(3).max(160),
      attemptId: z.string().min(3).max(160),
      selectionRef: z.string().min(3).max(160),
      policyId: z.string().min(3).max(160),
      profileId: z.string().min(3).max(160),
      modelId: z.string().min(1).max(256),
      effort: z.string().min(1).max(64),
    })
    .strict(),
  expected: z
    .object({
      selectedPolicyRevision: z.string().regex(/^(0|[1-9][0-9]*)$/),
      selectedModelId: z.string().min(1).max(256),
      historyRevisions: z
        .array(z.string().regex(/^(0|[1-9][0-9]*)$/))
        .min(1)
        .max(64),
      selectionRemainsPinned: z.literal(true),
      foreignProjectRefused: z.literal(true),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const RepositoryProductSimulationSchema = CommonSchema.extend({
  kind: z.literal("repository_product"),
  runnerId: z.literal("wayfinder.repository-product"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      planDisplayNames: z.array(z.string().min(1).max(200)).length(2),
      noteTitle: z.string().min(1).max(512),
      noteBodyMarkdown: z.string().min(1).max(8_000),
      changePath: z.string().min(1).max(1_024),
      changeContent: z.string().min(1).max(8_000),
      testProfileId: z.string().min(1).max(160),
      reviewRationale: z.string().min(1).max(8_000),
    })
    .strict(),
  expected: z
    .object({
      planCount: z.literal(2),
      crossContextErrorCode: z.literal("not_found"),
      noteTargetDomain: z.literal("worktree"),
      integrationStateAfterPrepare: z.literal("candidate"),
      integrationStateAfterPromote: z.literal("promoted"),
      diffIncludesDeclaredChange: z.literal(true),
      exactContextReads: z.literal(true),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const RepositoryManagedProductSimulationSchema = CommonSchema.extend({
  kind: z.literal("repository_managed_product"),
  runnerId: z.literal("wayfinder.repository-managed-product"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      mockBehaviorSeed: z.string().min(1).max(512),
      initialPath: z.string().min(1).max(1_024),
      initialContent: z.string().min(1).max(8_000),
      sourcePlanDisplayName: z.string().min(1).max(200),
      changePath: z.string().min(1).max(1_024),
      changeContent: z.string().min(1).max(8_000),
      workerProfileId: z.string().min(3).max(160),
      workerGoal: z.string().min(1).max(8_000),
      workerExpectedResult: z.string().min(1).max(8_000),
      answerNoteMarkdown: z.string().min(1).max(8_000),
    })
    .strict(),
  expected: z
    .object({
      defaultPlanAdopted: z.literal(true),
      assignmentKind: z.literal("isolated_child"),
      isolatedChildAgainstContextRoot: z.literal(true),
      promotionBlockedWhileRunning: z.literal(true),
      projectPauseSettled: z.literal(true),
      promotionWhilePaused: z.literal(true),
      answerDeliveredAfterContinue: z.literal(true),
      reportIncludes: z.string().min(1).max(8_000),
      assignmentOwnershipRetained: z.literal(true),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const ProviderProjectionSimulationSchema = CommonSchema.extend({
  kind: z.literal("provider_projection"),
  runnerId: z.literal("provider-coordinator.public-projection"),
  seed: z.string().min(1).max(512),
  inputs: z
    .object({
      provider: z.literal("qwen_code"),
      firstMessage: z.string().min(1).max(64_000),
      secondMessage: z.string().min(1).max(64_000),
      intermediateText: z.string().min(1).max(64_000),
      firstReply: z.string().min(1).max(64_000),
      secondReply: z.string().min(1).max(64_000),
      firstClientRequestId: z.string().min(3).max(160),
      secondClientRequestId: z.string().min(3).max(160),
    })
    .strict(),
  expected: z
    .object({
      acceptedNativeTurnId: z.null(),
      firstState: z.literal("running"),
      sendCountWhileBusy: z.literal(1),
      stateAfterQueuedDispatch: z.literal("running"),
      finalState: z.literal("ready"),
      exactTransportCorrelation: z.literal(true),
      intermediateMessageBoundaryKeepsRunning: z.literal(true),
      repeatedCompletionsRetained: z.literal(true),
      assistantReplies: z.array(z.string().min(1).max(64_000)).length(2),
      zeroLlmInference: z.literal(true),
    })
    .strict(),
}).strict();

export const MockSimulationDocumentSchema = z.union([
  ModelReducerSimulationSchema,
  ManagedProductSimulationSchema,
  CoordinatorHttpSimulationSchema,
  WorkspaceStoreSimulationSchema,
  RepositoryGitSimulationSchema,
  PortfolioRepositorySimulationSchema,
  AnnotationsProductSimulationSchema,
  ModelPolicyProductSimulationSchema,
  RepositoryProductSimulationSchema,
  RepositoryManagedProductSimulationSchema,
  ProviderProjectionSimulationSchema,
  ExecutionCatalogProductSimulationSchema,
  ExecutionCatalogRuntimeSimulationSchema,
  SourceInstallProductSimulationSchema,
]);
export type MockSimulationDocument = z.infer<typeof MockSimulationDocumentSchema>;

const CoverageRowSchema = z.union([
  z
    .object({
      feature: z.string().min(3).max(160),
      scenarioIds: z.array(z.string().min(3).max(160)).min(1).max(64),
    })
    .strict(),
  z.object({ feature: z.string().min(3).max(160), gap: z.string().min(1).max(2_000) }).strict(),
]);
export const MockSimulationRegistrySchema = z
  .object({
    protocol: z.literal("zap-mock-coverage/1"),
    documents: z
      .array(
        z
          .object({
            scenarioId: z.string().min(3).max(160),
            path: z.string().min(1).max(1_000),
            runnerId: MockSimulationRunnerIdSchema,
          })
          .strict(),
      )
      .max(10_000),
    featureInventory: z.array(z.string().min(3).max(160)).max(10_000),
    coverage: z.array(CoverageRowSchema).max(10_000),
  })
  .strict();
export type MockSimulationRegistry = z.infer<typeof MockSimulationRegistrySchema>;
