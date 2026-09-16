/** Authenticated workspace access and failure contracts. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { z } from "zod";
import {
  ActorIdSchema,
  JsonValueSchema,
  PrincipalIdSchema,
  type Result,
} from "../protocol/index.ts";
import { ClientIdSchema, ProjectIdSchema } from "./ids.ts";

export const WorkspaceErrorCodeSchema = z.enum([
  "invalid_input",
  "unauthorized",
  "forbidden",
  "not_found",
  "conflict",
  "stale_revision",
  "idempotency_conflict",
  "unavailable",
  "unsupported_operation",
  "storage_failure",
  "closed",
]);
export const WorkspaceErrorSchema = z
  .object({
    code: WorkspaceErrorCodeSchema,
    message: z.string().startsWith("violates REQ spec://"),
    details: z.record(z.string(), JsonValueSchema).optional(),
  })
  .strict();
export type WorkspaceError = z.infer<typeof WorkspaceErrorSchema>;
export type WorkspaceResult<T> = Result<T, WorkspaceError>;
export type Awaitable<T> = T | Promise<T>;

export const WorkspaceAccessContextSchema = z
  .object({
    principalId: PrincipalIdSchema,
    actorId: ActorIdSchema.nullable(),
    clientId: ClientIdSchema,
    authorizedProjectIds: z.array(ProjectIdSchema).min(1).max(256),
    catalogAdministrator: z.boolean().optional(),
  })
  .strict();
export type WorkspaceAccessContext = z.infer<typeof WorkspaceAccessContextSchema>;
export const WorkspaceCommandContextSchema = WorkspaceAccessContextSchema;
export type WorkspaceCommandContext = WorkspaceAccessContext;
