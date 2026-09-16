/** Additive trusted plan-context registration. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import { createHash } from "node:crypto";
import { z } from "zod";
import { DecimalSchema } from "../protocol/index.ts";
import {
  CoordinatorLaunchOptionSchema,
  ProjectDescriptorSchema,
  WorkContextIdSchema,
  WorkContextDescriptorSchema,
} from "../workspace-model/index.ts";
import { failure } from "./errors.ts";
import * as execution from "./execution.ts";
import * as scope from "./scope.ts";
import type { WorkspaceState } from "./state.ts";
import {
  ExistingContextPlanBindingSchema,
  ExistingContextPlanningBindingSchema,
  RegisteredPlanContextSchema,
  type ExistingContextPlanBinding,
  type ExistingContextPlanningBinding,
  type TrustedPlanContextRegistration,
} from "./types.ts";

const ExistingSchema = z.object({
  request_digest: z.string(),
  plan_json: z.string(),
  root_worktree_json: z.string(),
  context_id: z.string(),
});
const ProjectSchema = z.object({ public_json: z.string() });
const CountSchema = z.object({ value: z.bigint() });

export function registerPlanContext(state: WorkspaceState, input: TrustedPlanContextRegistration) {
  const publicContext = { ...input.context };
  delete publicContext.brokerScope;
  const digest = createHash("sha256")
    .update(JSON.stringify({ ...input, context: publicContext }))
    .digest("hex");
  const existing = state.database.get(
    `SELECT request_digest, plan_json, root_worktree_json, context_id
     FROM workspace_plan_contexts WHERE registration_id = ?`,
    ExistingSchema,
    [input.registrationId],
  );
  if (existing !== null) {
    if (existing.request_digest !== digest)
      return failure("idempotency_conflict", "plan context registration identity changed content");
    const context = scope.context(
      state,
      input.projectId,
      WorkContextIdSchema.parse(existing.context_id),
    );
    if (context === null) return failure("storage_failure", "registered plan context is missing");
    const plan: unknown = JSON.parse(existing.plan_json);
    const rootWorktree: unknown = JSON.parse(existing.root_worktree_json);
    return {
      ok: true as const,
      value: RegisteredPlanContextSchema.parse({
        plan,
        context,
        rootWorktree,
        coordinatorLaunchOptions: input.coordinatorLaunchOptions,
      }),
    };
  }
  const projectRow = state.database.get(
    "SELECT public_json FROM workspace_projects WHERE project_id = ?",
    ProjectSchema,
    [input.projectId],
  );
  if (projectRow === null) return failure("not_found", "base project does not exist");
  const collision = state.database.get(
    `SELECT (
       (SELECT COUNT(*) FROM workspace_contexts WHERE context_id = ?) +
       (SELECT COUNT(*) FROM workspace_plan_contexts WHERE plan_id = ?)
     ) AS value`,
    CountSchema,
    [input.plan.contextId, input.plan.planId],
  );
  if (collision !== null && collision.value > 0n)
    return failure("conflict", "plan or context identity already exists");
  const now = state.now();
  const context = WorkContextDescriptorSchema.parse({
    contextId: input.plan.contextId,
    projectId: input.projectId,
    displayName: input.context.displayName,
    workspaceRef: input.context.workspaceRef,
    branchLabel: input.context.branchLabel,
    revisionBinding: input.context.revisionBinding,
    planning: input.context.planning,
    coordinatorConversationId: input.context.coordinatorConversationId,
    planId: input.plan.planId,
    repositoryId: input.plan.repositoryId,
    rootWorktreeId: input.rootWorktree.worktreeId,
    revision: DecimalSchema.parse("1"),
    createdAt: now,
    updatedAt: now,
  });
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
  state.database.run(
    `INSERT INTO workspace_context_launch_options(
       context_id, project_id, launch_options_json, registration_id, request_digest
     ) VALUES(?, ?, ?, ?, ?)`,
    [
      context.contextId,
      context.projectId,
      state.json(input.coordinatorLaunchOptions),
      input.registrationId,
      digest,
    ],
  );
  state.database.run(
    `INSERT INTO workspace_plan_contexts(
       plan_id, project_id, context_id, root_worktree_id, registration_id,
       request_digest, plan_json, root_worktree_json
     ) VALUES(?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      input.plan.planId,
      input.projectId,
      context.contextId,
      input.rootWorktree.worktreeId,
      input.registrationId,
      digest,
      state.json(input.plan),
      state.json(input.rootWorktree),
    ],
  );
  if (input.context.brokerScope !== undefined) {
    const mapped = scope.registerAgentScope(
      state,
      input.context.brokerScope,
      input.projectId,
      context.contextId,
    );
    if (!mapped.ok) return mapped;
  }
  execution.initializeProjectExecution(state, input.projectId, context.contextId, now);
  const project = state.parse(projectRow.public_json, ProjectDescriptorSchema);
  const updatedProject = ProjectDescriptorSchema.parse({
    ...project,
    revision: DecimalSchema.parse(String(BigInt(project.revision) + 1n)),
    updatedAt: now,
  });
  state.database.run("UPDATE workspace_projects SET public_json = ? WHERE project_id = ?", [
    state.json(updatedProject),
    input.projectId,
  ]);
  state.appendHistory({
    projectId: input.projectId,
    contextId: context.contextId,
    kind: "plan.workspace.registered",
    source: "lens",
    actorId: null,
    occurrenceAt: now,
    sourceEventId: `registration:${input.registrationId}`,
    sourceSequence: null,
    correlationId: input.plan.planId,
    causationId: null,
    planProvenance: null,
    payload: {
      planId: input.plan.planId,
      contextId: context.contextId,
      rootWorktreeId: input.rootWorktree.worktreeId,
    },
  });
  return {
    ok: true as const,
    value: RegisteredPlanContextSchema.parse({
      plan: input.plan,
      context,
      rootWorktree: input.rootWorktree,
      coordinatorLaunchOptions: z
        .array(CoordinatorLaunchOptionSchema)
        .parse(input.coordinatorLaunchOptions),
    }),
  };
}

export function bindExistingContextPlan(state: WorkspaceState, raw: ExistingContextPlanBinding) {
  const input = ExistingContextPlanBindingSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "existing context plan binding is invalid");
  const value = input.data;
  if (
    value.plan.projectId !== value.projectId ||
    value.plan.contextId !== value.contextId ||
    value.plan.rootWorktreeId !== value.rootWorktree.worktreeId ||
    value.plan.repositoryId !== value.rootWorktree.repositoryId ||
    value.rootWorktree.projectId !== value.projectId ||
    value.rootWorktree.contextId !== value.contextId ||
    value.plan.state !== "ready" ||
    value.rootWorktree.state !== "ready"
  )
    return failure("invalid_input", "adopted plan identities are inconsistent");
  const current = scope.context(state, value.projectId, value.contextId);
  if (current === null) return failure("not_found", "existing project context does not exist");
  if (current.planId !== null || current.repositoryId !== null || current.rootWorktreeId !== null)
    return current.planId === value.plan.planId &&
      current.repositoryId === value.plan.repositoryId &&
      current.rootWorktreeId === value.rootWorktree.worktreeId
      ? { ok: true as const, value: current }
      : failure("conflict", "existing context is already bound to another plan workspace");
  const now = state.now();
  const next = WorkContextDescriptorSchema.parse({
    ...current,
    planId: value.plan.planId,
    repositoryId: value.plan.repositoryId,
    rootWorktreeId: value.rootWorktree.worktreeId,
    revision: DecimalSchema.parse(String(BigInt(current.revision) + 1n)),
    updatedAt: now,
  });
  state.database.run(
    "UPDATE workspace_contexts SET public_json = ? WHERE project_id = ? AND context_id = ?",
    [state.json(next), value.projectId, value.contextId],
  );
  state.appendHistory({
    projectId: value.projectId,
    contextId: value.contextId,
    kind: "plan.workspace.adopted",
    source: "lens",
    actorId: null,
    occurrenceAt: now,
    sourceEventId: `plan-adopted:${value.plan.planId}`,
    sourceSequence: null,
    correlationId: value.plan.planId,
    causationId: null,
    planProvenance: null,
    payload: {
      planId: value.plan.planId,
      repositoryId: value.plan.repositoryId,
      rootWorktreeId: value.rootWorktree.worktreeId,
    },
  });
  return { ok: true as const, value: next };
}

export function bindExistingContextPlanning(
  state: WorkspaceState,
  raw: ExistingContextPlanningBinding,
) {
  const input = ExistingContextPlanningBindingSchema.safeParse(raw);
  if (!input.success) return failure("invalid_input", "context planning binding is invalid");
  const current = scope.context(state, input.data.projectId, input.data.contextId);
  if (current === null) return failure("not_found", "project context does not exist");
  if (JSON.stringify(current.planning) === JSON.stringify(input.data.planning))
    return { ok: true as const, value: current };
  if (current.planning.state === "configured" && input.data.planning.state === "configured")
    return failure("conflict", "project context is already bound to another planning source");
  if (current.revision !== input.data.expectedRevision)
    return failure("conflict", "project context revision changed");
  const now = state.now();
  const next = WorkContextDescriptorSchema.parse({
    ...current,
    planning: input.data.planning,
    revision: DecimalSchema.parse(String(BigInt(current.revision) + 1n)),
    updatedAt: now,
  });
  state.database.run(
    "UPDATE workspace_contexts SET public_json = ? WHERE project_id = ? AND context_id = ?",
    [state.json(next), input.data.projectId, input.data.contextId],
  );
  state.appendHistory({
    projectId: input.data.projectId,
    contextId: input.data.contextId,
    kind: "plan.workspace.planning-bound",
    source: "lens",
    actorId: null,
    occurrenceAt: now,
    sourceEventId: `planning-bound:${input.data.contextId}:${next.revision}`,
    sourceSequence: null,
    correlationId: next.planId,
    causationId: null,
    planProvenance: null,
    payload: input.data.planning,
  });
  return { ok: true as const, value: next };
}
