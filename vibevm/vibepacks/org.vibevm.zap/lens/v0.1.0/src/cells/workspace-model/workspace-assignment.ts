/** Browser-safe managed workspace intent and assignment. @scope spec://org.vibevm.zap/lens/PROP-014#assignment */
import { z } from "zod";
import {
  GitObjectIdSchema,
  IntegrationAttemptIdSchema,
  ProjectPlanIdSchema,
  RepositoryIdSchema,
  RepositoryWorktreeIdSchema,
  RevisionSchema,
} from "../repository-model/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";

const OpaqueIdSchema = z
  .string()
  .min(1)
  .max(160)
  .regex(/^[A-Za-z0-9][A-Za-z0-9._:-]*$/);

export const ManagedWorkspaceRequestSchema = z.discriminatedUnion("mode", [
  z.object({ mode: z.literal("inherit") }).strict(),
  z
    .object({
      mode: z.literal("isolated_child"),
      parentWorktreeId: RepositoryWorktreeIdSchema,
      expectedParentHead: GitObjectIdSchema,
    })
    .strict(),
  z
    .object({
      mode: z.literal("integration_resolution"),
      integrationId: IntegrationAttemptIdSchema,
      expectedIntegrationRevision: RevisionSchema,
    })
    .strict(),
]);
export type ManagedWorkspaceRequest = z.infer<typeof ManagedWorkspaceRequestSchema>;

export const ManagedWorkspaceAssignmentSchema = z
  .object({
    assignmentId: OpaqueIdSchema,
    kind: z.enum(["legacy_registered", "inherited", "isolated_child", "integration_resolution"]),
    planId: ProjectPlanIdSchema.nullable(),
    projectId: ProjectIdSchema.nullable(),
    contextId: WorkContextIdSchema.nullable(),
    repositoryId: RepositoryIdSchema.nullable(),
    worktreeId: RepositoryWorktreeIdSchema.nullable(),
    executionHostId: OpaqueIdSchema.nullable(),
    basisCommit: GitObjectIdSchema.nullable(),
    initialHead: GitObjectIdSchema.nullable(),
    worktreeRevisionAtAssignment: RevisionSchema.nullable(),
    assignedByPrincipalId: OpaqueIdSchema,
    assignedAt: z.iso.datetime(),
  })
  .strict()
  .superRefine((assignment, context) => {
    const repositoryFields = [
      assignment.planId,
      assignment.projectId,
      assignment.contextId,
      assignment.repositoryId,
      assignment.worktreeId,
      assignment.executionHostId,
      assignment.basisCommit,
      assignment.initialHead,
      assignment.worktreeRevisionAtAssignment,
    ];
    const populated = repositoryFields.filter((field) => field !== null).length;
    if (assignment.kind === "legacy_registered" && populated !== 0)
      context.addIssue({
        code: "custom",
        message: "legacy assignments cannot claim a repository workspace",
      });
    if (assignment.kind !== "legacy_registered" && populated !== repositoryFields.length)
      context.addIssue({
        code: "custom",
        message: "repository assignments require the complete protected binding identity",
      });
  });
export type ManagedWorkspaceAssignment = z.infer<typeof ManagedWorkspaceAssignmentSchema>;
