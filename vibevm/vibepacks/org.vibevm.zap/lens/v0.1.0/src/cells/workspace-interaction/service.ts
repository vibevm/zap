/** Durable interaction coordination. @scope spec://org.vibevm.zap/lens/PROP-005#simultaneous-clients */
import { createHash } from "node:crypto";
import { z } from "zod";
import { PendingHostRequestSchema, type AgentRuntimeError } from "../agent-runtime/index.ts";
import { ActorIdSchema, JsonValueSchema, PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  WorkspaceAccessContextSchema,
  type NativeInteractionRecord,
  type QuestionAnswerVersion,
} from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { nativeQuestionResponse, projectNativeRequest } from "./native.ts";
import {
  interactionFailure,
  type NativeInteractionDispatch,
  type ObservedInteractionScope,
  type WorkspaceInteractionFeature,
  type AgentAnswerDeliveryPort,
} from "./types.ts";

const ResolvedRequestSchema = z
  .object({ requestId: z.union([z.string(), z.number().int()]) })
  .loose();

export interface WorkspaceInteractionOptions {
  readonly store: WorkspaceStore;
  readonly clock?: () => Date;
  readonly agentAnswers?: AgentAnswerDeliveryPort;
}

export function createWorkspaceInteractionFeature(
  options: WorkspaceInteractionOptions,
): WorkspaceInteractionFeature {
  const clock = options.clock ?? (() => new Date());
  return {
    observeNativeRequest(scope) {
      if (scope.event.kind !== "host_request_pending") return { ok: true, value: null };
      const pending = PendingHostRequestSchema.safeParse(scope.event.data);
      if (
        !pending.success ||
        scope.originActorId === null ||
        pending.data.coordinatorSessionId !== scope.event.coordinatorSessionId ||
        pending.data.processEpoch !== scope.event.processEpoch
      ) {
        return interactionFailure(
          "invalid_input",
          "observed native request identity is inconsistent",
        );
      }
      const id = deterministicIds(pending.data);
      const projection = projectNativeRequest(
        pending.data,
        { ...scope, originActorId: scope.originActorId },
        id,
        clock().toISOString(),
      );
      if (!projection.ok) return projection;
      if (projection.value.kind === "approval") {
        return options.store.recordNativeApproval({
          approval: projection.value.approval,
          identity: projection.value.identity,
        });
      }
      const access = hostAccess(scope);
      const recorded = options.store.recordNativeQuestion({
        access,
        projectId: scope.projectId,
        contextId: scope.contextId,
        clientRequestId: projection.value.clientRequestId,
        conversationId: scope.conversationId,
        draft: projection.value.draft,
        identity: projection.value.identity,
        questionMap: projection.value.questionMap,
      });
      return recorded.ok ? { ok: true, value: recorded.value.question } : recorded;
    },
    async afterQuestionCommand(access, request, response, dispatch) {
      if (
        (request.operation !== "question.answer.v1" && request.operation !== "question.amend.v1") ||
        (response.operation !== "question.answer.v1" && response.operation !== "question.amend.v1")
      ) {
        return { ok: true, value: null };
      }
      const bound = options.store.readNativeInteractionForQuestion(
        response.question.questionGroupId,
      );
      if (!bound.ok) return bound;
      if (bound.value !== null) {
        if (bound.value.state !== "answer_ready") return { ok: true, value: null };
        if (!sameAccess(access, bound.value)) {
          return interactionFailure("forbidden", "native question answer scope is inconsistent");
        }
        if (dispatch === null || !dispatch.executionEnabled) return { ok: true, value: null };
        return dispatchQuestion(
          options.store,
          bound.value,
          response.answerVersion,
          dispatch,
          clock,
        );
      }
      return dispatchAgentAnswer(
        options.store,
        options.agentAnswers,
        response.answerVersion,
        clock,
      );
    },
    async respondToApproval(access, request, dispatch) {
      const answered = options.store.command(access, request);
      if (!answered.ok || answered.value.operation !== "native-approval.respond.v1")
        return answered;
      if (dispatch === null || !dispatch.executionEnabled) {
        return {
          ok: true,
          value: { operation: "native-approval.respond.v1", approval: answered.value.approval },
        };
      }
      const pending = options.store.pendingNativeResponses(
        request.projectId,
        request.contextId,
        256,
      );
      if (!pending.ok) return pending;
      const record = pending.value.find(
        (item) => item.approval?.nativeApprovalId === request.nativeApprovalId,
      );
      if (record === undefined) {
        const current = options.store.read(access, {
          operation: "native-approval.get.v1",
          projectId: request.projectId,
          contextId: request.contextId,
          nativeApprovalId: request.nativeApprovalId,
        });
        if (!current.ok) return current;
        return current.value.operation === "native-approval.get.v1"
          ? {
              ok: true,
              value: { operation: "native-approval.respond.v1", approval: current.value.approval },
            }
          : interactionFailure(
              "storage_failure",
              "native approval read returned another operation",
            );
      }
      const settled = await dispatchRecord(
        options.store,
        record,
        request.response,
        dispatch,
        clock,
      );
      if (!settled.ok) return settled;
      return {
        ok: true,
        value: {
          operation: "native-approval.respond.v1",
          approval: settled.value.approval ?? answered.value.approval,
        },
      };
    },
    async drain(dispatch) {
      if (!dispatch.executionEnabled) return { ok: true, value: null };
      const pending = options.store.pendingNativeResponses(
        dispatch.projectId,
        dispatch.contextId,
        256,
      );
      if (!pending.ok) return pending;
      for (const record of pending.value) {
        const response = responseForRecord(options.store, record);
        if (!response.ok) return response;
        const settled = await dispatchRecord(
          options.store,
          record,
          response.value,
          dispatch,
          clock,
        );
        if (!settled.ok) return settled;
      }
      return { ok: true, value: null };
    },
    observeResolved(scope) {
      if (scope.event.kind !== "host_request_resolved") return { ok: true, value: null };
      const resolved = ResolvedRequestSchema.safeParse(scope.event.data);
      if (!resolved.success)
        return interactionFailure("invalid_input", "native resolved request identity is malformed");
      const settled = options.store.settleNativeResponse({
        coordinatorSessionId: scope.event.coordinatorSessionId,
        processEpoch: scope.event.processEpoch,
        requestId: resolved.data.requestId,
        observation: "resolved",
        updatedAt: clock().toISOString(),
      });
      return settled.ok ? { ok: true, value: null } : settled;
    },
  };
}

