/** Authenticated integration-resolution work composition. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import { randomUUID } from "node:crypto";
import type { IntegrationAttempt } from "../repository-model/index.ts";
import type { ManagedAgentBackend } from "../managed-work/index.ts";
import type { RepositoryWorkspaceService } from "../repository-workspaces/index.ts";
import {
  managedWorkView,
  type RepositoryWorkspaceCommandRequest,
} from "../workspace-service/index.ts";
import type {
  WorkspaceAccessContext,
  WorkspaceCommandResponse,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import { createRepositoryManagedWorkspaceProvisioningPort } from "./repository-managed.ts";

export interface RepositoryResolutionPreparationPort {
  prepare(input: {
    readonly access: WorkspaceAccessContext;
    readonly request: Extract<
      RepositoryWorkspaceCommandRequest,
      { operation: "integration.resolution.prepare.v1" }
    >;
    readonly integration: IntegrationAttempt;
  }): Promise<WorkspaceResult<WorkspaceCommandResponse>>;
}

export interface RuntimeRepositoryResolution {
  readonly preparation: RepositoryResolutionPreparationPort;
  authorize(input: {
    readonly access: WorkspaceAccessContext;
    readonly integration: IntegrationAttempt;
    readonly clientRequestId: string;
    readonly planId: string;
  }):
    | { readonly ok: true; readonly value: null }
    | { readonly ok: false; readonly message: string };
}

export function createRuntimeRepositoryResolution(input: {
  readonly managedWork: () => ManagedAgentBackend | undefined;
}): RuntimeRepositoryResolution {
  const activeGrants = new Set<string>();
  return {
    authorize(candidate) {
      return activeGrants.has(grantKey(candidate))
        ? { ok: true, value: null }
        : { ok: false, message: "integration resolution work was not explicitly prepared" };
    },
    preparation: {
      async prepare(prepared) {
        const backend = input.managedWork();
        if (backend === undefined)
          return failure("unavailable", "managed integration resolution is not configured");
        const grant = {
          access: prepared.access,
          integration: prepared.integration,
          clientRequestId: prepared.request.clientRequestId,
          planId: prepared.integration.planId,
        };
        const key = grantKey(grant);
        activeGrants.add(key);
        try {
          const created = await backend.prepare(prepared.access, {
            clientRequestId: prepared.request.clientRequestId,
            projectId: prepared.request.projectId,
            contextId: prepared.request.contextId,
            planId: prepared.integration.planId,
            workspaceRequest: {
              mode: "integration_resolution",
              integrationId: prepared.integration.integrationId,
              expectedIntegrationRevision: prepared.request.expectedRevision,
            },
            selection: { mode: "project_policy" },
            goal: `Resolve the recorded conflicts for integration ${prepared.integration.integrationId}.`,
            expectedResult:
              "Commit the conflict resolution in the assigned integration worktree and report the result for human review.",
            targetRefs: [
              {
                projectId: prepared.request.projectId,
                contextId: prepared.request.contextId,
                domain: "integration",
                ref: prepared.integration.integrationId,
              },
            ],
            contextRefs: [],
            parentTaskId: null,
            parentRunId: null,
            projectedParentActorId: prepared.access.actorId,
            sourceBasisRef: `integration:${prepared.integration.integrationId}:${prepared.integration.revision}`,
            planRevision: null,
            depth: 0,
            budgets: { maximumTurns: 32, wallTimeMs: 3_600_000 },
          });
          return created.ok
            ? {
                ok: true,
                value: {
                  operation: prepared.request.operation,
                  integration: prepared.integration,
                  work: managedWorkView(created.value),
                },
              }
            : failure(mapCode(created.error.code), created.error.message);
        } finally {
          activeGrants.delete(key);
        }
      },
    },
  };
}

export function createRepositoryResolutionComposition(input: {
  readonly repositories: RepositoryWorkspaceService;
  readonly executionHostId: string;
  readonly backend: () => ManagedAgentBackend | undefined;
}) {
  const authorization = createRuntimeRepositoryResolution({ managedWork: input.backend });
  return {
    resolution: authorization.preparation,
    workspaces: createRepositoryManagedWorkspaceProvisioningPort({
      repositories: input.repositories,
      executionHostId: input.executionHostId,
      id: () => `workspace.assignment.${randomUUID()}`,
      now: () => new Date().toISOString(),
      authorizeIntegrationResolution: (candidate) => authorization.authorize(candidate),
    }),
  };
}

function grantKey(input: {
  readonly access: WorkspaceAccessContext;
  readonly integration: IntegrationAttempt;
  readonly clientRequestId: string;
  readonly planId: string;
}): string {
  return [
    input.access.principalId,
    input.integration.integrationId,
    input.integration.revision,
    input.clientRequestId,
    input.planId,
  ].join("\u0000");
}

function mapCode(code: string): "invalid_input" | "forbidden" | "conflict" | "unavailable" {
  if (code === "invalid_input") return "invalid_input";
  if (code === "forbidden") return "forbidden";
  return code === "conflict" ? "conflict" : "unavailable";
}

function failure(
  code: "invalid_input" | "forbidden" | "conflict" | "unavailable",
  message: string,
): WorkspaceResult<never> {
  return { ok: false, error: { code, message } };
}
