/** Repository-workspace service ports. @scope spec://org.vibevm.zap/lens/PROP-014#root */
import { z } from "zod";
import {
  AlgorithmBindingSchema,
  GitObjectIdSchema,
  IntegrationAttemptSchema,
  ProjectPlanRecordSchema,
  RepositoryProjectBindingSchema,
  RepositoryRecordSchema,
  RepositoryWorktreeRecordSchema,
  RevisionSchema,
  type IntegrationAttempt,
  type ProjectPlanRecord,
  type RepositoryProjectBinding,
  type RepositoryRecord,
  type RepositoryWorktreeRecord,
} from "../repository-model/index.ts";

const IdSchema = z
  .string()
  .min(1)
  .max(160)
  .regex(/^[A-Za-z0-9][A-Za-z0-9._:-]*$/);
const RequestSchema = z
  .object({ requestId: IdSchema, principalId: IdSchema, executionHostId: IdSchema })
  .strict();

export const RegisterRepositoryRequestSchema = RequestSchema.extend({
  projectId: IdSchema,
  contextId: IdSchema,
  trustedProjectCwd: z.string().min(1).max(32_000),
  displayName: z.string().min(1).max(200),
}).strict();
export type RegisterRepositoryRequest = z.infer<typeof RegisterRepositoryRequestSchema>;

export const PreparePlanRequestSchema = RequestSchema.extend({
  projectId: IdSchema,
  contextId: IdSchema,
  planId: IdSchema,
  displayName: z.string().min(1).max(200),
  baseWorktreeId: IdSchema,
  expectedBaseHead: GitObjectIdSchema,
  algorithmBinding: AlgorithmBindingSchema.default({ state: "pending" }),
}).strict();
export type PreparePlanRequest = z.infer<typeof PreparePlanRequestSchema>;
export const AdoptRegisteredPlanRequestSchema = RequestSchema.extend({
  projectId: IdSchema,
  contextId: IdSchema,
  planId: IdSchema,
  displayName: z.string().min(1).max(200),
  registeredWorktreeId: IdSchema,
  expectedHead: GitObjectIdSchema,
  algorithmBinding: AlgorithmBindingSchema.default({ state: "pending" }),
}).strict();
export type AdoptRegisteredPlanRequest = z.infer<typeof AdoptRegisteredPlanRequestSchema>;

export const PrepareChildRequestSchema = RequestSchema.extend({
  projectId: IdSchema,
  contextId: IdSchema,
  planId: IdSchema,
  parentWorktreeId: IdSchema,
  expectedParentHead: GitObjectIdSchema,
}).strict();
export type PrepareChildRequest = z.infer<typeof PrepareChildRequestSchema>;

export const UpdateAlgorithmBindingRequestSchema = RequestSchema.extend({
  planId: IdSchema,
  expectedRevision: RevisionSchema,
  algorithmBinding: AlgorithmBindingSchema,
}).strict();
export type UpdateAlgorithmBindingRequest = z.infer<typeof UpdateAlgorithmBindingRequestSchema>;

export const RecordWorktreeHeadRequestSchema = RequestSchema.extend({
  worktreeId: IdSchema,
  expectedRevision: RevisionSchema,
  expectedOldHead: GitObjectIdSchema,
  newHead: GitObjectIdSchema,
}).strict();
export type RecordWorktreeHeadRequest = z.infer<typeof RecordWorktreeHeadRequestSchema>;

