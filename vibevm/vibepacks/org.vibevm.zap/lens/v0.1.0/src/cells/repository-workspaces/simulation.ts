/** Real-Git zero-inference scenario contract. @scope spec://org.vibevm.zap/lens/PROP-014#verification */
import { z } from "zod";

const KeySchema = z
  .string()
  .min(1)
  .max(80)
  .regex(/^[a-z][a-z0-9._-]*$/);
const IdSchema = z
  .string()
  .min(1)
  .max(160)
  .regex(/^[A-Za-z0-9][A-Za-z0-9._:-]*$/);
const SafeFixturePathSchema = z
  .string()
  .min(1)
  .max(500)
  .superRefine((value, context) => {
    const segments = value.split("/");
    if (
      value.includes("\\") ||
      value.startsWith("/") ||
      /^[A-Za-z]:/.test(value) ||
      segments.some(
        (segment) =>
          segment === "" || segment === "." || segment === ".." || segment.toLowerCase() === ".git",
      )
    )
      context.addIssue({
        code: "custom",
        message: "fixture path must stay below the fixture repository",
      });
  });
const FileMutationSchema = z
  .object({ path: SafeFixturePathSchema, content: z.string().max(100_000) })
  .strict();
const CommonActionSchema = z
  .object({ actionId: KeySchema, requestKey: KeySchema.optional() })
  .strict();
const InputActionSchema = z.discriminatedUnion("kind", [
  CommonActionSchema.extend({
    kind: z.literal("register_repository"),
    projectKey: KeySchema,
    projectId: IdSchema,
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("prepare_plan"),
    planKey: KeySchema,
    planId: IdSchema,
    contextId: IdSchema,
    baseWorktreeKey: KeySchema,
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("prepare_child"),
    worktreeKey: KeySchema,
    planKey: KeySchema,
    parentWorktreeKey: KeySchema,
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("fixture_commit"),
    worktreeKey: KeySchema,
    files: z.array(FileMutationSchema).min(1).max(64),
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("fixture_write"),
    worktreeKey: KeySchema,
    files: z.array(FileMutationSchema).min(1).max(64),
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("prepare_integration"),
    integrationKey: KeySchema,
    integrationId: IdSchema,
    planKey: KeySchema,
    sourceWorktreeKey: KeySchema,
    targetWorktreeKey: KeySchema,
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("fixture_resolve"),
    integrationKey: KeySchema,
    files: z.array(FileMutationSchema).min(1).max(64),
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("run_test"),
    integrationKey: KeySchema,
    profileId: z.enum(["fixture.clean", "fixture.file_equals"]),
    expectedFile: FileMutationSchema.optional(),
  }).strict(),
  CommonActionSchema.extend({
    kind: z.literal("record_review"),
    integrationKey: KeySchema,
    accepted: z.boolean(),
  }).strict(),
  CommonActionSchema.extend({ kind: z.literal("promote"), integrationKey: KeySchema }).strict(),
  CommonActionSchema.extend({ kind: z.literal("reopen_store") }).strict(),
  CommonActionSchema.extend({ kind: z.literal("inspect") }).strict(),
]);

export const RepositoryGitSimulationSchema = z
  .object({
    protocol: z.literal("zap-mock-behavior/1"),
    kind: z.literal("repository_git"),
    scenarioId: z.string().min(3).max(160),
    runnerId: z.literal("repository.git"),
    seed: z.string().min(1).max(512),
    tags: z.array(z.string().min(1).max(80)).min(1).max(32),
    coverage: z.array(z.string().min(3).max(160)).min(1).max(64),
    inputs: z
      .object({
        fixture: z
          .object({
            projectRelativePath: SafeFixturePathSchema,
            initialFiles: z.array(FileMutationSchema).min(1).max(128),
            ignoredFiles: z.array(FileMutationSchema).max(32),
          })
          .strict(),
        inputTape: z.array(InputActionSchema).min(1).max(256),
      })
      .strict(),
    expected: z
      .object({
        finalActionStates: z.record(KeySchema, z.enum(["succeeded", "refused", "conflicted"])),
        targetWorktreeKey: KeySchema,
        targetFiles: z.array(FileMutationSchema).max(128),
        conflictPaths: z.array(SafeFixturePathSchema).max(128),
        refusalCodes: z.record(
          KeySchema,
          z.enum([
            "invalid_input",
            "conflict",
            "unavailable",
            "host_unavailable",
            "dirty",
            "stale",
            "conflicted",
            "not_ready",
            "denied",
          ]),
        ),
        unchangedOriginalCheckout: z.literal(true),
        zeroInference: z.literal(true),
      })
      .strict(),
  })
  .strict();
export type RepositoryGitSimulation = z.infer<typeof RepositoryGitSimulationSchema>;
