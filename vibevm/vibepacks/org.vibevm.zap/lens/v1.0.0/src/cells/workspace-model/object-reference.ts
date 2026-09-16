/** Stable project-scoped object identity shared by canvas, notes and work. @scope spec://org.vibevm.zap/lens/PROP-011#anchored-notes */
import { z } from "zod";
import { ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";

export const ProjectObjectReferenceSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    domain: z.enum([
      "project",
      "semantic_object",
      "semantic_relationship",
      "agent",
      "work_task",
      "work_run",
      "plan_workspace",
      "worktree",
      "integration",
    ]),
    ref: z.string().min(1).max(1_152),
  })
  .strict();
export type ProjectObjectReference = z.infer<typeof ProjectObjectReferenceSchema>;

export function projectObjectReferenceKey(reference: ProjectObjectReference): string {
  return [reference.projectId, reference.contextId, reference.domain, reference.ref].join("\u0000");
}