export const AssignWorktreeRequestSchema = RequestSchema.extend({
  worktreeId: IdSchema,
  expectedRevision: RevisionSchema,
  assignmentId: IdSchema,
  attemptId: IdSchema,
  basisCommit: GitObjectIdSchema,
  actorId: IdSchema.nullable(),
  taskId: IdSchema.nullable(),
  runId: IdSchema.nullable(),
  semanticTargetRefs: z
    .array(
      z
        .object({
          projectId: IdSchema,
          contextId: IdSchema,
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
}).strict();
export type AssignWorktreeRequest = z.infer<typeof AssignWorktreeRequestSchema>;
export const AssignIntegrationWorktreeRequestSchema = AssignWorktreeRequestSchema.extend({
  integrationId: IdSchema,
  expectedIntegrationRevision: RevisionSchema,
}).strict();
export type AssignIntegrationWorktreeRequest = z.infer<
  typeof AssignIntegrationWorktreeRequestSchema
>;

export const PrepareIntegrationRequestSchema = RequestSchema.extend({
  integrationId: IdSchema,
  planId: IdSchema,
  sourceWorktreeId: IdSchema,
  targetWorktreeId: IdSchema,
  expectedSourceHead: GitObjectIdSchema,
  expectedTargetHead: GitObjectIdSchema,
}).strict();
export type PrepareIntegrationRequest = z.infer<typeof PrepareIntegrationRequestSchema>;

export const RecordResolutionRequestSchema = RequestSchema.extend({
  integrationId: IdSchema,
  expectedRevision: RevisionSchema,
  resolutionCommit: GitObjectIdSchema,
}).strict();
export type RecordResolutionRequest = z.infer<typeof RecordResolutionRequestSchema>;

export const RunIntegrationTestRequestSchema = RequestSchema.extend({
  integrationId: IdSchema,
  expectedRevision: RevisionSchema,
  profileId: IdSchema,
}).strict();
export type RunIntegrationTestRequest = z.infer<typeof RunIntegrationTestRequestSchema>;

export const RecordIntegrationReviewRequestSchema = RequestSchema.extend({
  integrationId: IdSchema,
  expectedRevision: RevisionSchema,
  accepted: z.boolean(),
  rationale: z.string().min(1).max(8_000),
}).strict();
export type RecordIntegrationReviewRequest = z.infer<typeof RecordIntegrationReviewRequestSchema>;

export const PromoteIntegrationRequestSchema = RequestSchema.extend({
  integrationId: IdSchema,
  expectedRevision: RevisionSchema,
}).strict();
export type PromoteIntegrationRequest = z.infer<typeof PromoteIntegrationRequestSchema>;

export type RepositoryWorkspaceErrorCode =
  | "invalid_input"
  | "conflict"
  | "unavailable"
  | "host_unavailable"
  | "dirty"
  | "stale"
  | "conflicted"
  | "not_ready"
  | "denied";
export type RepositoryWorkspaceResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: { readonly code: RepositoryWorkspaceErrorCode; readonly message: string };
    };

export interface RegisteredRepository {
  readonly repository: RepositoryRecord;
  readonly binding: RepositoryProjectBinding;
  readonly worktree: RepositoryWorktreeRecord;
}
export interface PreparedPlan {
  readonly plan: ProjectPlanRecord;
  readonly worktree: RepositoryWorktreeRecord;
}
/** Trusted server-only launch binding; never project this path to browser clients. */
export interface ResolveExecutionWorkspaceRequest {
  readonly projectId: string;
  readonly contextId: string;
  readonly worktreeId: string;
  readonly executionHostId: string;
  readonly expectedRevision?: string;
}
export interface ResolveAssignedWorkspaceRequest extends ResolveExecutionWorkspaceRequest {
  readonly assignmentId: string;
  readonly attemptId: string;
  readonly mode: "initial" | "resume";
  readonly expectedInitialHead: string;
}
export interface ResolveIntegrationWorkspaceRequest {
  readonly integrationId: string;
  readonly projectId: string;
  readonly contextId: string;
  readonly worktreeId: string;
  readonly executionHostId: string;
  readonly expectedRevision: string;
}
export interface TrustedExecutionWorkspace {
  readonly worktree: RepositoryWorktreeRecord;
  readonly projectCwd: string;
  readonly verifiedHead: string;
  readonly dirty: boolean;
}
export interface ObserveWorktreeRequest {
  readonly worktreeId: string;
  readonly executionHostId: string;
  readonly projectId: string;
  readonly contextId: string;
}
export interface TrustedWorktreeObservation {
  readonly worktree: RepositoryWorktreeRecord;
  readonly currentHead: string;
  readonly dirty: boolean;
}
export interface IntegrationDiffRequest {
  readonly integrationId: string;
  readonly executionHostId: string;
  readonly projectId: string;
  readonly contextId: string;
  readonly maximumBytes: number;
}
export interface IntegrationDiff {
  readonly targetCommit: string;
  readonly candidateCommit: string;
  readonly changedFiles: readonly string[];
  readonly unifiedText: string;
  readonly truncated: boolean;
}
export interface RepositoryWriterLeaseView {
  readonly leaseId: string;
  readonly epoch: string;
  readonly target: {
    readonly repositoryId: string;
    readonly executionHostId: string;
    readonly planId: string;
    readonly targetWorktreeId: string;
    readonly expectedHead: string;
    readonly operationId: string;
  };
}

export interface RepositoryWorkspaceService {
  registerRepository(
    input: RegisterRepositoryRequest,
  ): Promise<RepositoryWorkspaceResult<RegisteredRepository>>;
  preparePlanRoot(input: PreparePlanRequest): Promise<RepositoryWorkspaceResult<PreparedPlan>>;
  adoptRegisteredPlan(
    input: AdoptRegisteredPlanRequest,
  ): Promise<RepositoryWorkspaceResult<PreparedPlan>>;
  prepareChildWorktree(
    input: PrepareChildRequest,
  ): Promise<RepositoryWorkspaceResult<RepositoryWorktreeRecord>>;
  updateAlgorithmBinding(
    input: UpdateAlgorithmBindingRequest,
  ): Promise<RepositoryWorkspaceResult<ProjectPlanRecord>>;
  recordWorktreeHead(
    input: RecordWorktreeHeadRequest,
  ): Promise<RepositoryWorkspaceResult<RepositoryWorktreeRecord>>;
  assignWorktree(
    input: AssignWorktreeRequest,
  ): Promise<RepositoryWorkspaceResult<RepositoryWorktreeRecord>>;
  assignIntegrationWorktree(
    input: AssignIntegrationWorktreeRequest,
  ): Promise<RepositoryWorkspaceResult<RepositoryWorktreeRecord>>;
  prepareIntegration(
    input: PrepareIntegrationRequest,
  ): Promise<RepositoryWorkspaceResult<IntegrationAttempt>>;
  recordResolution(
    input: RecordResolutionRequest,
  ): Promise<RepositoryWorkspaceResult<IntegrationAttempt>>;
  runIntegrationTest(
    input: RunIntegrationTestRequest,
  ): Promise<RepositoryWorkspaceResult<IntegrationAttempt>>;
  recordIntegrationReview(
    input: RecordIntegrationReviewRequest,
  ): Promise<RepositoryWorkspaceResult<IntegrationAttempt>>;
  promoteIntegration(
    input: PromoteIntegrationRequest,
  ): Promise<RepositoryWorkspaceResult<IntegrationAttempt>>;
  getRepository(repositoryId: string): RepositoryWorkspaceResult<RepositoryRecord>;
  getPlan(planId: string): RepositoryWorkspaceResult<ProjectPlanRecord>;
  getWorktree(worktreeId: string): RepositoryWorkspaceResult<RepositoryWorktreeRecord>;
  getIntegration(integrationId: string): RepositoryWorkspaceResult<IntegrationAttempt>;
  listPlans(projectId: string): RepositoryWorkspaceResult<readonly ProjectPlanRecord[]>;
  listWorktrees(planId: string): RepositoryWorkspaceResult<readonly RepositoryWorktreeRecord[]>;
  listIntegrations(planId: string): RepositoryWorkspaceResult<readonly IntegrationAttempt[]>;
  resolveExecutionWorkspace(
    input: ResolveExecutionWorkspaceRequest,
  ): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>>;
  resolveAssignedWorkspace(
    input: ResolveAssignedWorkspaceRequest,
  ): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>>;
  resolveIntegrationWorkspace(
    input: ResolveIntegrationWorkspaceRequest,
  ): Promise<RepositoryWorkspaceResult<TrustedExecutionWorkspace>>;
  observeWorktree(
    input: ObserveWorktreeRequest,
  ): Promise<RepositoryWorkspaceResult<TrustedWorktreeObservation>>;
  readIntegrationDiff(
    input: IntegrationDiffRequest,
  ): Promise<RepositoryWorkspaceResult<IntegrationDiff>>;
  currentWriterLease(targetWorktreeId: string): RepositoryWriterLeaseView | null;
  close(): void;
}

export const RegisteredRepositorySchema = z
  .object({
    repository: RepositoryRecordSchema,
    binding: RepositoryProjectBindingSchema,
    worktree: RepositoryWorktreeRecordSchema,
  })
  .strict();
export const PreparedPlanSchema = z
  .object({ plan: ProjectPlanRecordSchema, worktree: RepositoryWorktreeRecordSchema })
  .strict();
export const IntegrationResultSchema = IntegrationAttemptSchema;
