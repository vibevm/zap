/** Browser-safe repository plan/worktree/integration API. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { z } from "zod";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import {
  GitObjectIdSchema,
  IntegrationAttemptIdSchema,
  IntegrationAttemptSchema,
  ProjectPlanIdSchema,
  ProjectPlanRecordSchema,
  RepositoryIdSchema,
  RepositoryProjectBindingSchema,
  RepositoryRecordSchema,
  RepositoryWorktreeIdSchema,
  RepositoryWorktreeRecordSchema,
  RevisionSchema,
} from "../repository-model/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";
import { WorkContextDescriptorSchema } from "./entities.ts";
import { ManagedWorkViewSchema } from "./managed-work.ts";

const ScopeSchema = z.object({ projectId: ProjectIdSchema, contextId: WorkContextIdSchema });
const PlanScopeSchema = ScopeSchema.extend({ planId: ProjectPlanIdSchema });
export const RepositoryTestProfileSchema = z
  .object({
    profileId: z.string().min(1).max(160),
    displayName: z.string().min(1).max(256),
    descriptionMarkdown: z.string().max(2_000),
  })
  .strict();

export const RepositoryWorkspaceReadRequestSchemas = [
  ScopeSchema.extend({ operation: z.literal("repository.get.v1") }).strict(),
  z.object({ operation: z.literal("plan.workspace.list.v1"), projectId: ProjectIdSchema }).strict(),
  PlanScopeSchema.extend({ operation: z.literal("plan.workspace.get.v1") }).strict(),
  PlanScopeSchema.extend({ operation: z.literal("worktree.list.v1") }).strict(),
  ScopeSchema.extend({
    operation: z.literal("worktree.get.v1"),
    worktreeId: RepositoryWorktreeIdSchema,
  }).strict(),
  PlanScopeSchema.extend({ operation: z.literal("integration.list.v1") }).strict(),
  ScopeSchema.extend({
    operation: z.literal("integration.get.v1"),
    integrationId: IntegrationAttemptIdSchema,
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("integration.diff.v1"),
    integrationId: IntegrationAttemptIdSchema,
    maximumBytes: z.number().int().min(1_024).max(200_000).default(100_000),
  }).strict(),
] as const;

export const RepositoryWorkspaceReadResponseSchemas = [
  z
    .object({
      operation: z.literal("repository.get.v1"),
      repository: RepositoryRecordSchema,
      binding: RepositoryProjectBindingSchema,
      registeredWorktree: RepositoryWorktreeRecordSchema,
      contextWorktree: RepositoryWorktreeRecordSchema,
      observedContextHead: GitObjectIdSchema,
      workingTreeState: z.enum(["clean", "dirty", "unknown"]),
      testProfiles: z.array(RepositoryTestProfileSchema).max(64),
    })
    .strict(),
  z
    .object({
      operation: z.literal("plan.workspace.list.v1"),
      plans: z.array(ProjectPlanRecordSchema),
    })
    .strict(),
  z
    .object({ operation: z.literal("plan.workspace.get.v1"), plan: ProjectPlanRecordSchema })
    .strict(),
  z
    .object({
      operation: z.literal("worktree.list.v1"),
      worktrees: z.array(RepositoryWorktreeRecordSchema),
    })
    .strict(),
  z
    .object({ operation: z.literal("worktree.get.v1"), worktree: RepositoryWorktreeRecordSchema })
    .strict(),
  z
    .object({
      operation: z.literal("integration.list.v1"),
      integrations: z.array(IntegrationAttemptSchema),
    })
    .strict(),
  z
    .object({ operation: z.literal("integration.get.v1"), integration: IntegrationAttemptSchema })
    .strict(),
  z
    .object({
      operation: z.literal("integration.diff.v1"),
      targetCommit: GitObjectIdSchema,
      candidateCommit: GitObjectIdSchema,
      changedFiles: z.array(z.string().min(1).max(4_096)).max(2_048),
      unifiedText: z.string().max(200_000),
      truncated: z.boolean(),
    })
    .strict(),
] as const;

export const RepositoryWorkspaceCommandRequestSchemas = [
  ScopeSchema.extend({
    operation: z.literal("plan.workspace.prepare.v1"),
    clientRequestId: ClientRequestIdSchema,
    displayName: z.string().min(1).max(200),
    expectedBaseHead: GitObjectIdSchema,
  }).strict(),
  PlanScopeSchema.extend({
    operation: z.literal("worktree.prepare.v1"),
    clientRequestId: ClientRequestIdSchema,
    parentWorktreeId: RepositoryWorktreeIdSchema,
    expectedParentHead: GitObjectIdSchema,
  }).strict(),
  PlanScopeSchema.extend({
    operation: z.literal("integration.prepare.v1"),
    clientRequestId: ClientRequestIdSchema,
    sourceWorktreeId: RepositoryWorktreeIdSchema,
    targetWorktreeId: RepositoryWorktreeIdSchema,
    expectedSourceHead: GitObjectIdSchema,
    expectedTargetHead: GitObjectIdSchema,
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("integration.test.v1"),
    clientRequestId: ClientRequestIdSchema,
    integrationId: IntegrationAttemptIdSchema,
    expectedRevision: RevisionSchema,
    profileId: z.string().min(1).max(160),
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("integration.review.v1"),
    clientRequestId: ClientRequestIdSchema,
    integrationId: IntegrationAttemptIdSchema,
    expectedRevision: RevisionSchema,
    accepted: z.boolean(),
    rationale: z.string().min(1).max(8_000),
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("integration.resolution.prepare.v1"),
    clientRequestId: ClientRequestIdSchema,
    integrationId: IntegrationAttemptIdSchema,
    expectedRevision: RevisionSchema,
  }).strict(),
  ScopeSchema.extend({
    operation: z.literal("integration.promote.v1"),
    clientRequestId: ClientRequestIdSchema,
    integrationId: IntegrationAttemptIdSchema,
    expectedRevision: RevisionSchema,
  }).strict(),
] as const;

export const RepositoryWorkspaceCommandResponseSchemas = [
  z
    .object({
      operation: z.literal("plan.workspace.prepare.v1"),
      plan: ProjectPlanRecordSchema,
      context: WorkContextDescriptorSchema,
      worktree: RepositoryWorktreeRecordSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("worktree.prepare.v1"),
      worktree: RepositoryWorktreeRecordSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("integration.prepare.v1"),
      integration: IntegrationAttemptSchema,
    })
    .strict(),
  z
    .object({ operation: z.literal("integration.test.v1"), integration: IntegrationAttemptSchema })
    .strict(),
  z
    .object({
      operation: z.literal("integration.review.v1"),
      integration: IntegrationAttemptSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("integration.resolution.prepare.v1"),
      integration: IntegrationAttemptSchema,
      work: ManagedWorkViewSchema,
    })
    .strict(),
  z
    .object({
      operation: z.literal("integration.promote.v1"),
      integration: IntegrationAttemptSchema,
    })
    .strict(),
] as const;

export {
  IntegrationAttemptIdSchema,
  ProjectPlanIdSchema,
  RepositoryIdSchema,
  RepositoryWorktreeIdSchema,
};
