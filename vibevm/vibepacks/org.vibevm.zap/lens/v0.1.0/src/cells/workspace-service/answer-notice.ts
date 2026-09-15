/** Safe-point delivery of rich answers to owned coordinators. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import { createHash } from "node:crypto";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import type {
  WorkspaceAccessContext,
  WorkspaceCommandResponse,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { dispatchNextChat } from "./chat.ts";
import { workspaceFailure } from "./errors.ts";
import type { Launch, LaunchActions } from "./launch.ts";
import type { OwnedCoordinatorAgentPort } from "./types.ts";

export async function notifyOwnedAnswer(input: {
  readonly store: WorkspaceStore;
  readonly ownedAgents: OwnedCoordinatorAgentPort | undefined;
  readonly launches: ReadonlyMap<string, Launch>;
  readonly actions: LaunchActions;
  readonly access: WorkspaceAccessContext;
  readonly response: WorkspaceCommandResponse;
}): Promise<WorkspaceResult<null>> {
  const response = input.response;
  if (
    input.ownedAgents === undefined ||
    (response.operation !== "question.answer.v1" && response.operation !== "question.amend.v1")
  ) {
    return { ok: true, value: null };
  }
  const binding = input.store.readAgentQuestionBinding(response.question.questionGroupId);
  if (!binding.ok || binding.value === null || binding.value.state !== "persisted")
    return binding.ok ? { ok: true, value: null } : binding;
  const route = input.ownedAgents.route(binding.value.actorId);
  if (!route.ok || route.value === null) return route.ok ? { ok: true, value: null } : route;
  const execution = input.store.readProjectExecution(
    binding.value.projectId,
    binding.value.contextId,
  );
  if (!execution.ok) return execution;
  if (execution.value.sessionId !== route.value.coordinatorSessionId)
    return workspaceFailure("conflict", "owned answer route does not match project coordinator");
  const answer = response.answerVersion;
  const kind = route.value.forwarding ? "FORWARDING" : "ANSWER READY";
  const bodyMarkdown = [
    `${kind}: /ZapAskUserQuestion ${response.question.questionGroupId}`,
    `Origin actor: ${binding.value.actorId}. Answer version: ${answer.answerVersionId}.`,
    route.value.forwarding
      ? "This is forwarded to the owned coordinator because direct native-child input is unavailable; child consumption is not claimed."
      : "This answer is addressed to your preprovisioned broker actor.",
    `Submission: ${JSON.stringify(answer.submission)}`,
  ].join("\n\n");
  const posted = input.store.command(input.access, {
    operation: "chat.post.v1",
    clientRequestId: ClientRequestIdSchema.parse(
      `request.answer-notice.${createHash("sha256").update(answer.answerVersionId).digest("hex")}`,
    ),
    projectId: binding.value.projectId,
    contextId: binding.value.contextId,
    conversationId: binding.value.conversationId,
    bodyMarkdown,
    artifactRefs: [],
    correlationId: response.question.questionGroupId,
    causationMessageId: null,
  });
  if (!posted.ok) return posted;
  if (posted.value.operation !== "chat.post.v1")
    return workspaceFailure("storage_failure", "answer notice returned another operation");
  const queued = input.store.queueChat(posted.value.message.messageId);
  if (!queued.ok) return queued;
  const launch = input.launches.get(route.value.coordinatorSessionId);
  if (
    launch !== undefined &&
    launch.descriptor.state !== "running" &&
    execution.value.state === "running"
  ) {
    const dispatched = await dispatchNextChat(input.actions, launch);
    if (!dispatched.ok) return dispatched;
  }
  return { ok: true, value: null };
}
