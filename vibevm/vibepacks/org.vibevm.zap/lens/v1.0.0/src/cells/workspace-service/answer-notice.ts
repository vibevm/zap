/** Safe-point delivery of rich answers to owned coordinators. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import { createHash } from "node:crypto";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import type {
  WorkspaceAccessContext,
  WorkspaceCommandResponse,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import { ManagedWakeNoticeSchema, type WorkspaceStore } from "../workspace-store/index.ts";
import { dispatchNextChat } from "./chat.ts";
import { workspaceFailure } from "./errors.ts";
import type { Launch, LaunchActions } from "./launch.ts";
import type { ManagedWakePort, OwnedCoordinatorAgentPort } from "./types.ts";
import { dispatchManagedWake } from "./managed-wake.ts";

export async function notifyOwnedAnswer(input: {
  readonly store: WorkspaceStore;
  readonly ownedAgents: OwnedCoordinatorAgentPort | undefined;
  readonly launches: ReadonlyMap<string, Launch>;
  readonly actions: LaunchActions;
  readonly access: WorkspaceAccessContext;
  readonly response: WorkspaceCommandResponse;
  readonly managedWake: ManagedWakePort | undefined;
  readonly clock: () => Date;
}): Promise<WorkspaceResult<null>> {
  const response = input.response;
  if (
    (input.ownedAgents === undefined && input.managedWake === undefined) ||
    (response.operation !== "question.answer.v1" &&
      response.operation !== "question.amend.v1" &&
      response.operation !== "question.cancel.v1")
  ) {
    return { ok: true, value: null };
  }
  const binding = input.store.readAgentQuestionBinding(response.question.questionGroupId);
  if (!binding.ok || binding.value === null || binding.value.state !== "persisted")
    return binding.ok ? { ok: true, value: null } : binding;
  const execution = input.store.readProjectExecution(
    binding.value.projectId,
    binding.value.contextId,
  );
  if (!execution.ok) return execution;
  const answer = response.operation === "question.cancel.v1" ? null : response.answerVersion;
  const kind = response.operation === "question.cancel.v1" ? "QUESTION CANCELED" : "ANSWER READY";
  const versionId = answer?.answerVersionId ?? `cancel.${response.question.revision}`;
  const bodyMarkdown = [
    `${kind}: /ZapAskUserQuestion ${response.question.questionGroupId}`,
    `Origin actor: ${binding.value.actorId}. Question revision: ${response.question.revision}.`,
    response.operation === "question.cancel.v1"
      ? "The human canceled this question; do not continue waiting for an answer."
      : input.managedWake !== undefined
        ? "This answer is addressed to the managed worker's exact run and lease."
        : "This answer is addressed to your preprovisioned broker actor.",
    ...(answer === null ? [] : [`Submission: ${JSON.stringify(answer.submission)}`]),
  ].join("\n\n");
  if (input.managedWake !== undefined) {
    const target = await input.managedWake.resolveRecipient({
      projectId: binding.value.projectId,
      contextId: binding.value.contextId,
      actorId: binding.value.actorId,
    });
    if (target !== null) {
      const queued = input.store.queueManagedWake(
        ManagedWakeNoticeSchema.parse({
          wakeId: ClientRequestIdSchema.parse(
            `request.managed-wake.${createHash("sha256").update(`${response.question.questionGroupId}:${versionId}`).digest("hex")}`,
          ),
          projectId: binding.value.projectId,
          contextId: binding.value.contextId,
          actorId: binding.value.actorId,
          runId: target.runId,
          attemptId: target.attemptId,
          adapterSessionId: target.adapterSessionId,
          kind:
            response.operation === "question.answer.v1"
              ? "answer"
              : response.operation === "question.amend.v1"
                ? "amend"
                : "cancel",
          questionGroupId: response.question.questionGroupId,
          answerVersionId: answer?.answerVersionId ?? null,
          bodyMarkdown,
          sourceEventId: `question:${response.question.questionGroupId}:${versionId}`,
          state: "queued",
          updatedAt: input.clock().toISOString(),
        }),
      );
      if (!queued.ok) return queued;
      if (execution.value.state === "running" || execution.value.state === "uninitialized")
        await dispatchManagedWake({
          store: input.store,
          managedWake: input.managedWake,
          projectId: binding.value.projectId,
          contextId: binding.value.contextId,
          actorId: binding.value.actorId,
          clock: input.clock,
        });
      return { ok: true, value: null };
    }
  }
  if (input.ownedAgents === undefined) return { ok: true, value: null };
  const route = input.ownedAgents.route(binding.value.actorId);
  if (!route.ok || route.value === null) return route.ok ? { ok: true, value: null } : route;
  if (execution.value.sessionId !== route.value.coordinatorSessionId)
    return workspaceFailure("conflict", "owned answer route does not match project coordinator");
  const posted = input.store.command(input.access, {
    operation: "chat.post.v1",
    clientRequestId: ClientRequestIdSchema.parse(
      `request.answer-notice.${createHash("sha256").update(`${response.question.questionGroupId}:${versionId}`).digest("hex")}`,
    ),
    projectId: binding.value.projectId,
    contextId: binding.value.contextId,
    conversationId: binding.value.conversationId,
    bodyMarkdown: route.value.forwarding
      ? `${bodyMarkdown}\n\nThis is forwarded to the owned coordinator because direct native-child input is unavailable; child consumption is not claimed.`
      : bodyMarkdown,
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
