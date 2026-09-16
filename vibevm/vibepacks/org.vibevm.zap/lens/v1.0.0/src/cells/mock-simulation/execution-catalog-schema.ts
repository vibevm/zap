/** Synthetic cross-provider catalog scenario. @scope spec://org.vibevm.zap/lens/PROP-015#mock */
import { z } from "zod";
import {
  AgentProductSchema,
  ExecutionModalitySchema,
  TaskSpecializationSchema,
} from "../execution-catalog/index.ts";

const Id = z.string().min(3).max(160);
const MockIdentity = z.enum(["codex", "claude_code", "opencode", "qwen_code", "zap_mock"]);
const Score = z
  .object({
    specialization: TaskSpecializationSchema,
    suitability: z.number().int().min(0).max(100),
    quality: z.number().int().min(0).max(100),
    economy: z.number().int().min(0).max(100),
    order: z.number().int().min(0).max(10_000),
  })
  .strict();

export const ExecutionCatalogProductSimulationSchema = z
  .object({
    protocol: z.literal("zap-mock-behavior/1"),
    kind: z.literal("execution_catalog_product"),
    scenarioId: Id,
    runnerId: z.literal("execution-catalog.public-routing"),
    seed: z.string().min(1).max(512),
    tags: z.array(z.string().min(1).max(80)).min(1).max(32),
    coverage: z.array(Id).min(1).max(64),
    inputs: z
      .object({
        now: z.iso.datetime(),
        projectId: Id,
        contextId: Id,
        foreignProjectId: Id,
        connections: z
          .array(
            z
              .object({
                connectionId: Id,
                accountName: z.string().min(1).max(160),
                agentProduct: AgentProductSchema,
                emulatedAgentProduct: MockIdentity,
                usage: z
                  .object({
                    state: z.enum(["fresh", "stale", "unknown", "unsupported"]),
                    remainingPercent: z.number().min(0).max(100).nullable(),
                    bucketId: Id,
                  })
                  .strict(),
              })
              .strict(),
          )
          .min(2)
          .max(64),
        personas: z
          .array(
            z
              .object({
                configurationId: Id,
                connectionId: Id,
                agentProduct: AgentProductSchema,
                emulatedAgentProduct: MockIdentity,
                modelVendorId: Id,
                modelFamilyId: Id,
                modelId: z.string().min(1).max(256),
                imageToolModelId: z.string().min(1).max(256).optional(),
                efforts: z
                  .array(
                    z.enum([
                      "default",
                      "none",
                      "minimal",
                      "low",
                      "medium",
                      "high",
                      "xhigh",
                      "max",
                      "ultra",
                    ]),
                  )
                  .min(1),
                contexts: z.array(z.number().int().positive()).min(1).max(32),
                modalities: z.array(ExecutionModalitySchema).min(1).max(3),
                scores: z.array(Score).min(1).max(TaskSpecializationSchema.options.length),
              })
              .strict(),
          )
          .min(10)
          .max(128),
        routeCases: z
          .array(
            z
              .object({
                caseId: Id,
                specialization: TaskSpecializationSchema,
                weight: z.number().int().min(0).max(100),
                quotaEnabled: z.boolean(),
                expectedConfigurationId: Id,
              })
              .strict(),
          )
          .min(10)
          .max(128),
        rename: z.object({ configurationId: Id, displayName: z.string().min(1).max(200) }).strict(),
      })
      .strict(),
    expected: z
      .object({
        uniqueFamilyCount: z.number().int().min(10),
        sameAgentAccountCount: z.number().int().min(2),
        generatedNamesUnique: z.literal(true),
        renamedDisplayName: z.string().min(1).max(200),
        staleQuotaNotLow: z.literal(true),
        unknownQuotaNotZero: z.literal(true),
        staleRevisionRefused: z.literal(true),
        replayStable: z.literal(true),
        coldReopenStable: z.literal(true),
        revokedFutureDispatchRefused: z.literal(true),
        pinnedSelectionPreserved: z.literal(true),
        foreignProjectRefused: z.literal(true),
        unsupportedEffortRefused: z.literal(true),
        unsupportedContextRefused: z.literal(true),
        imageControllerModelId: z.string().min(1).max(256),
        imageToolModelId: z.string().min(1).max(256),
        imageArtifactRef: Id,
        recordingDispatchCount: z.number().int().min(10),
        materializedAgentProduct: z.literal("zap_mock"),
        emulatedIdentitySeparate: z.literal(true),
        zeroLlmInference: z.literal(true),
      })
      .strict(),
  })
  .strict();

export type ExecutionCatalogProductSimulation = z.infer<
  typeof ExecutionCatalogProductSimulationSchema
>;

export const ExecutionCatalogRuntimeSimulationSchema = z
  .object({
    protocol: z.literal("zap-mock-behavior/1"),
    kind: z.literal("execution_catalog_runtime"),
    scenarioId: Id,
    runnerId: z.literal("execution-catalog.managed-runtime"),
    seed: z.string().min(1).max(512),
    tags: z.array(z.string().min(1).max(80)).min(1).max(32),
    coverage: z.array(Id).min(1).max(64),
    inputs: z
      .object({
        bindingDisplayName: z.string().min(1).max(160),
        specialization: TaskSpecializationSchema,
        goal: z.string().min(1).max(8_000),
        expectedResult: z.string().min(1).max(8_000),
      })
      .strict(),
    expected: z
      .object({
        provider: z.literal("zap_mock"),
        modelId: z.literal("zap-mock/deterministic-v1"),
        modelFamilyId: z.literal("zap_mock"),
        questionPrompt: z.string().min(1).max(8_000),
        selectionPinned: z.literal(true),
        zeroLlmInference: z.literal(true),
      })
      .strict(),
  })
  .strict();
export type ExecutionCatalogRuntimeSimulation = z.infer<
  typeof ExecutionCatalogRuntimeSimulationSchema
>;
