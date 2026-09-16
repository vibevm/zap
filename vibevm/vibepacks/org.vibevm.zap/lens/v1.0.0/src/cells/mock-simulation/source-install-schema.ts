/** Source-install lifecycle simulation. @scope spec://org.vibevm.zap/lens/PROP-016#verification */
import { z } from "zod";

const Id = z.string().min(3).max(160);

export const SourceInstallProductSimulationSchema = z
  .object({
    protocol: z.literal("zap-mock-behavior/1"),
    kind: z.literal("source_install_product"),
    scenarioId: Id,
    runnerId: z.literal("source-install.public-lifecycle"),
    seed: z.string().min(1).max(512),
    tags: z.array(z.string().min(1).max(80)).min(1).max(32),
    coverage: z.array(Id).min(1).max(64),
    inputs: z
      .object({
        platform: z.enum(["win32", "linux", "darwin"]),
        homeDir: z.string().min(1).max(32_768),
        modulePath: z.string().min(1).max(32_768),
        settingsDir: z.string().min(1).max(32_768),
        registryDir: z.string().min(1).max(32_768),
        vibeExecutable: z.string().min(1).max(32_768),
        npmRegistry: z.url(),
        markerRelativePath: z.string().min(1).max(512),
        installerRelativePath: z.string().min(1).max(512),
        commands: z.array(z.enum(["install", "status", "update", "uninstall"])).length(4),
        failurePhase: z.enum(["materialize", "build", "package", "deploy"]),
        unrelatedFile: z.string().min(1).max(512),
      })
      .strict(),
    expected: z
      .object({
        protocol: z.literal("zap-source-install/1"),
        installationId: z.literal("org.vibevm.zap.source-install"),
        lensCoordinate: z.literal("org.vibevm.zap/lens@1.0.0"),
        engineCoordinate: z.literal("org.vibevm.zap/zap@1.0.0"),
        structuredProcessArguments: z.literal(true),
        pathsWithSpacesPreserved: z.literal(true),
        statusProcessCalls: z.literal(0),
        statusWrites: z.literal(0),
        standaloneRegistryRefused: z.literal(true),
        unrelatedDirectoryRefused: z.literal(true),
        markerMismatchRefused: z.literal(true),
        failureNeverReady: z.literal(true),
        buildWithoutPreparedRefused: z.literal(true),
        deploySkippedWithoutPrepared: z.literal(true),
        npmRegistryNormalized: z.literal(true),
        npmRegistryPreservedOnUpdate: z.literal(true),
        unsafeNpmRegistryRefused: z.literal(true),
        shortQuickAliasIncluded: z.literal(true),
        legacyQuickAliasRetained: z.literal(true),
        headlessServerNoAgentStart: z.literal(true),
        previousGenerationPreserved: z.literal(true),
        receiptOwnedUndeployOnly: z.literal(true),
        cachePreserved: z.literal(true),
        userStatePreserved: z.literal(true),
        zeroLlmInference: z.literal(true),
      })
      .strict(),
  })
  .strict();

export type SourceInstallProductSimulation = z.infer<typeof SourceInstallProductSimulationSchema>;
