/** Pure rich-question form helpers and safe artifact notices. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
import { component$ } from "@qwik.dev/core";
import {
  QuestionOptionIdSchema,
  type ArtifactReference,
  type QuestionAnswer,
  type QuestionGroup,
  type QuestionItem,
  type QuestionSubmission,
} from "../workspace-model/index.ts";

export const ArtifactNotice = component$<{ readonly references: readonly ArtifactReference[] }>(
  (props) =>
    props.references.length === 0 ? null : (
      <ul class="question-artifacts" aria-label="Question attachments">
        {props.references.map((reference) => (
          <li key={reference.artifactRefId}>
            <span>Attachment unavailable: {reference.label}</span>
          </li>
        ))}
      </ul>
    ),
);

export function clearValues(
  selected: Record<string, string>,
  multiple: Record<string, string[]>,
  custom: Record<string, string>,
): void {
  for (const key of Object.keys(selected)) selected[key] = "";
  for (const key of Object.keys(multiple)) multiple[key] = [];
  for (const key of Object.keys(custom)) custom[key] = "";
}

export function applyDraft(
  submission: QuestionSubmission,
  selected: Record<string, string>,
  multiple: Record<string, string[]>,
  custom: Record<string, string>,
): void {
  for (const entry of submission.answers) {
    if (entry.answer.kind === "single_choice")
      selected[entry.questionItemId] = entry.answer.optionId;
    else if (entry.answer.kind === "multiple_choice")
      multiple[entry.questionItemId] = [...entry.answer.optionIds];
    else if (entry.answer.kind === "short_text" || entry.answer.kind === "multiline_text")
      selected[entry.questionItemId] = entry.answer.text;
    else if (entry.answer.kind === "custom") {
      selected[entry.questionItemId] = "__custom";
      custom[entry.questionItemId] = entry.answer.text;
    }
  }
}

export function buildDraftSubmission(
  group: QuestionGroup,
  selected: Readonly<Record<string, string>>,
  multiple: Readonly<Record<string, string[]>>,
  custom: Readonly<Record<string, string>>,
): QuestionSubmission {
  return {
    answers: group.items.map((item) => ({
      questionItemId: item.questionItemId,
      answer: answerFor(item, selected, multiple, custom) ?? { kind: "skipped" },
    })),
    noteMarkdown: null,
  };
}

export function buildSubmission(
  group: QuestionGroup,
  selected: Readonly<Record<string, string>>,
  multiple: Readonly<Record<string, string[]>>,
  custom: Readonly<Record<string, string>>,
): QuestionSubmission | null {
  const answers = group.items.map((item) => {
    const answer = answerFor(item, selected, multiple, custom);
    return answer === null ? null : { questionItemId: item.questionItemId, answer };
  });
  if (answers.some((answer, index) => answer === null && group.items[index]?.required)) return null;
  return { answers: answers.filter((answer) => answer !== null), noteMarkdown: null };
}

export function hasAnswer(
  item: QuestionItem,
  selected: Readonly<Record<string, string>>,
  multiple: Readonly<Record<string, string[]>>,
  custom: Readonly<Record<string, string>>,
): boolean {
  const key = item.questionItemId;
  const selectedValue = selected[key] ?? "";
  const choices = multiple[key] ?? [];
  return selectedValue === "__custom" || choices.includes("__custom")
    ? (custom[key] ?? "").trim().length > 0
    : selectedValue.trim().length > 0 || choices.length > 0;
}

export function deadlineLabel(deadlineAt: string): string {
  const deadline = new Date(deadlineAt);
  if (Number.isNaN(deadline.valueOf())) return `Deadline · ${deadlineAt}`;
  return deadline.valueOf() <= Date.now()
    ? `Overdue · ${deadline.toLocaleString()}`
    : `Deadline · ${deadline.toLocaleString()}`;
}

export function deadlineClass(deadlineAt: string): string {
  const deadline = new Date(deadlineAt);
  return !Number.isNaN(deadline.valueOf()) && deadline.valueOf() <= Date.now()
    ? "question-deadline overdue"
    : "question-deadline";
}

function answerFor(
  item: QuestionItem,
  selected: Readonly<Record<string, string>>,
  multiple: Readonly<Record<string, string[]>>,
  custom: Readonly<Record<string, string>>,
): QuestionAnswer | null {
  const key = item.questionItemId;
  const value = selected[key] ?? "";
  const choices = multiple[key] ?? [];
  if (value === "__custom" || choices.includes("__custom")) {
    const text = (custom[key] ?? "").trim();
    return text.length === 0 ? null : { kind: "custom", text };
  }
  if (item.answerMode === "single_choice") {
    return value.length === 0
      ? item.required
        ? null
        : { kind: "skipped" }
      : { kind: "single_choice", optionId: QuestionOptionIdSchema.parse(value) };
  }
  if (item.answerMode === "multiple_choice") {
    const optionIds = choices
      .filter((choice) => choice !== "__custom")
      .map((choice) => QuestionOptionIdSchema.parse(choice));
    return optionIds.length === 0
      ? item.required
        ? null
        : { kind: "skipped" }
      : { kind: "multiple_choice", optionIds };
  }
  if (value.trim().length === 0) return item.required ? null : { kind: "skipped" };
  return item.answerMode === "multiline_text"
    ? { kind: "multiline_text", text: value.trim() }
    : { kind: "short_text", text: value.trim() };
}

export function toggle(values: Record<string, string[]>, key: string, value: string): void {
  const current = values[key] ?? [];
  values[key] = current.includes(value)
    ? current.filter((candidate) => candidate !== value)
    : [...current, value];
}
