/** Workspace-level opaque identities. @scope spec://org.vibevm.zap/lens/PROP-005#project-context */
import { z } from "zod";

const OpaqueIdSchema = z
  .string()
  .min(3)
  .max(160)
  .regex(/^[A-Za-z][A-Za-z0-9._:-]*$/);

export const ProjectIdSchema = OpaqueIdSchema.brand<"ProjectId">();
export type ProjectId = z.infer<typeof ProjectIdSchema>;
export const WorkContextIdSchema = OpaqueIdSchema.brand<"WorkContextId">();
export type WorkContextId = z.infer<typeof WorkContextIdSchema>;
export const AgentSessionIdSchema = OpaqueIdSchema.brand<"AgentSessionId">();
export type AgentSessionId = z.infer<typeof AgentSessionIdSchema>;
export const RunIdSchema = OpaqueIdSchema.brand<"RunId">();
export type RunId = z.infer<typeof RunIdSchema>;
export const AttemptIdSchema = OpaqueIdSchema.brand<"AttemptId">();
export type AttemptId = z.infer<typeof AttemptIdSchema>;
export const ExecutionHostIdSchema = OpaqueIdSchema.brand<"ExecutionHostId">();
export type ExecutionHostId = z.infer<typeof ExecutionHostIdSchema>;
export const TerminalIdSchema = OpaqueIdSchema.brand<"TerminalId">();
export type TerminalId = z.infer<typeof TerminalIdSchema>;
export const TerminalLeaseIdSchema = OpaqueIdSchema.brand<"TerminalLeaseId">();
export type TerminalLeaseId = z.infer<typeof TerminalLeaseIdSchema>;
export const ClientIdSchema = OpaqueIdSchema.brand<"ClientId">();
export type ClientId = z.infer<typeof ClientIdSchema>;
export const QuestionGroupIdSchema = OpaqueIdSchema.brand<"QuestionGroupId">();
export type QuestionGroupId = z.infer<typeof QuestionGroupIdSchema>;
export const QuestionItemIdSchema = OpaqueIdSchema.brand<"QuestionItemId">();
export type QuestionItemId = z.infer<typeof QuestionItemIdSchema>;
export const QuestionOptionIdSchema = OpaqueIdSchema.brand<"QuestionOptionId">();
export type QuestionOptionId = z.infer<typeof QuestionOptionIdSchema>;
export const AnswerVersionIdSchema = OpaqueIdSchema.brand<"AnswerVersionId">();
export type AnswerVersionId = z.infer<typeof AnswerVersionIdSchema>;
export const HistoryEventIdSchema = OpaqueIdSchema.brand<"HistoryEventId">();
export type HistoryEventId = z.infer<typeof HistoryEventIdSchema>;
export const ArtifactRefIdSchema = OpaqueIdSchema.brand<"ArtifactRefId">();
export type ArtifactRefId = z.infer<typeof ArtifactRefIdSchema>;
export const TaskIdSchema = OpaqueIdSchema.brand<"TaskId">();
export type TaskId = z.infer<typeof TaskIdSchema>;
