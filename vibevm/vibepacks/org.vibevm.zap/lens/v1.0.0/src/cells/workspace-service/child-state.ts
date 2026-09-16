/** Native child lifecycle projection. @scope spec://org.vibevm.zap/lens/PROP-007#agent-network */
import { z } from "zod";
import type { CoordinatorEvent } from "../agent-runtime/index.ts";
import type { AgentDescriptor } from "../workspace-model/index.ts";

const ChildStatusSchema = z.enum(["starting", "active", "waiting_for_user", "stopped", "failed"]);
const HostStatusSchema = z.union([
  z.enum([
    "starting",
    "bootstrapping",
    "ready",
    "running",
    "waiting_for_user",
    "pausing",
    "paused",
    "stopping",
    "stopped",
    "failed",
  ]),
  z.looseObject({
    type: z.enum(["notLoaded", "idle", "systemError", "active", "closed"]),
    activeFlags: z.array(z.string()).optional(),
  }),
]);

export function childStateFromEvent(event: CoordinatorEvent): AgentDescriptor["state"] | null {
  if (event.kind === "turn_started") return "active";
  if (event.kind === "turn_completed") {
    const parsed = z.looseObject({ status: z.string() }).safeParse(event.data);
    return parsed.success && parsed.data.status === "failed" ? "failed" : "stopped";
  }
  if (event.kind !== "session_status") return null;
  const parsed = z.looseObject({ status: z.unknown() }).safeParse(event.data);
  if (!parsed.success) return null;
  const direct = ChildStatusSchema.safeParse(parsed.data.status);
  if (direct.success) return direct.data;
  const host = HostStatusSchema.safeParse(parsed.data.status);
  if (!host.success || typeof host.data === "string") return null;
  if (host.data.type === "active") return "active";
  if (host.data.type === "systemError") return "failed";
  return host.data.type === "closed" ? "stopped" : null;
}
