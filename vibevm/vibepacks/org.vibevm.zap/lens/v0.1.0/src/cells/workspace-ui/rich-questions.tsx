/** Grouped rich questions and immutable answer history. @scope spec://org.vibevm.zap/lens/PROP-005#rich-questions */
import { component$, useSignal, useStore, type QRL } from "@qwik.dev/core";

import {
  QuestionOptionIdSchema,
  type QuestionAnswer,
  type QuestionAnswerVersion,
  type QuestionGroup,
  type QuestionGroupId,
  type QuestionItem,
  type QuestionSubmission,
} from "../workspace-model/index.ts";

export interface RichQuestionsProps {
  readonly groups: readonly QuestionGroup[];
  readonly selected: {
    readonly question: QuestionGroup;
    readonly answerVersions: readonly QuestionAnswerVersion[];
  } | null;
  readonly loading: boolean;
  readonly error: string | null;
  readonly onSelect$: QRL<(questionGroupId: QuestionGroupId) => void>;
  readonly onSubmit$: QRL<
    (
      question: QuestionGroup,
      submission: QuestionSubmission,
      amendmentReason: string | null,
    ) => Promise<boolean>
  >;
}

export const RichQuestionsPanel = component$<RichQuestionsProps>((props) => {
  const selectedValues = useStore<Record<string, string>>({});
  const multipleValues = useStore<Record<string, string[]>>({});
  const customValues = useStore<Record<string, string>>({});
  const amendmentReason = useSignal("");
  const amending = useSignal(false);
  const submitting = useSignal(false);
  const question = props.selected?.question ?? null;
  const canSubmit =
    question !== null &&
    question.items.every(
      (item) => !item.required || hasAnswer(item, selectedValues, multipleValues, customValues),
    );
  return (
    <div class="question-workspace">
      <section class="workspace-panel question-groups">
        <div class="workspace-panel-heading">
          <div>
            <p class="eyebrow">Inbox</p>
            <h2>Questions</h2>
          </div>
          <span class="count-chip">
            {props.groups.filter((group) => group.state === "open").length} open
          </span>
        </div>
        {props.groups.length === 0 ? (
          <div class="workspace-empty compact">No question groups in this context.</div>
        ) : (
          <nav aria-label="Question groups">
            {props.groups.map((group) => (
              <button
                key={group.questionGroupId}
                class={`question-group-link ${group.questionGroupId === question?.questionGroupId ? "selected" : ""}`}
                onClick$={() => props.onSelect$(group.questionGroupId)}
              >
                <span>
                  <strong>{group.title}</strong>
                  <small>{group.items.length} item(s)</small>
                </span>
                <span class={`question-state question-state-${group.state}`}>{group.state}</span>
              </button>
            ))}
          </nav>
        )}
      </section>

      <section class="workspace-panel question-detail">
        {props.loading ? (
          <p class="workspace-muted">Loading question details…</p>
        ) : props.error !== null ? (
          <p class="workspace-notice">{props.error}</p>
        ) : question === null ? (
          <div class="workspace-empty">
            <strong>Select a question group</strong>
            <span>Grouped prompts, context, choices, and answer history appear here.</span>
          </div>
        ) : (
          <>
            <div class="workspace-panel-heading">
              <div>
                <p class="eyebrow">/ZapAskUserQuestion</p>
                <h2>{question.title}</h2>
              </div>
              <span class={`question-state question-state-${question.state}`}>
                {question.state}
              </span>
            </div>
            <p class="question-introduction">{question.introductionMarkdown}</p>
            <div class="rich-question-list">
              {question.items.map((item) => (
                <RichQuestionItem
                  key={item.questionItemId}
                  item={item}
                  enabled={question.state === "open" || amending.value}
                  selectedValues={selectedValues}
                  multipleValues={multipleValues}
                  customValues={customValues}
                />
              ))}
            </div>
            {question.state === "answered" && !amending.value ? (
              <button class="button secondary" onClick$={() => (amending.value = true)}>
                Amend answers
              </button>
            ) : null}
            {amending.value ? (
              <label class="question-amendment">
                <span class="field-label">Reason for amendment</span>
                <textarea
                  rows={3}
                  value={amendmentReason.value}
                  onInput$={(_, element) => (amendmentReason.value = element.value)}
                />
              </label>
            ) : null}
            {question.state === "open" || amending.value ? (
              <button
                class="button primary"
                disabled={
                  submitting.value ||
                  !canSubmit ||
                  (amending.value && amendmentReason.value.trim().length === 0)
                }
                onClick$={async () => {
                  const submission = buildSubmission(
                    question,
                    selectedValues,
                    multipleValues,
                    customValues,
                  );
                  if (submission === null) return;
                  submitting.value = true;
                  const saved = await props.onSubmit$(
                    question,
                    submission,
                    amending.value ? amendmentReason.value.trim() : null,
                  );
                  submitting.value = false;
                  if (saved) {
                    amending.value = false;
                    amendmentReason.value = "";
                  }
                }}
              >
                {submitting.value
                  ? "Committing…"
                  : amending.value
                    ? "Commit amendment"
                    : "Submit answers"}
              </button>
            ) : null}
            <AnswerHistory versions={props.selected?.answerVersions ?? []} />
          </>
        )}
      </section>
    </div>
  );
});

