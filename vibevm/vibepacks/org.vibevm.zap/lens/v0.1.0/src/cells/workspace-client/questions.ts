/** Human question command builders for WorkspaceClientPort owners. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import type {
  ProjectId,
  QuestionGroupId,
  WorkContextId,
  WorkspaceCommandRequest,
} from "../workspace-model/index.ts";

export function createQuestionCancelRequest(input: {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly questionGroupId: QuestionGroupId;
  readonly expectedRevision: string;
  readonly reasonMarkdown: string;
}): Extract<WorkspaceCommandRequest, { operation: "question.cancel.v1" }> {
  return {
    operation: "question.cancel.v1",
    clientRequestId: ClientRequestIdSchema.parse(
      `request.question.cancel.${input.questionGroupId}.${crypto.randomUUID()}`,
    ),
    projectId: input.projectId,
    contextId: input.contextId,
    questionGroupId: input.questionGroupId,
    expectedRevision: DecimalSchema.parse(input.expectedRevision),
    reasonMarkdown: input.reasonMarkdown,
  };
}
