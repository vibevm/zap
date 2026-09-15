/** @scope spec://org.vibevm.zap/lens/PROP-001#identity */
import { z } from "zod";
import type { Result } from "../protocol/index.ts";
import {
  AuthenticatedActorHandleSchema,
  PersistedHostMessageSchema,
  REQ_DELIVERY,
  REQ_IDENTITY,
  SafePointHookInputSchema,
  SafePointHostSchema,
  diagnostic,
  failure,
  notObserved,
  observed,
  success,
  zodDetails,
} from "./types.ts";
import type {
  ActorRouteBinding,
  AdapterDiagnostic,
  HookRoute,
  SafePointHookInput,
  SafePointHost,
  SafePointOffer,
} from "./types.ts";

function activeHostBindings(
  host: SafePointHost,
  sessionId: string,
  bindings: readonly ActorRouteBinding[],
): readonly ActorRouteBinding[] {
  return bindings.filter(
    (binding) =>
      binding.actor.state === "active" &&
      binding.actor.hostKind === host &&
      binding.host.kind === host &&
      binding.host.sessionId === sessionId,
  );
}

function nativeClaimsAgree(
  binding: ActorRouteBinding,
  host: SafePointHost,
  input: SafePointHookInput,
): boolean {
  if (binding.host.kind !== host || binding.host.sessionId !== input.session_id) {
    return false;
  }
  return input.agent_id === undefined || binding.host.subagentId === input.agent_id;
}

/** @implements spec://org.vibevm.zap/lens/PROP-001#identity */
export function routeSafePointHook(
  hostInput: unknown,
  input: unknown,
  bindings: readonly ActorRouteBinding[],
  authenticatedHandle?: unknown,
): Result<HookRoute, AdapterDiagnostic> {
  const host = SafePointHostSchema.safeParse(hostInput);
  if (!host.success) {
    return failure(
      diagnostic(
        "invalid_hook_input",
        REQ_IDENTITY,
        "host hook input did not match its runtime schema",
        "send a documented hook event with a supported host and non-empty native session id",
        false,
        zodDetails(host.error),
      ),
    );
  }
  const hook = SafePointHookInputSchema.safeParse(input);
  if (!hook.success) {
    return failure(
      diagnostic(
        "invalid_hook_input",
        REQ_IDENTITY,
        "host hook input did not match its runtime schema",
        "send a documented hook event with a supported host and non-empty native session id",
        false,
        zodDetails(hook.error),
      ),
    );
  }

  const candidates = activeHostBindings(host.data, hook.data.session_id, bindings);
  let selected: ActorRouteBinding | undefined;
  if (authenticatedHandle !== undefined) {
    const handle = AuthenticatedActorHandleSchema.safeParse(authenticatedHandle);
    if (!handle.success) {
      return failure(
        diagnostic(
          "invalid_hook_input",
          REQ_IDENTITY,
          "authenticated actor handle was malformed",
          "pass the broker-attested actor, binding and generation tuple",
          false,
          zodDetails(handle.error),
        ),
      );
    }
    selected = bindings.find(
      (binding) =>
        binding.handle.actorId === handle.data.actorId &&
        binding.handle.bindingId === handle.data.bindingId &&
        binding.handle.workspaceId === handle.data.workspaceId &&
        binding.handle.conversationId === handle.data.conversationId,
    );
    if (selected === undefined) {
      return failure(
        diagnostic(
          "binding_not_found",
          REQ_IDENTITY,
          "authenticated actor handle has no binding",
          "reconnect the actor and pass the current protected handle",
        ),
      );
    }
    if (
      selected.actor.state !== "active" ||
      selected.handle.generation !== handle.data.generation
    ) {
      return failure(
        diagnostic(
          "stale_binding",
          REQ_IDENTITY,
          "authenticated actor handle names an expired or superseded binding",
          "refresh the actor handle before claiming or offering inbox work",
          true,
        ),
      );
    }
    if (hook.data.agent_id === undefined && candidates.length > 1 && !handle.data.nativeAttested) {
      return failure(
        diagnostic(
          "ambiguous_binding",
          REQ_IDENTITY,
          "inherited parent handle does not identify which concurrent actor emitted this hook",
          "use an event-native agent id or a separately attested exact actor handle",
        ),
      );
    }
    if (!nativeClaimsAgree(selected, host.data, hook.data)) {
      return failure(
        diagnostic(
          "binding_mismatch",
          REQ_IDENTITY,
          "authenticated actor handle disagrees with available native hook claims",
          "bind the exact host session and subagent id supplied by the hook",
        ),
      );
    }
  } else {
    const nativeMatches =
      hook.data.agent_id === undefined
        ? candidates
        : candidates.filter((binding) => binding.host.subagentId === hook.data.agent_id);
    if (nativeMatches.length === 0) {
      return failure(
        diagnostic(
          "binding_not_found",
          REQ_IDENTITY,
          "native hook claims do not identify an active actor binding",
          "connect or resume the actor, or supply its authenticated actor handle",
          true,
        ),
      );
    }
    if (nativeMatches.length !== 1) {
      return failure(
        diagnostic(
          "ambiguous_binding",
          REQ_IDENTITY,
          "parent session identity resolves to several active actors",
          "supply the authenticated child actor handle or a native subagent id",
        ),
      );
    }
    selected = nativeMatches[0];
  }

  if (selected === undefined || selected.host.sessionId === undefined) {
    return failure(
      diagnostic(
        "binding_not_found",
        REQ_IDENTITY,
        "route selection produced no usable native session binding",
        "register a host binding with a native session id",
      ),
    );
  }
  return success({
    host: host.data,
    actorId: selected.handle.actorId,
    bindingId: selected.handle.bindingId,
    bindingGeneration: selected.handle.generation,
    eventName: hook.data.hook_event_name,
    hostSessionId: selected.host.sessionId,
    ...(selected.host.subagentId === undefined ? {} : { hostSubagentId: selected.host.subagentId }),
  });
}

