/** Trusted workspace event ingestion contract. @scope spec://org.vibevm.zap/lens/PROP-005#shared-code */
import { z } from "zod";
import { ActorIdSchema, DecimalSchema, JsonValueSchema } from "../protocol/index.ts";
import { HistoryEventSchema } from "./entities.ts";
import { HistoryEventIdSchema, ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";

export const WorkspaceEventIngestSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema.nullable(),
    kind: z.string().min(3).max(160),
    source: z.enum(["lens", "host", "zap"]),
    actorId: ActorIdSchema.nullable(),
    occurrenceAt: z.iso.datetime(),
    sourceEventId: z.string().min(1).max(512).nullable(),
    correlationId: z.string().min(3).max(160).nullable(),
    causationId: HistoryEventIdSchema.nullable(),
    planProvenance: HistoryEventSchema.shape.planProvenance,
    sourceSequence: DecimalSchema.nullable(),
    payload: JsonValueSchema,
  })
  .strict();
export type WorkspaceEventIngest = z.infer<typeof WorkspaceEventIngestSchema>;