async function dispatchAgentAnswer(
  store: WorkspaceStore,
  delivery: AgentAnswerDeliveryPort | undefined,
  answer: QuestionAnswerVersion,
  clock: () => Date,
) {
  const bound = store.readAgentQuestionBinding(answer.questionGroupId);
  if (!bound.ok || bound.value === null || bound.value.state !== "answer_ready")
    return bound.ok ? { ok: true as const, value: null } : bound;
  if (delivery === undefined) return { ok: true as const, value: null };
  const delivered = await delivery.deliver({ binding: bound.value, answer });
  const settlement = delivered.ok
    ? { observation: "persisted" as const, messageId: delivered.value.messageId }
    : { observation: agentDeliveryObservation(delivered.error), messageId: null };
  const settled = store.settleAgentAnswer({
    questionGroupId: answer.questionGroupId,
    expectedRevision: bound.value.revision,
    ...settlement,
    updatedAt: clock().toISOString(),
  });
  return settled.ok ? { ok: true as const, value: null } : settled;
}

function agentDeliveryObservation(error: { readonly code: string }): "refused" | "uncertain" {
  return error.code === "storage_failure" || error.code === "unavailable" ? "uncertain" : "refused";
}

async function dispatchQuestion(
  store: WorkspaceStore,
  record: NativeInteractionRecord,
  answer: QuestionAnswerVersion,
  dispatch: NativeInteractionDispatch,
  clock: () => Date,
) {
  const response = nativeQuestionResponse(record, answer);
  return response.ok
    ? dispatchRecord(store, record, response.value, dispatch, clock).then((result) =>
        result.ok ? { ok: true as const, value: null } : result,
      )
    : response;
}

