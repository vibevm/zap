/** @scope spec://org.vibevm.zap/lens/PROP-001#adapters */
import {
  buildSafePointOffer,
  routeSafePointHook,
  type PersistedHostMessage,
  type SafePointOffer,
} from "../host-adapters/index.ts";
import { DecimalSchema, type Credential, type Result } from "../protocol/index.ts";
import {
  type AdapterSessionId,
  type AdapterSessions,
  failure,
  type TransportBrokerPort,
} from "../transport/index.ts";

export interface HostHookRequest {
  readonly broker: TransportBrokerPort;
  readonly sessions: AdapterSessions;
  readonly principalToken: Credential;
  readonly host: string;
  readonly input: unknown;
  readonly adapterSessionId?: AdapterSessionId;
}

/** Routes one native hook to an exact actor and returns a complete bounded offer. */
export async function executeHostHook(
  request: HostHookRequest,
): Promise<Result<SafePointOffer | null>> {
  const bindings = request.sessions.bindingsFor(request.principalToken);
  if (!bindings.ok || bindings.value.length === 0) {
    return failure("unauthorized", "host hook principal has no actor bindings");
  }
  const handle =
    request.adapterSessionId === undefined
      ? undefined
      : request.sessions.handleFor(request.adapterSessionId, request.principalToken);
  if (handle !== undefined && !handle.ok) return handle;
  const route = routeSafePointHook(
    request.host,
    request.input,
    bindings.value,
    handle?.ok ? handle.value : undefined,
  );
  if (!route.ok) {
    return failure(
      "invalid_input",
      route.error.message,
      "bind the exact native actor and retry at a safe point",
    );
  }
  const routed = request.sessions.sessionForActor(request.principalToken, route.value.actorId);
  if (!routed.ok) return routed;
  const cursor = request.sessions.offerCursor(route.value.actorId);
  if (!cursor.ok) return cursor;
  let inbox = await request.broker.inbox(routed.value.auth, {
    afterSequence: cursor.value,
    limit: 10,
  });
  if (inbox.ok && inbox.value.deliveries.length === 0 && cursor.value !== "0") {
    inbox = await request.broker.inbox(routed.value.auth, {
      afterSequence: DecimalSchema.parse("0"),
      limit: 10,
    });
  }
  if (!inbox.ok) return inbox;
  if (inbox.value.deliveries.length === 0) return { ok: true, value: null };
  const messages = inbox.value.deliveries.map(
    (delivery): PersistedHostMessage => ({
      deliveryId: delivery.deliveryId,
      messageId: delivery.message.messageId,
      recipientActorId: delivery.recipientActorId,
      bindingGeneration: route.value.bindingGeneration,
      content: boundedHookContent(delivery.message.payload),
      persisted: true,
    }),
  );
  const offered = boundedOffer(route.value, messages);
  if (!offered.ok) return failure("invalid_input", offered.error.message);
  const last = inbox.value.deliveries[offered.value.deliveries.length - 1];
  if (last !== undefined) {
    const saved = request.sessions.setOfferCursor(route.value.actorId, last.message.sequence);
    if (!saved.ok) return saved;
  }
  return { ok: true, value: offered.value };
}

function boundedOffer(
  route: Parameters<typeof buildSafePointOffer>[0],
  messages: readonly PersistedHostMessage[],
) {
  const first = messages[0];
  if (first === undefined) return buildSafePointOffer(route, []);
  let selected: PersistedHostMessage[] = [];
  let latest = buildSafePointOffer(route, [first]);
  for (const message of messages) {
    const candidate = buildSafePointOffer(route, [...selected, message]);
    if (
      !candidate.ok ||
      candidate.value.hostOutput.hookSpecificOutput.additionalContext.length > 2_500
    ) {
      break;
    }
    selected = [...selected, message];
    latest = candidate;
  }
  return latest;
}

function boundedHookContent(payload: unknown): string {
  const complete = JSON.stringify(payload);
  return complete.length <= 1_500
    ? complete
    : JSON.stringify({
        payloadOmitted: true,
        reason: "read complete payload through codlens_inbox",
      });
}
