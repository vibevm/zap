/** Durable queue wake predicates for coordinator-safe points. @scope spec://org.vibevm.zap/lens/PROP-012#wake-queue */
import type { CoordinatorEvent } from "../agent-runtime/index.ts";
import type { Launch } from "./launch.ts";

/**
 * A wake is only a request to inspect the durable chat queue. dispatchNextChat
 * performs the execution, epoch and ready-state fences, so an active status
 * event cannot cause an unsafe send.
 */
export function shouldWakeCoordinatorChat(
  launch: { readonly descriptor: Pick<Launch["descriptor"], "nativeThreadRef"> },
  event: CoordinatorEvent,
): boolean {
  const root =
    event.nativeThreadId === null ||
    event.nativeThreadId === launch.descriptor.nativeThreadRef.value;
  if (!root) return false;
  return (
    event.kind === "turn_completed" ||
    event.kind === "session_continued" ||
    event.kind === "session_started" ||
    event.kind === "session_resumed" ||
    event.kind === "session_status"
  );
}
