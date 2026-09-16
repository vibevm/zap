/** Bounded authenticated inbox wait. @scope spec://org.vibevm.zap/lens/PROP-005#question-routing */
import type { BindingAuth, WaitInboxInputSchema } from "../protocol/index.ts";
import { failure, type TransportBrokerPort } from "../transport/index.ts";
import { delay } from "./wire.ts";
import type { z } from "zod";

export async function executeInboxWait(
  broker: TransportBrokerPort,
  auth: BindingAuth,
  input: z.output<typeof WaitInboxInputSchema>,
  signal: AbortSignal,
) {
  const deadline = Date.now() + input.timeoutMilliseconds;
  const inboxInput = { afterSequence: input.afterSequence, limit: input.limit };
  let page = await broker.inbox(auth, inboxInput);
  while (page.ok && page.value.deliveries.length === 0 && Date.now() < deadline) {
    await delay(Math.min(100, Math.max(1, deadline - Date.now())), signal);
    if (signal.aborted) break;
    page = await broker.inbox(auth, inboxInput);
  }
  return signal.aborted ? failure("closed", "authenticated inbox wait was cancelled") : page;
}
