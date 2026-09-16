/** Repository workspace client scope proof. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import assert from "node:assert/strict";
import test from "node:test";
import {
  IntegrationAttemptSchema,
  IntegrationAttemptIdSchema,
  ProjectPlanRecordSchema,
  RepositoryProjectBindingSchema,
  RepositoryRecordSchema,
  RepositoryWorktreeRecordSchema,
} from "../repository-model/index.ts";
import {
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceReadResponseSchema,
  type WorkspaceClientPort,
  type WorkspaceCommandRequest,
} from "../workspace-model/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { readWorkspaceCanvas } from "./canvas.ts";
import {
  commandRepositoryWorkspace,
  readIntegrationDiff,
  readRepositoryWorkspace,
} from "./repository.ts";

const projectId = ProjectIdSchema.parse("project.same-repository");
const contextA = WorkContextIdSchema.parse("context.plan-a");
const contextB = WorkContextIdSchema.parse("context.plan-b");
const commitA = "a".repeat(40);
const commitB = "b".repeat(40);
const commitC = "c".repeat(40);

test("repository client keeps same-project context selection and observed base exact", async () => {
  const commands: WorkspaceCommandRequest[] = [];
  const port = fixturePort(commands);
  const read = await readRepositoryWorkspace(port, projectId, contextB);
  assert.equal(read.ok, true);
  if (!read.ok) return;
  assert.equal(read.value.plans.length, 1);
  assert.equal(read.value.plans[0]?.displayName, "Plan B");
  assert.equal(read.value.registeredWorktree.contextId, contextA);
  assert.equal(read.value.contextWorktree.contextId, contextB);
  assert.deepEqual(read.value.worktrees.map((worktree) => worktree.worktreeId).sort(), [
    "worktree.plan-b",
    "worktree.registered",
  ]);
  assert.equal(read.value.observedContextHead, commitC);
  assert.equal(read.value.workingTreeState, "dirty");
  assert.equal(read.value.testProfiles[0]?.displayName, "Focused checks");
  const prepared = await commandRepositoryWorkspace(port, {
    operation: "plan.workspace.prepare.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.plan-c"),
    projectId,
    contextId: contextB,
    displayName: "Plan C",
    expectedBaseHead: read.value.observedContextHead,
  });
  assert.equal(prepared.ok, true);
  assert.equal(commands[0]?.operation, "plan.workspace.prepare.v1");
  if (commands[0]?.operation === "plan.workspace.prepare.v1")
    assert.equal(commands[0].expectedBaseHead, commitC);
  const diff = await readIntegrationDiff(
    port,
    projectId,
    contextB,
    IntegrationAttemptIdSchema.parse("integration.b"),
  );
  assert.equal(diff.ok, true);
  if (diff.ok) {
    assert.deepEqual(diff.value.changedFiles, ["src/search.ts"]);
    assert.match(diff.value.unifiedText, /index search/);
  }
});

test("canvas selection bound refuses nine exact contexts instead of silently truncating", async () => {
  const port = fixturePort([]);
  const read = await readWorkspaceCanvas(
    port,
    Array.from({ length: 9 }, (_, index) => ({
      projectId,
      contextId: WorkContextIdSchema.parse(`context.bound-${index}`),
      fallbackLabel: `Plan ${index}`,
    })),
  );
  assert.equal(read.ok, false);
  if (!read.ok) assert.match(read.error.message, /1 through 8/);
});

function fixturePort(commands: WorkspaceCommandRequest[]): WorkspaceClientPort {
  const registered = worktree("worktree.registered", contextA, null, "registered", commitA);
  const rootB = worktree("worktree.plan-b", contextB, "plan.b", "plan_root", commitB);
  const integration = IntegrationAttemptSchema.parse({
    integrationId: "integration.b",
    repositoryId: "repository.shared",
    executionHostId: "host.local",
    planId: "plan.b",
    sourceWorktreeId: rootB.worktreeId,
    targetWorktreeId: registered.worktreeId,
    integrationWorktreeId: "worktree.integration-b",
    expectedSourceHead: commitB,
    expectedTargetHead: commitA,
    candidateCommit: null,
    conflictPaths: [],
    state: "preparing",
    testEvidence: null,
    review: null,
    creatorPrincipalId: "principal.owner",
    revision: "1",
    createdAt: "2026-09-16T10:00:00.000Z",
  });
  const planA = plan("plan.a", contextA, "Plan A", registered.worktreeId);
  const planB = plan("plan.b", contextB, "Plan B", rootB.worktreeId);
  return {
    read: (request) => {
      const value =
        request.operation === "repository.get.v1"
          ? {
              operation: request.operation,
              repository: RepositoryRecordSchema.parse({
                repositoryId: "repository.shared",
                executionHostId: "host.local",
                displayName: "Shared repository",
                objectFormat: "sha1",
                revision: "1",
                createdAt: "2026-09-16T10:00:00.000Z",
              }),
              binding: RepositoryProjectBindingSchema.parse({
                repositoryId: "repository.shared",
                executionHostId: "host.local",
                projectId,
                registeredWorktreeId: registered.worktreeId,
                creatorPrincipalId: "principal.owner",
                revision: "1",
              }),
              registeredWorktree: registered,
              contextWorktree: rootB,
              observedContextHead: commitC,
              workingTreeState: "dirty",
              testProfiles: [
                {
                  profileId: "checks.focused",
                  displayName: "Focused checks",
                  descriptionMarkdown: "Run the configured focused checks.",
                },
              ],
            }
          : request.operation === "plan.workspace.list.v1"
            ? { operation: request.operation, plans: [planA, planB] }
            : request.operation === "worktree.list.v1"
              ? { operation: request.operation, worktrees: [rootB] }
              : request.operation === "integration.list.v1"
                ? { operation: request.operation, integrations: [integration] }
                : request.operation === "integration.diff.v1"
                  ? {
                      operation: request.operation,
                      targetCommit: commitA,
                      candidateCommit: commitC,
                      changedFiles: ["src/search.ts"],
                      unifiedText: "--- a/src/search.ts\n+++ b/src/search.ts\n+index search\n",
                      truncated: false,
                    }
                  : null;
      return value === null
        ? Promise.resolve(failure("unsupported fixture read"))
        : Promise.resolve({ ok: true, value: WorkspaceReadResponseSchema.parse(value) });
    },
    command: (request) => {
      commands.push(request);
      if (request.operation !== "plan.workspace.prepare.v1")
        return Promise.resolve(failure("unsupported fixture command"));
      return Promise.resolve({
        ok: true,
        value: WorkspaceCommandResponseSchema.parse({
          operation: request.operation,
          plan: plan("plan.c", contextB, request.displayName, rootB.worktreeId),
          context: {
            projectId,
            contextId: contextB,
            displayName: request.displayName,
            workspaceRef: "workspace.plan-c",
            branchLabel: "plan-c",
            revisionBinding: request.expectedBaseHead,
            planning: { state: "unavailable", reason: "Not configured" },
            coordinatorConversationId: "conversation.plan-c",
            revision: "1",
            createdAt: "2026-09-16T10:00:00.000Z",
            updatedAt: "2026-09-16T10:00:00.000Z",
          },
          worktree: rootB,
        }),
      });
    },
    events: () => Promise.resolve(failure("unused events")),
    subscribe: async function* () {
      await Promise.resolve();
      yield failure("unused subscription");
    },
  };
}

function worktree(
  worktreeId: string,
  contextId: WorkContextIdSchemaType,
  planId: string | null,
  kind: "registered" | "plan_root",
  headCommit: string,
) {
  return RepositoryWorktreeRecordSchema.parse({
    worktreeId,
    repositoryId: "repository.shared",
    executionHostId: "host.local",
    projectId,
    contextId,
    planId,
    kind,
    parentWorktreeId: null,
    branchRef: `refs/heads/${worktreeId}`,
    basisCommit: headCommit,
    headCommit,
    state: "ready",
    assignments: [],
    revision: "1",
    createdAt: "2026-09-16T10:00:00.000Z",
  });
}

function plan(
  planId: string,
  contextId: WorkContextIdSchemaType,
  displayName: string,
  root: string,
) {
  return ProjectPlanRecordSchema.parse({
    planId,
    repositoryId: "repository.shared",
    executionHostId: "host.local",
    projectId,
    contextId,
    displayName,
    rootWorktreeId: root,
    integrationTargetWorktreeId: "worktree.registered",
    algorithmBinding: { state: "pending" },
    state: "ready",
    creatorPrincipalId: "principal.owner",
    lastUpdatedByPrincipalId: "principal.owner",
    revision: "1",
    createdAt: "2026-09-16T10:00:00.000Z",
  });
}

type WorkContextIdSchemaType = ReturnType<typeof WorkContextIdSchema.parse>;
function failure(message: string) {
  return {
    ok: false as const,
    error: {
      code: "unavailable" as const,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-014#projection: ${message}`,
    },
  };
}
