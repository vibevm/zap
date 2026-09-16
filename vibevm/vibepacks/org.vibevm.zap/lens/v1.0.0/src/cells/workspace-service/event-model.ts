/** Host event projection schemas. @scope spec://org.vibevm.zap/lens/PROP-007#agent-network */
import { z } from "zod";

export const ChildObservationSchema = z.looseObject({
  fromNativeThreadId: z.string().min(1).nullable(),
  toNativeThreadId: z.string().min(1),
  relationship: z.enum(["parent", "message"]),
});
const StatusSchema = z.enum([
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
]);
export const HostStatusSchema = z.union([
  StatusSchema,
  z.looseObject({
    type: z.enum(["notLoaded", "idle", "systemError", "active", "closed"]),
    activeFlags: z.array(z.string()).optional(),
  }),
]);
export const DeltaSchema = z.looseObject({ delta: z.string() });
export const CompletedItemSchema = z.looseObject({
  type: z.string().min(1),
  text: z.string().optional(),
  bodyMarkdown: z.string().optional(),
});

export function itemType(value: unknown): string {
  const parsed = CompletedItemSchema.safeParse(value);
  return parsed.success ? parsed.data.type : "unknown";
}
