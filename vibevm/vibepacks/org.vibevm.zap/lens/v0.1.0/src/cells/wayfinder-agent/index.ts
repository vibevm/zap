/** Broker-authenticated actor bridge into Zap Wayfinder. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import { createHash } from "node:crypto";
import {
  ClientRequestIdSchema,
  PublicConnectionSchema,
  BrokerErrorCodeSchema,
  type BrokerError,
  type PublicConnection,
  type Result,
} from "../protocol/index.ts";
import {
  ClientIdSchema,
  WorkspaceAccessContextSchema,
  type QuestionGroup,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import {
  AgentQuestionInputSchema,
  interactionFailure,
  type AgentAnswerDeliveryPort,
  type AgentQuestionInput,
  type AgentQuestionPublisher,
} from "../workspace-interaction/index.ts";
import type { PrincipalTransportPort } from "../transport/index.ts";

export interface WayfinderAgentPublisherOptions {
  readonly store: WorkspaceStore;
}

export function createWayfinderAgentPublisher(
  options: WayfinderAgentPublisherOptions,
): AgentQuestionPublisher {
  return {
    publish(rawActor: PublicConnection, rawInput: AgentQuestionInput) {
      return Promise.resolve().then(() => {
        const actor = PublicConnectionSchema.safeParse(rawActor);
        const input = AgentQuestionInputSchema.safeParse(rawInput);
        if (!actor.success || !input.success)
          return brokerFailure("invalid_input", "rich question request is malformed");
        if (
          actor.data.actor.state !== "active" ||
          actor.data.actor.actorId !== actor.data.handle.actorId ||
          actor.data.actor.workspaceId !== actor.data.handle.workspaceId ||
          actor.data.actor.conversationId !== actor.data.handle.conversationId ||
          !actor.data.actor.capabilities.includes("question:ask")
        ) {
          return brokerFailure("forbidden", "current broker actor cannot publish questions");
        }
        const scope = options.store.resolveAgentScope(
          actor.data.actor.workspaceId,
          actor.data.actor.conversationId,
        );
        if (!scope.ok) return fromWorkspaceError(scope.error);
        const access = WorkspaceAccessContextSchema.parse({
          principalId: actor.data.actor.principalId,
          actorId: actor.data.actor.actorId,
          clientId: ClientIdSchema.parse(clientId(actor.data)),
          authorizedProjectIds: [scope.value.projectId],
        });
        const result = options.store.recordAgentQuestion({
          access,
          actor: actor.data,
          clientRequestId: input.data.clientRequestId,
          projectId: scope.value.projectId,
          contextId: scope.value.contextId,
          draft: input.data.draft,
        });
        if (!result.ok) return fromWorkspaceError(result.error);
        return { ok: true as const, value: result.value.question };
      });
    },
  };
}

/** Delivers a committed rich answer to the exact broker actor inbox; consumption remains unobserved. */
export function createBrokerAgentAnswerDelivery(
  principal: PrincipalTransportPort,
): AgentAnswerDeliveryPort {
  return {
    async deliver({ binding, answer }) {
      const requestId = ClientRequestIdSchema.parse(
        `request.agent-answer.${digestText(answer.answerVersionId)}`,
      );
      const emitted = await principal.emit({
        clientRequestId: requestId,
        workspaceId: binding.workspaceId,
        conversationId: binding.conversationId,
        toActorId: binding.actorId,
        correlationId: binding.questionGroupId,
        payload: {
          type: "question.answer",
          questionGroupId: binding.questionGroupId,
          answerVersionId: answer.answerVersionId,
          revision: answer.revision,
          submission: answer.submission,
          delivery: "persisted",
        },
      });
      return emitted.ok
        ? { ok: true, value: { observation: "persisted", messageId: emitted.value.messageId } }
        : interactionFailure(
            emitted.error.code === "storage_failure" ? "storage_failure" : "conflict",
            emitted.error.message,
          );
    },
  };
}

function clientId(actor: PublicConnection): string {
  const digest = digestText(
    `${actor.actor.principalId}\0${actor.handle.bindingId}\0${actor.handle.generation}`,
  );
  return `client.wayfinder.agent.${digest}`;
}

function digestText(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function fromWorkspaceError(error: {
  readonly code: string;
  readonly message: string;
}): Result<never> {
  const code = BrokerErrorCodeSchema.safeParse(error.code);
  return brokerFailure(code.success ? code.data : "unsupported_operation", error.message);
}

function brokerFailure(code: BrokerError["code"], why: string): Result<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#question-routing: ${why}; fix surface: reconnect the exact broker actor to its registered project context`,
    },
  };
}

export type { AgentQuestionInput, AgentQuestionPublisher, QuestionGroup };
