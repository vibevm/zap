/** Workspace read feature routing. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import type { WorkspaceAccessContext, WorkspaceClientPort } from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { ManagedAgentBackend } from "../managed-work/index.ts";
import type { ModelPolicyService } from "../model-policy-service/index.ts";
import { reconcileManagedAgentNetwork } from "./managed-agent.ts";
import { readModelPolicy } from "./model-policy.ts";
import { listTerminals, readTerminal } from "./terminal.ts";
import type { WorkspaceManagedTerminalPort } from "./types.ts";
import { readManagedWork } from "./managed-work.ts";
import type { AnnotationService } from "../workspace-annotations/index.ts";
import type { AnnotationReadRequest } from "../workspace-model/index.ts";
import { workspaceFailure } from "./errors.ts";

export function readWorkspace(
  store: WorkspaceStore,
  planning: WorkspacePlanningFeature | undefined,
  terminals: WorkspaceManagedTerminalPort | undefined,
  modelPolicy: ModelPolicyService | undefined,
  managedWork: ManagedAgentBackend | undefined,
  annotations: AnnotationService | undefined,
  access: WorkspaceAccessContext,
  request: Parameters<WorkspaceClientPort["read"]>[0],
): ReturnType<WorkspaceClientPort["read"]> {
  if (isAnnotationRead(request))
    return annotations === undefined
      ? workspaceFailure("unsupported_operation", "annotation service is not configured")
      : annotations.read(access, request);
  if (request.operation === "project.snapshot.v1" && planning !== undefined)
    return planning.snapshot(access, request.projectId, request.contextId);
  if (request.operation === "terminal.list.v1") return listTerminals(terminals, access, request);
  if (request.operation === "terminal.output.page.v1")
    return readTerminal(terminals, access, request);
  if (
    request.operation === "managed-work.profile.list.v1" ||
    request.operation === "managed-work.get.v1" ||
    request.operation === "managed-work.list.v1"
  )
    return readManagedWork(managedWork, access, request);
  if (
    request.operation === "model-policy.get.v1" ||
    request.operation === "model-policy.preview.v1" ||
    request.operation === "model-policy.history.v1" ||
    request.operation === "model-selection.get.v1"
  )
    return readModelPolicy(modelPolicy, access, request);
  if (request.operation === "agent.network.v1") {
    const stored = store.read(access, request);
    if (!stored.ok || stored.value.operation !== "agent.network.v1") return stored;
    const listed = terminals?.list(access, request.projectId, request.contextId);
    const network = reconcileManagedAgentNetwork(
      stored.value.network,
      listed?.ok ? { state: "available", terminals: listed.value } : { state: "unavailable" },
    );
    return { ok: true, value: { operation: request.operation, network } };
  }
  return store.read(access, request);
}

function isAnnotationRead(
  request: Parameters<WorkspaceClientPort["read"]>[0],
): request is AnnotationReadRequest {
  return request.operation.startsWith("annotation.");
}