function responseForRecord(store: WorkspaceStore, record: NativeInteractionRecord) {
  if (record.kind !== "user_input") {
    return record.response === null
      ? interactionFailure("storage_failure", "native approval response is missing")
      : { ok: true as const, value: record.response };
  }
  if (record.questionGroupId === null || record.answerVersionId === null) {
    return interactionFailure("storage_failure", "native question answer binding is incomplete");
  }
  const detail = store.read(hostAccess(record), {
    operation: "question.get.v1",
    projectId: record.projectId,
    contextId: record.contextId,
    questionGroupId: record.questionGroupId,
  });
  if (!detail.ok || detail.value.operation !== "question.get.v1") return detail;
  const answer = detail.value.detail.answerVersions.find(
    (version) => version.answerVersionId === record.answerVersionId,
  );
  return answer === undefined
    ? interactionFailure("storage_failure", "native question answer version is missing")
    : nativeQuestionResponse(record, answer);
}

async function dispatchRecord(
  store: WorkspaceStore,
  record: NativeInteractionRecord,
  response: unknown,
  dispatch: NativeInteractionDispatch,
  clock: () => Date,
) {
  if (
    record.projectId !== dispatch.projectId ||
    record.contextId !== dispatch.contextId ||
    record.identity.coordinatorSessionId !== dispatch.coordinatorSessionId
  ) {
    return interactionFailure("forbidden", "native response target is outside the active session");
  }
  if (record.identity.processEpoch !== dispatch.processEpoch) {
    const stale = store.settleNativeResponse({
      coordinatorSessionId: record.identity.coordinatorSessionId,
      processEpoch: record.identity.processEpoch,
      requestId: record.identity.requestId,
      observation: "stale",
      updatedAt: clock().toISOString(),
    });
    return stale.ok
      ? interactionFailure("stale_revision", "native response belongs to an earlier process epoch")
      : stale;
  }
  const safe = JsonValueSchema.safeParse(response);
  if (
    !safe.success ||
    safe.data === null ||
    Array.isArray(safe.data) ||
    typeof safe.data !== "object"
  )
    return interactionFailure("invalid_input", "native response is malformed");
  const prepared = store.prepareNativeResponse({
    projectId: record.projectId,
    contextId: record.contextId,
    questionGroupId: record.questionGroupId,
    nativeApprovalId: record.approval?.nativeApprovalId ?? null,
    expectedRevision: record.revision,
    currentProcessEpoch: dispatch.processEpoch,
    response: safe.data,
    updatedAt: clock().toISOString(),
  });
  if (!prepared.ok) return prepared;
  const delivered = await dispatch.adapter.respondToRequest({
    coordinatorSessionId: record.identity.coordinatorSessionId,
    requestId: record.identity.requestId,
    processEpoch: record.identity.processEpoch,
    answer: safe.data,
  });
  const observation = delivered.ok ? "host_accepted" : deliveryObservation(delivered.error);
  return store.settleNativeResponse({
    coordinatorSessionId: record.identity.coordinatorSessionId,
    processEpoch: record.identity.processEpoch,
    requestId: record.identity.requestId,
    observation,
    updatedAt: clock().toISOString(),
  });
}

function deliveryObservation(error: AgentRuntimeError): "refused" | "uncertain" | "stale" {
  if (error.code === "stale_epoch" || error.code === "not_found") return "stale";
  return error.code === "transport_lost" || error.code === "protocol_error"
    ? "uncertain"
    : "refused";
}

function deterministicIds(pending: z.infer<typeof PendingHostRequestSchema>) {
  let sequence = 0;
  const seed = JSON.stringify([
    pending.coordinatorSessionId,
    pending.processEpoch,
    typeof pending.requestId,
    pending.requestId,
  ]);
  return (kind: string): string => {
    const digest = createHash("sha256")
      .update(JSON.stringify([seed, kind, sequence++]))
      .digest("hex");
    return `${kind}.${digest}`;
  };
}

function hostAccess(scope: Pick<ObservedInteractionScope, "projectId" | "originActorId">) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse("principal.wayfinder.native-interaction"),
    actorId: ActorIdSchema.parse(scope.originActorId),
    clientId: ClientIdSchema.parse("client.wayfinder.native-interaction"),
    authorizedProjectIds: [scope.projectId],
  });
}

function sameAccess(
  access: { readonly authorizedProjectIds: readonly string[] },
  record: NativeInteractionRecord,
) {
  return access.authorizedProjectIds.includes(record.projectId);
}