const RichQuestionItem = component$<{
  readonly item: QuestionItem;
  readonly enabled: boolean;
  readonly selectedValues: Record<string, string>;
  readonly multipleValues: Record<string, string[]>;
  readonly customValues: Record<string, string>;
}>((props) => {
  const key = props.item.questionItemId;
  const selected = props.selectedValues[key] ?? "";
  const multiple = props.multipleValues[key] ?? [];
  const customSelected = selected === "__custom" || multiple.includes("__custom");
  return (
    <fieldset class="rich-question" disabled={!props.enabled}>
      <legend>{props.item.header}</legend>
      <p>{props.item.promptMarkdown}</p>
      {props.item.contextMarkdown === null ? null : (
        <div class="question-context">{props.item.contextMarkdown}</div>
      )}
      {props.item.recommendation === null ? null : (
        <div class="question-recommendation">
          <strong>Recommendation · not selected</strong>
          <span>{props.item.recommendation.explanationMarkdown}</span>
        </div>
      )}
      {props.item.answerMode === "single_choice" ? (
        <div class="question-options">
          {props.item.options.map((option) => (
            <OptionRow
              key={option.optionId}
              item={props.item}
              option={option}
              checked={selected === option.optionId}
              onChange$={() => (props.selectedValues[key] = option.optionId)}
            />
          ))}
          {props.item.customAnswer === null ? null : (
            <CustomChoice
              kind="radio"
              label={props.item.customAnswer.label}
              checked={customSelected}
              value={props.customValues[key] ?? ""}
              multiline={props.item.customAnswer.multiline}
              onSelect$={() => (props.selectedValues[key] = "__custom")}
              onInput$={(value) => (props.customValues[key] = value)}
            />
          )}
        </div>
      ) : props.item.answerMode === "multiple_choice" ? (
        <div class="question-options">
          {props.item.options.map((option) => (
            <OptionRow
              key={option.optionId}
              item={props.item}
              option={option}
              checked={multiple.includes(option.optionId)}
              multiple
              onChange$={() => {
                toggle(props.multipleValues, key, option.optionId);
              }}
            />
          ))}
          {props.item.customAnswer === null ? null : (
            <CustomChoice
              kind="checkbox"
              label={props.item.customAnswer.label}
              checked={customSelected}
              value={props.customValues[key] ?? ""}
              multiline={props.item.customAnswer.multiline}
              onSelect$={() => {
                toggle(props.multipleValues, key, "__custom");
              }}
              onInput$={(value) => (props.customValues[key] = value)}
            />
          )}
        </div>
      ) : props.item.answerMode === "multiline_text" ? (
        <textarea
          rows={5}
          value={selected}
          onInput$={(_, element) => (props.selectedValues[key] = element.value)}
        />
      ) : (
        <input
          value={selected}
          onInput$={(_, element) => (props.selectedValues[key] = element.value)}
        />
      )}
      {!props.item.required && selected.length === 0 && multiple.length === 0 ? (
        <small>Optional · leaving this blank records an explicit skip.</small>
      ) : null}
    </fieldset>
  );
});

const OptionRow = component$<{
  readonly item: QuestionItem;
  readonly option: QuestionItem["options"][number];
  readonly checked: boolean;
  readonly multiple?: boolean;
  readonly onChange$: QRL<() => void>;
}>((props) => (
  <label class="question-option">
    <input
      type={props.multiple ? "checkbox" : "radio"}
      name={props.multiple ? `${props.item.questionItemId}:multiple` : props.item.questionItemId}
      checked={props.checked}
      onChange$={props.onChange$}
    />
    <span>
      <strong>{props.option.label}</strong>
      <small>{props.option.description}</small>
      {props.option.previewMarkdown === null ? null : <em>{props.option.previewMarkdown}</em>}
    </span>
  </label>
));

const CustomChoice = component$<{
  readonly kind: "radio" | "checkbox";
  readonly label: string;
  readonly checked: boolean;
  readonly value: string;
  readonly multiline: boolean;
  readonly onSelect$: QRL<() => void>;
  readonly onInput$: QRL<(value: string) => void>;
}>((props) => (
  <div class="question-option custom">
    <label>
      <input type={props.kind} checked={props.checked} onChange$={props.onSelect$} />
      <strong>{props.label}</strong>
    </label>
    {props.multiline ? (
      <textarea
        rows={3}
        disabled={!props.checked}
        value={props.value}
        onInput$={(_, element) => props.onInput$(element.value)}
      />
    ) : (
      <input
        disabled={!props.checked}
        value={props.value}
        onInput$={(_, element) => props.onInput$(element.value)}
      />
    )}
  </div>
));

const AnswerHistory = component$<{ readonly versions: readonly QuestionAnswerVersion[] }>(
  (props) =>
    props.versions.length === 0 ? null : (
      <section class="answer-history">
        <h3>Answer history</h3>
        <ol>
          {props.versions.map((version) => (
            <li key={version.answerVersionId}>
              <strong>Revision {version.revision}</strong>
              <span>{version.submission.answers.length} answer(s)</span>
              {version.amendmentReasonMarkdown === null ? null : (
                <p>{version.amendmentReasonMarkdown}</p>
              )}
            </li>
          ))}
        </ol>
      </section>
    ),
);

function hasAnswer(
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

function buildSubmission(
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

function toggle(values: Record<string, string[]>, key: string, value: string): void {
  const current = values[key] ?? [];
  values[key] = current.includes(value)
    ? current.filter((candidate) => candidate !== value)
    : [...current, value];
}
