/** Workspace store row schemas. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { z } from "zod";

export const ProjectRowSchema = z.object({
  public_json: z.string(),
  coordinator_launch_options_json: z.string(),
  request_digest: z.string(),
});
export const ScopeRowSchema = z.object({ project_id: z.string(), context_id: z.string() });
export const MaxSequenceSchema = z.object({ value: z.bigint().nullable() });
export const OutputSourceSchema = z.object({ source_event_id: z.string() });
