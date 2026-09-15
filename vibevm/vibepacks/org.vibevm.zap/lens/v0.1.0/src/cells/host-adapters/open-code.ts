/** @scope spec://org.vibevm.zap/lens/PROP-001#adapters */
import type { Result } from "../protocol/index.ts";
import {
  ActorRouteBindingSchema,
  OpenCodeSendResultSchema,
  OpenCodeStatusSchema,
  PersistedHostMessageSchema,
  REQ_ADAPTERS,
  REQ_DELIVERY,
  REQ_IDENTITY,
  diagnostic,
  failure,
  notObserved,
  observed,
  success,
  uncertain,
  zodDetails,
} from "./types.ts";
import type {
  ActorRouteBinding,
  AdapterDiagnostic,
  OpenCodeDispatch,
  OpenCodeTransport,
  PersistedHostMessage,
} from "./types.ts";

function validateOpenCodeBinding(
  bindingInput: unknown,
  message: PersistedHostMessage,
): Result<ActorRouteBinding, AdapterDiagnostic> {
  const binding = ActorRouteBindingSchema.safeParse(bindingInput);
  if (!binding.success || binding.data.host.kind !== "opencode") {
    return failure(
      diagnostic(
        "invalid_offer",
        REQ_ADAPTERS,
        "OpenCode dispatch requires a schema-valid OpenCode actor binding",
        "pass the active broker binding for the target OpenCode session",
        false,
        binding.success ? undefined : zodDetails(binding.error),
      ),
    );
  }
  if (
    binding.data.actor.state !== "active" ||
    binding.data.handle.actorId !== message.recipientActorId ||
    binding.data.handle.generation !== message.bindingGeneration
  ) {
    return failure(
      diagnostic(
        "stale_binding",
        REQ_IDENTITY,
        "OpenCode message does not match the active actor binding generation",
        "refresh the binding and reclaim the delivery before dispatch",
        true,
      ),
    );
  }
  if (binding.data.host.sessionId === undefined) {
    return failure(
      diagnostic(
        "binding_not_found",
        REQ_IDENTITY,
        "OpenCode binding has no known server session id",
        "attach to the known shared server and attest the exact session id",
      ),
    );
  }
  return success(binding.data);
}

/** @implements spec://org.vibevm.zap/lens/PROP-001#adapters */
export async function dispatchOpenCode(
  messageInput: unknown,
  bindingInput: unknown,
  transport: OpenCodeTransport,
): Promise<Result<OpenCodeDispatch, AdapterDiagnostic>> {
  const message = PersistedHostMessageSchema.safeParse(messageInput);
  if (!message.success) {
    return failure(
      diagnostic(
        "invalid_offer",
        REQ_DELIVERY,
        "OpenCode offer was not a persisted host message",
        "validate the broker delivery before host dispatch",
        false,
        zodDetails(message.error),
      ),
    );
  }
  const binding = validateOpenCodeBinding(bindingInput, message.data);
  if (!binding.ok) return binding;
  const sessionId = binding.value.host.sessionId;
  if (sessionId === undefined) {
    return failure(
      diagnostic(
        "binding_not_found",
        REQ_IDENTITY,
        "OpenCode binding lost its native session id",
        "refresh the exact server session binding",
      ),
    );
  }

  let statusRaw: unknown;
  try {
    statusRaw = await transport.observeStatus(sessionId);
  } catch {
    return failure(
      diagnostic(
        "transport_failure",
        REQ_ADAPTERS,
        "OpenCode status transport failed before a host offer",
        "retry from the separate dispatcher after checking the known server endpoint",
        true,
      ),
    );
  }
  const status = OpenCodeStatusSchema.safeParse(statusRaw);
  if (!status.success) {
    return failure(
      diagnostic(
        "transport_failure",
        REQ_ADAPTERS,
        "OpenCode status response did not match the injected transport schema",
        "fix the HTTP client decoder for the documented status response",
        true,
        zodDetails(status.error),
      ),
    );
  }
  if (status.data.state !== "idle") {
    return success({
      outcome: "held",
      deliveryId: message.data.deliveryId,
      messageId: message.data.messageId,
      reconcileRequired: false,
      observations: {
        persisted: observed(true, "broker-persisted input"),
        offered: observed(false, `status ${status.data.state}; send not attempted`),
        hostAccepted: notObserved(),
        actorAcknowledged: notObserved(),
      },
    });
  }

  let sendRaw: unknown;
  try {
    sendRaw = await transport.sendWhenIdle({
      sessionId,
      expectedStatusObservationId: status.data.observationId,
      deliveryId: message.data.deliveryId,
      messageId: message.data.messageId,
      content: message.data.content,
    });
  } catch {
    return failure(
      diagnostic(
        "transport_failure",
        REQ_ADAPTERS,
        "OpenCode send transport failed after an idle observation",
        "reconcile message history by broker ids before retrying",
        false,
      ),
    );
  }
  const send = OpenCodeSendResultSchema.safeParse(sendRaw);
  if (!send.success) {
    return failure(
      diagnostic(
        "transport_failure",
        REQ_ADAPTERS,
        "OpenCode send response did not match the injected transport schema",
        "fix the HTTP client decoder and reconcile before retrying",
        true,
        zodDetails(send.error),
      ),
    );
  }
  const common = {
    deliveryId: message.data.deliveryId,
    messageId: message.data.messageId,
  };
  switch (send.data.kind) {
    case "completed":
      if (send.data.statusObservationId !== status.data.observationId) {
        return success({
          ...common,
          outcome: "uncertain",
          reconcileRequired: true,
          observations: {
            persisted: observed(true, "broker-persisted input"),
            offered: observed(true, "OpenCode message request"),
            hostAccepted: uncertain("status observation changed during dispatch"),
            actorAcknowledged: notObserved(),
          },
        });
      }
      return success({
        ...common,
        outcome: "host_accepted",
        reconcileRequired: false,
        observations: {
          persisted: observed(true, "broker-persisted input"),
          offered: observed(true, "OpenCode message request"),
          hostAccepted: observed(true, `receipt ${send.data.hostReceiptId}`),
          actorAcknowledged: notObserved(),
        },
      });
    case "accepted_without_result":
      return success({
        ...common,
        outcome: "uncertain",
        reconcileRequired: true,
        observations: {
          persisted: observed(true, "broker-persisted input"),
          offered: observed(true, "OpenCode asynchronous HTTP acceptance"),
          hostAccepted: uncertain("HTTP acceptance did not prove a scheduled model turn"),
          actorAcknowledged: notObserved(),
        },
      });
    case "busy":
      return success({
        ...common,
        outcome: "uncertain",
        reconcileRequired: true,
        observations: {
          persisted: observed(true, "broker-persisted input"),
          offered: observed(true, "OpenCode idle-race send attempt"),
          hostAccepted: uncertain("session became busy after the idle observation"),
          actorAcknowledged: notObserved(),
        },
      });
    case "rejected":
      return success({
        ...common,
        outcome: "rejected",
        reconcileRequired: false,
        observations: {
          persisted: observed(true, "broker-persisted input"),
          offered: observed(true, "OpenCode rejected send attempt"),
          hostAccepted: observed(false, send.data.reason),
          actorAcknowledged: notObserved(),
        },
      });
  }
}
