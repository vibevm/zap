/** Workspace request routing predicates. @scope spec://org.vibevm.zap/lens/PROP-014#projection */
import type { AnnotationCommandRequest } from "../workspace-model/index.ts";
import type { WorkspaceClientPort, WorkspaceCommandRequest } from "../workspace-model/index.ts";
import type { RepositoryWorkspaceCommandRequest, RepositoryWorkspaceReadRequest } from "./types.ts";

export function repositoryRead(
  request: Parameters<WorkspaceClientPort["read"]>[0],
): request is RepositoryWorkspaceReadRequest {
  return [
    "repository.get.v1",
    "plan.workspace.list.v1",
    "plan.workspace.get.v1",
    "worktree.list.v1",
    "worktree.get.v1",
    "integration.list.v1",
    "integration.get.v1",
    "integration.diff.v1",
  ].includes(request.operation);
}

export function repositoryCommand(
  request: WorkspaceCommandRequest,
): request is RepositoryWorkspaceCommandRequest {
  return [
    "plan.workspace.prepare.v1",
    "worktree.prepare.v1",
    "integration.prepare.v1",
    "integration.test.v1",
    "integration.review.v1",
    "integration.resolution.prepare.v1",
    "integration.promote.v1",
  ].includes(request.operation);
}

export function opensRepositoryWriter(operation: WorkspaceCommandRequest["operation"]): boolean {
  return [
    "session.start.v1",
    "chat.post.v1",
    "project.continue.v1",
    "managed-work.start.v1",
    "managed-work.continue.v1",
    "terminal.start.v1",
    "terminal.acquire.v1",
    "terminal.input.v1",
  ].includes(operation);
}

export function isAnnotationCommand(
  request: WorkspaceCommandRequest,
): request is AnnotationCommandRequest {
  return request.operation.startsWith("annotation.");
}
