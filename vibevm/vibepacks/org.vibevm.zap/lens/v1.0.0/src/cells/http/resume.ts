/** @scope spec://org.vibevm.zap/lens/PROP-001#identity */
import { randomUUID } from "node:crypto";
import { ClientRequestIdSchema, type Result } from "../protocol/index.ts";
import type { AdapterSessions, TransportBrokerPort } from "../transport/index.ts";

/** Fences prior broker bindings once per durable actor while preserving aliases. */
export async function resumeRetainedSessions(
  broker: TransportBrokerPort,
  sessions: AdapterSessions,
): Promise<Result<null>> {
  const retained = sessions.retained();
  if (!retained.ok) return retained;
  const groups = new Map<string, typeof retained.value>();
  for (const entry of retained.value) {
    const session = entry[1];
    const key = `${session.connection.actor.principalId}\u0000${session.connection.actor.actorId}`;
    groups.set(key, [...(groups.get(key) ?? []), entry]);
  }
  for (const aliases of groups.values()) {
    const first = aliases[0];
    if (first === undefined) continue;
    const session = first[1];
    const state = sessions.actorState(session.connection.actor.actorId);
    if (!state.ok) return state;
    if (state.value === "expired") continue;
    const resumed = await broker.resume({
      principalToken: session.principalToken,
      clientRequestId: ClientRequestIdSchema.parse(`resume.${randomUUID().replaceAll("-", "")}`),
      actorId: session.connection.actor.actorId,
      resumeCredential: session.connection.credentials.resumeCredential,
      host: session.host,
    });
    if (!resumed.ok) return resumed;
    for (const [id, alias] of aliases) {
      const replaced = sessions.replace(
        id,
        alias.principalToken,
        resumed.value,
        alias.host,
        alias.replyPolicy,
      );
      if (!replaced.ok) return replaced;
    }
  }
  return { ok: true, value: null };
}
