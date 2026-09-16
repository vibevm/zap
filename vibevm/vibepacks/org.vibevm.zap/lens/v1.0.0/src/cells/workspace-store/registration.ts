/** Durable project and additive context registration. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { createHash } from "node:crypto";
import { isAbsolute, resolve } from "node:path";
import { z } from "zod";
import { DecimalSchema } from "../protocol/index.ts";
import {
  CoordinatorLaunchOptionSchema,
  ProjectDescriptorSchema,
  ProjectDetailSchema,
  WorkContextDescriptorSchema,
  type ProjectDetail,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import * as execution from "./execution.ts";
import { failure } from "./errors.ts";
import * as planContext from "./plan-context.ts";
import * as scope from "./scope.ts";
import type { WorkspaceState } from "./state.ts";
import { ProjectRowSchema } from "./store-model.ts";
import {
  ExistingContextPlanBindingSchema,
  ExistingContextPlanningBindingSchema,
  TrustedPlanContextRegistrationSchema,
  TrustedProjectRegistrationSchema,
  type ExistingContextPlanBinding,
  type ExistingContextPlanningBinding,
  type TrustedPlanContextRegistration,
  type TrustedProjectRegistration,
} from "./types.ts";

export function registerProject(
  state: WorkspaceState,
  rawInput: TrustedProjectRegistration,
): WorkspaceResult<ProjectDetail> {
  if (state.closed) return failure("closed", "workspace store is closed");
  const parsed = TrustedProjectRegistrationSchema.safeParse(rawInput);
  if (!parsed.success || !isAbsolute(parsed.data.protected.cwd))
    return failure("invalid_input", "trusted project registration requires an absolute cwd");
  const input = {
    ...parsed.data,
    protected: { ...parsed.data.protected, cwd: resolve(parsed.data.protected.cwd) },
  };
  try {
    return state.database.transaction(() => insertProject(state, input));
  } catch {
    return failure("storage_failure", "project registration transaction failed");
  }
}

export function registerPlanContext(
  state: WorkspaceState,
  rawInput: TrustedPlanContextRegistration,
) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const parsed = TrustedPlanContextRegistrationSchema.safeParse(rawInput);
  if (!parsed.success || !isAbsolute(parsed.data.protected.cwd))
    return failure("invalid_input", "trusted plan context requires an absolute prepared cwd");
  const input = {
    ...parsed.data,
    protected: { ...parsed.data.protected, cwd: resolve(parsed.data.protected.cwd) },
  };
  try {
    return state.database.transaction(() => planContext.registerPlanContext(state, input));
  } catch {
    return failure("storage_failure", "plan context registration transaction failed");
  }
}

export function bindExistingContextPlan(
  state: WorkspaceState,
  rawInput: ExistingContextPlanBinding,
) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = ExistingContextPlanBindingSchema.safeParse(rawInput);
  if (!input.success) return failure("invalid_input", "existing context plan binding is invalid");
  try {
    return state.database.transaction(() => planContext.bindExistingContextPlan(state, input.data));
  } catch {
    return failure("storage_failure", "existing context plan binding transaction failed");
  }
}

export function bindExistingContextPlanning(
  state: WorkspaceState,
  rawInput: ExistingContextPlanningBinding,
) {
  if (state.closed) return failure("closed", "workspace store is closed");
  const input = ExistingContextPlanningBindingSchema.safeParse(rawInput);
  if (!input.success) return failure("invalid_input", "context planning binding is invalid");
  try {
    return state.database.transaction(() =>
      planContext.bindExistingContextPlanning(state, input.data),
    );
  } catch {
    return failure("storage_failure", "context planning binding transaction failed");
  }
}

function insertProject(
  state: WorkspaceState,
  input: TrustedProjectRegistration,
): WorkspaceResult<ProjectDetail> {
  const publicContext = { ...input.context };
  delete publicContext.brokerScope;
  const digest = createHash("sha256")
    .update(JSON.stringify({ ...input, context: publicContext }))
    .digest("hex");
  const existing = state.database.get(
    `SELECT public_json, coordinator_launch_options_json, request_digest
     FROM workspace_projects WHERE registration_id = ?`,
    ProjectRowSchema,
    [input.registrationId],
  );
  if (existing !== null) return replayProject(state, input, existing, digest);
  if (state.projectExists(input.projectId))
    return failure("conflict", "project identity already exists under another registration");
  const now = state.now();
  const project = ProjectDescriptorSchema.parse({
    projectId: input.projectId,
    displayName: input.displayName,
    repositoryRootRefs: input.repositoryRootRefs,
    defaultContextId: input.context.contextId,
    actions: input.actions,
    revision: DecimalSchema.parse("1"),
    createdAt: now,
    updatedAt: now,
  });
  const context = WorkContextDescriptorSchema.parse({
    contextId: input.context.contextId,
    displayName: input.context.displayName,
    workspaceRef: input.context.workspaceRef,
    branchLabel: input.context.branchLabel,
    revisionBinding: input.context.revisionBinding,
    planning: input.context.planning,
    coordinatorConversationId: input.context.coordinatorConversationId,
    projectId: input.projectId,
    revision: DecimalSchema.parse("1"),
    createdAt: now,
    updatedAt: now,
  });
  state.database.run(
    `INSERT INTO workspace_projects(
       project_id, public_json, coordinator_launch_options_json, default_context_id,
       protected_cwd, protected_profile_ref, registration_id, request_digest
     ) VALUES(?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      project.projectId,
      state.json(project),
      state.json(input.coordinatorLaunchOptions),
      project.defaultContextId,
      input.protected.cwd,
      input.protected.launchProfileRef,
      input.registrationId,
      digest,
    ],
  );
  state.database.run(
    `INSERT INTO workspace_contexts(
       context_id, project_id, public_json, protected_cwd, protected_profile_ref
     ) VALUES(?, ?, ?, ?, ?)`,
    [
      context.contextId,
      context.projectId,
      state.json(context),
      input.protected.cwd,
      input.protected.launchProfileRef,
    ],
  );
  registerContextLaunchOptions(
    state,
    project.projectId,
    context.contextId,
    input.coordinatorLaunchOptions,
    input.registrationId,
    digest,
  );
  if (input.context.brokerScope !== undefined) {
    const mapped = scope.registerAgentScope(
      state,
      input.context.brokerScope,
      project.projectId,
      context.contextId,
    );
    if (!mapped.ok) return mapped;
  }
  execution.initializeProjectExecution(state, project.projectId, context.contextId, now);
  state.appendHistory({
    projectId: project.projectId,
    contextId: context.contextId,
    kind: "project.registered",
    source: "lens",
    actorId: null,
    occurrenceAt: now,
    sourceEventId: `registration:${input.registrationId}`,
    sourceSequence: null,
    correlationId: project.projectId,
    causationId: null,
    planProvenance: null,
    payload: { projectId: project.projectId, contextId: context.contextId },
  });
  return {
    ok: true,
    value: ProjectDetailSchema.parse({
      project,
      contexts: [context],
      coordinator: null,
      coordinatorLaunchOptions: input.coordinatorLaunchOptions,
    }),
  };
}

function replayProject(
  state: WorkspaceState,
  input: TrustedProjectRegistration,
  existing: z.infer<typeof ProjectRowSchema>,
  digest: string,
): WorkspaceResult<ProjectDetail> {
  if (existing.request_digest !== digest)
    return failure("idempotency_conflict", "project registration identity changed content");
  const context = scope.context(state, input.projectId, input.context.contextId);
  if (context === null) return failure("storage_failure", "registered project context is missing");
  if (input.context.brokerScope !== undefined) {
    const mapped = scope.registerAgentScope(
      state,
      input.context.brokerScope,
      input.projectId,
      input.context.contextId,
    );
    if (!mapped.ok) return mapped;
  }
  execution.initializeProjectExecution(
    state,
    input.projectId,
    input.context.contextId,
    state.now(),
  );
  registerContextLaunchOptions(
    state,
    input.projectId,
    input.context.contextId,
    input.coordinatorLaunchOptions,
    input.registrationId,
    digest,
  );
  return {
    ok: true,
    value: ProjectDetailSchema.parse({
      project: state.parse(existing.public_json, ProjectDescriptorSchema),
      contexts: [context],
      coordinator: null,
      coordinatorLaunchOptions: z
        .array(CoordinatorLaunchOptionSchema)
        .parse(JSON.parse(existing.coordinator_launch_options_json)),
    }),
  };
}

function registerContextLaunchOptions(
  state: WorkspaceState,
  projectId: string,
  contextId: string,
  options: readonly z.infer<typeof CoordinatorLaunchOptionSchema>[],
  registrationId: string,
  requestDigest: string,
): void {
  state.database.run(
    `INSERT OR IGNORE INTO workspace_context_launch_options(
       context_id, project_id, launch_options_json, registration_id, request_digest
     ) VALUES(?, ?, ?, ?, ?)`,
    [contextId, projectId, state.json(options), registrationId, requestDigest],
  );
}