function escapeContext(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

/** @implements spec://org.vibevm.zap/lens/PROP-001#delivery */
export function buildSafePointOffer(
  route: HookRoute,
  messagesInput: unknown,
): Result<SafePointOffer, AdapterDiagnostic> {
  const messages = z.array(PersistedHostMessageSchema).min(1).max(50).safeParse(messagesInput);
  if (!messages.success) {
    return failure(
      diagnostic(
        "invalid_offer",
        REQ_DELIVERY,
        "safe-point offer was not a bounded list of persisted messages",
        "pass one to fifty schema-valid persisted inbox messages",
        false,
        zodDetails(messages.error),
      ),
    );
  }
  const mismatch = messages.data.find(
    (message) =>
      message.recipientActorId !== route.actorId ||
      message.bindingGeneration !== route.bindingGeneration,
  );
  if (mismatch !== undefined) {
    return failure(
      diagnostic(
        "stale_binding",
        REQ_DELIVERY,
        "message recipient or delivery generation does not match the routed actor",
        "reclaim the inbox with the current actor binding before offering it",
        true,
      ),
    );
  }
  const body = messages.data
    .map(
      (message) =>
        `<message delivery_id="${message.deliveryId}" message_id="${message.messageId}">${escapeContext(message.content)}</message>`,
    )
    .join("\n");
  const additionalContext = [
    `<codlens_inbox protocol="lens/1" actor_id="${route.actorId}" binding_generation="${route.bindingGeneration}">`,
    body,
    "</codlens_inbox>",
    "A host offer is not an actor acknowledgement. Acknowledge each delivery through codlens_ack after reading it.",
  ].join("\n");
  return success({
    route,
    hostOutput: {
      hookSpecificOutput: {
        hookEventName: route.eventName,
        additionalContext,
      },
    },
    deliveries: messages.data.map((message) => ({
      deliveryId: message.deliveryId,
      messageId: message.messageId,
      observations: {
        persisted: observed(true, "broker-persisted input"),
        offered: observed(true, `safe-point ${route.eventName} output`),
        hostAccepted: notObserved(),
        actorAcknowledged: notObserved(),
      },
    })),
  });
}
