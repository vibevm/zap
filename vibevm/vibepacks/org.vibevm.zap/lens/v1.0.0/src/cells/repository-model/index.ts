/** Public repository-workspace records. @scope spec://org.vibevm.zap/lens/PROP-014#root */
import { z } from "zod";

const OpaqueIdSchema = z
  .string()
  .min(1)
  .max(160)
  .regex(/^[A-Za-z0-9][A-Za-z0-9._:-]*$/);
export const RepositoryIdSchema = OpaqueIdSchema;
export const ProjectPlanIdSchema = OpaqueIdSchema;
export const RepositoryWorktreeIdSchema = OpaqueIdSchema;
export const IntegrationAttemptIdSchema = OpaqueIdSchema;
export const GitObjectIdSchema = z.string().regex(/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/);
export const RevisionSchema = z.string().regex(/^(?:0|[1-9][0-9]*)$/);

export const AlgorithmBindingSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("pending") }).strict(),
  z
    .object({
      state: z.literal("bound"),
      storeId: OpaqueIdSchema,
      campaignId: OpaqueIdSchema,
      baseId: OpaqueIdSchema,
      adoptedPlanKey: z
        .object({ outcomeId: OpaqueIdSchema, generation: z.number().int().min(0) })
        .strict()
        .nullable(),
      snapshotRevision: RevisionSchema,
    })
    .strict(),
]);
export type AlgorithmBinding = z.infer<typeof AlgorithmBindingSchema>;

export const RepositoryRecordSchema = z
  .object({
    repositoryId: RepositoryIdSchema,
    executionHostId: OpaqueIdSchema,
    displayName: z.string().min(1).max(200),
    objectFormat: z.enum(["sha1", "sha256"]),
    revision: RevisionSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type RepositoryRecord = z.infer<typeof RepositoryRecordSchema>;

export const RepositoryProjectBindingSchema = z
  .object({
    repositoryId: RepositoryIdSchema,
    executionHostId: OpaqueIdSchema,
    projectId: OpaqueIdSchema,
    registeredWorktreeId: RepositoryWorktreeIdSchema,
    creatorPrincipalId: OpaqueIdSchema,
    revision: RevisionSchema,
  })
  .strict();
export type RepositoryProjectBinding = z.infer<typeof RepositoryProjectBindingSchema>;

export const ProjectPlanRecordSchema = z
  .object({
    planId: ProjectPlanIdSchema,
    repositoryId: RepositoryIdSchema,
    executionHostId: OpaqueIdSchema,
    projectId: OpaqueIdSchema,
    contextId: OpaqueIdSchema,
    displayName: z.string().min(1).max(200),
    rootWorktreeId: RepositoryWorktreeIdSchema,
    integrationTargetWorktreeId: RepositoryWorktreeIdSchema,
    algorithmBinding: AlgorithmBindingSchema,
    state: z.enum(["preparing", "ready", "unavailable"]),
    creatorPrincipalId: OpaqueIdSchema,
    lastUpdatedByPrincipalId: OpaqueIdSchema,
    revision: RevisionSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type ProjectPlanRecord = z.infer<typeof ProjectPlanRecordSchema>;

export const WorktreeAssignmentSchema = z
  .object({
    assignmentId: OpaqueIdSchema,
    attemptId: OpaqueIdSchema,
    assignerPrincipalId: OpaqueIdSchema,
    basisCommit: GitObjectIdSchema,
    actorId: OpaqueIdSchema.nullable(),
    taskId: OpaqueIdSchema.nullable(),
    runId: OpaqueIdSchema.nullable(),
    semanticTargetRefs: z
      .array(
        z
          .object({
            projectId: OpaqueIdSchema,
            contextId: OpaqueIdSchema,
            domain: z.enum([
              "project",
              "semantic_object",
              "semantic_relationship",
              "agent",
              "work_task",
              "work_run",
              "plan_workspace",
              "worktree",
              "integration",
            ]),
            ref: z.string().min(1).max(1_152),
          })
          .strict(),
      )
      .max(256),
    assignedAt: z.iso.datetime(),
    releasedAt: z.iso.datetime().nullable(),
  })
  .strict();
export type WorktreeAssignment = z.infer<typeof WorktreeAssignmentSchema>;

export const RepositoryWorktreeRecordSchema = z
  .object({
    worktreeId: RepositoryWorktreeIdSchema,
    repositoryId: RepositoryIdSchema,
    executionHostId: OpaqueIdSchema,
    projectId: OpaqueIdSchema,
    contextId: OpaqueIdSchema,
    planId: ProjectPlanIdSchema.nullable(),
    kind: z.enum(["registered", "plan_root", "worker", "integration"]),
    parentWorktreeId: RepositoryWorktreeIdSchema.nullable(),
    branchRef: z.string().min(1).max(500),
    basisCommit: GitObjectIdSchema,
    headCommit: GitObjectIdSchema,
    state: z.enum(["preparing", "ready", "conflicted", "unavailable"]),
    assignments: z.array(WorktreeAssignmentSchema).max(1024),
    revision: RevisionSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type RepositoryWorktreeRecord = z.infer<typeof RepositoryWorktreeRecordSchema>;

export const IntegrationTestEvidenceSchema = z
  .object({
    profileId: OpaqueIdSchema,
    runnerId: OpaqueIdSchema,
    runnerAuthorityId: OpaqueIdSchema,
    requestedByPrincipalId: OpaqueIdSchema,
    commit: GitObjectIdSchema,
    passed: z.boolean(),
    summary: z.string().min(1).max(4_000),
    observedAt: z.iso.datetime(),
  })
  .strict();
export type IntegrationTestEvidence = z.infer<typeof IntegrationTestEvidenceSchema>;

export const IntegrationReviewSchema = z
  .object({
    commit: GitObjectIdSchema,
    accepted: z.boolean(),
    reviewerPrincipalId: OpaqueIdSchema,
    rationale: z.string().min(1).max(8_000),
    observedAt: z.iso.datetime(),
  })
  .strict();
export type IntegrationReview = z.infer<typeof IntegrationReviewSchema>;

export const IntegrationAttemptSchema = z
  .object({
    integrationId: IntegrationAttemptIdSchema,
    repositoryId: RepositoryIdSchema,
    executionHostId: OpaqueIdSchema,
    planId: ProjectPlanIdSchema,
    sourceWorktreeId: RepositoryWorktreeIdSchema,
    targetWorktreeId: RepositoryWorktreeIdSchema,
    integrationWorktreeId: RepositoryWorktreeIdSchema,
    expectedSourceHead: GitObjectIdSchema,
    expectedTargetHead: GitObjectIdSchema,
    candidateCommit: GitObjectIdSchema.nullable(),
    conflictPaths: z.array(z.string().min(1).max(1_024)).max(2_048),
    state: z.enum([
      "preparing",
      "conflicted",
      "candidate",
      "tested",
      "accepted",
      "rejected",
      "stale",
      "promoted",
      "unavailable",
    ]),
    testEvidence: IntegrationTestEvidenceSchema.nullable(),
    review: IntegrationReviewSchema.nullable(),
    creatorPrincipalId: OpaqueIdSchema,
    revision: RevisionSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type IntegrationAttempt = z.infer<typeof IntegrationAttemptSchema>;
