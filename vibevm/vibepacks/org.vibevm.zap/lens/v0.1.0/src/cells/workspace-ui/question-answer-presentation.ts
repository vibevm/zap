/** Persisted human-answer presentation. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
import type {
  QuestionAnswer,
  QuestionAnswerVersion,
  QuestionItem,
} from "../workspace-model/index.ts";

export function latestAnswerVersion(
  versions: readonly QuestionAnswerVersion[],
): QuestionAnswerVersion | null {
  return versions.reduce<QuestionAnswerVersion | null>(
    (latest, version) =>
      latest === null || BigInt(version.revision) > BigInt(latest.revision) ? version : latest,
    null,
  );
}

export function answerLabel(answer: QuestionAnswer, item: QuestionItem | undefined): string {
  if (answer.kind === "short_text" || answer.kind === "multiline_text" || answer.kind === "custom")
    return answer.text;
  if (answer.kind === "skipped") return "Skipped";
  const labels = (answer.kind === "single_choice" ? [answer.optionId] : answer.optionIds).map(
    (optionId) => item?.options.find((option) => option.optionId === optionId)?.label ?? optionId,
  );
  return labels.join(", ");
}
