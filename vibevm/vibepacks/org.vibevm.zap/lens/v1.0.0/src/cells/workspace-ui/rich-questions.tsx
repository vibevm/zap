/** Grouped rich questions and immutable answer history. @scope spec://org.vibevm.zap/lens/PROP-005#rich-questions */
import {
  component$,
  noSerialize,
  useSignal,
  useStore,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";

import {
  QuestionDraftSchema,
  type QuestionAnswerVersion,
  type QuestionGroup,
  type QuestionGroupId,
  type QuestionItem,
  type QuestionSubmission,
} from "../workspace-model/index.ts";
import {
  createLocalQuestionDraftStore,
  type QuestionDraftStore,
} from "../workspace-client/index.ts";
import {
  ArtifactNotice,
  applyDraft,
  buildDraftSubmission,
  buildSubmission,
  clearValues,
  deadlineClass,
  deadlineLabel,
  hasAnswer,
  toggle,
} from "./question-helpers.tsx";
import { answerLabel, latestAnswerVersion } from "./question-answer-presentation.ts";

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
  readonly onCancel$?: QRL<(question: QuestionGroup, reason: string) => Promise<boolean>>;
  readonly draftStore?: QuestionDraftStore;
  readonly projectLabel?: string;
  readonly contextLabel?: string;
  readonly requestingAgentLabel?: string;
  readonly workLabel?: string;
}

export const RichQuestionsPanel = component$<RichQuestionsProps>((props) => {
  const selectedValues = useStore<Record<string, string>>({});
  const multipleValues = useStore<Record<string, string[]>>({});
  const customValues = useStore<Record<string, string>>({});
  const amendmentReason = useSignal("");
  const cancelReason = useSignal("");
  const amending = useSignal(false);
  const submitting = useSignal(false);
  const canceling = useSignal(false);
  const draftStatus = useSignal<string | null>(null);
  const draftStore = useSignal<NoSerialize<QuestionDraftStore>>(
    noSerialize(props.draftStore ?? createLocalQuestionDraftStore()),
  );
  const question = props.selected?.question ?? null;
  const currentAnswer = latestAnswerVersion(props.selected?.answerVersions ?? []);
  const canSubmit =
    question !== null &&
    question.items.every(
      (item) => !item.required || hasAnswer(item, selectedValues, multipleValues, customValues),
    );
  useVisibleTask$(({ track }) => {
    const currentQuestion = track(() => props.selected?.question ?? null);
    const questionGroupId = currentQuestion?.questionGroupId ?? null;
    const revision = currentQuestion?.revision ?? null;
    if (questionGroupId === null || revision === null || currentQuestion === null) return;
    clearValues(selectedValues, multipleValues, customValues);
    const store = draftStore.value;
    if (store === undefined) {
      draftStatus.value = "Draft storage is unavailable in this client.";
      return;
    }
    const loaded = store.load({
      projectId: currentQuestion.projectId,
      contextId: currentQuestion.contextId,
      questionGroupId,
    });
    if (!loaded.ok) {
      draftStatus.value = loaded.message;
      return;
    }
    if (loaded.value === null) {
      draftStatus.value = null;
      return;
    }
    if (loaded.value.expectedRevision !== revision) {
      draftStatus.value = "A saved draft is from an older question revision and was not loaded.";
      return;
    }
    applyDraft(loaded.value.submission, selectedValues, multipleValues, customValues);
    draftStatus.value = "Saved draft restored for this question revision.";
  });
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
                aria-current={
                  group.questionGroupId === question?.questionGroupId ? "page" : undefined
                }
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
            <div class="question-identity" aria-label="Question context">
              <span>Project · {props.projectLabel ?? question.projectId}</span>
              <span>Context · {props.contextLabel ?? question.contextId}</span>
              <span>Requesting agent · {props.requestingAgentLabel ?? question.originActorId}</span>
              {props.workLabel === undefined ? null : <span>Work · {props.workLabel}</span>}
              <details class="question-technical-details">
                <summary>Technical details</summary>
                <span>Conversation · {question.conversationId}</span>
              </details>
            </div>
            <p class="question-introduction">{question.introductionMarkdown}</p>
            <div class="question-attention" role="status" aria-live="polite">
              <span>
                {question.independentWorkAvailable
                  ? "Other work can continue."
                  : "Waiting for this answer may block the requesting work."}
              </span>
              {question.deadlineAt === null ? null : (
                <span class={deadlineClass(question.deadlineAt)}>
                  {deadlineLabel(question.deadlineAt)}
                </span>
              )}
            </div>
            {question.state === "answered" && !amending.value && currentAnswer !== null ? (
              <PersistedAnswers
                heading="Current persisted answer"
                version={currentAnswer}
                items={question.items}
              />
            ) : (
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
            )}
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
            {question.state === "open" && props.onCancel$ === undefined ? null : question.state ===
              "open" ? (
              <label class="question-cancel">
                <span class="field-label">Cancellation reason</span>
                <textarea
                  rows={2}
                  maxLength={8_000}
                  aria-label="Cancellation reason"
                  value={cancelReason.value}
                  onInput$={(_, element) => (cancelReason.value = element.value)}
                />
                <button
                  class="button secondary"
                  disabled={canceling.value || cancelReason.value.trim().length === 0}
                  onClick$={async () => {
                    if (props.onCancel$ === undefined) return;
                    const current = props.selected?.question;
                    if (current === undefined) return;
                    canceling.value = true;
                    const cancelled = await props.onCancel$(current, cancelReason.value.trim());
                    canceling.value = false;
                    if (cancelled) {
                      cancelReason.value = "";
                      draftStatus.value = "Question cancelled.";
                    }
                  }}
                >
                  {canceling.value ? "Cancelling…" : "Cancel question"}
                </button>
              </label>
            ) : null}
            {question.state === "open" || amending.value ? (
              <div class="question-actions">
                <button
                  class="button primary"
                  disabled={
                    submitting.value ||
                    !canSubmit ||
                    (amending.value && amendmentReason.value.trim().length === 0)
                  }
                  onClick$={async () => {
                    const current = props.selected?.question;
                    if (current === undefined) return;
                    const submission = buildSubmission(
                      current,
                      selectedValues,
                      multipleValues,
                      customValues,
                    );
                    if (submission === null) return;
                    submitting.value = true;
                    draftStatus.value = "Submitting answer to Wayfinder…";
                    const saved = await props.onSubmit$(
                      current,
                      submission,
                      amending.value ? amendmentReason.value.trim() : null,
                    );
                    submitting.value = false;
                    draftStatus.value = saved
                      ? "Answer persisted; delivery status is managed by Wayfinder."
                      : "Answer was not saved. Review the error and try again.";
                    if (saved) {
                      amending.value = false;
                      amendmentReason.value = "";
                      const store = draftStore.value;
                      store?.remove({
                        projectId: current.projectId,
                        contextId: current.contextId,
                        questionGroupId: current.questionGroupId,
                      });
                    }
                  }}
                >
                  {submitting.value
                    ? "Submitting…"
                    : amending.value
                      ? "Commit amendment"
                      : "Submit answers"}
                </button>
                <button
                  class="button secondary"
                  type="button"
                  disabled={submitting.value}
                  onClick$={() => {
                    const current = props.selected?.question;
                    const store = draftStore.value;
                    if (current === undefined || store === undefined) {
                      draftStatus.value = "Draft storage is unavailable in this client.";
                      return;
                    }
                    const submission = buildDraftSubmission(
                      current,
                      selectedValues,
                      multipleValues,
                      customValues,
                    );
                    const saved = store.save({
                      projectId: current.projectId,
                      contextId: current.contextId,
                      draft: QuestionDraftSchema.parse({
                        questionGroupId: current.questionGroupId,
                        expectedRevision: current.revision,
                        submission,
                        savedAt: new Date().toISOString(),
                      }),
                    });
                    draftStatus.value = saved.ok ? "Draft saved in this client." : saved.message;
                  }}
                >
                  Save draft
                </button>
              </div>
            ) : null}
            {draftStatus.value === null ? null : (
              <p class="question-status" role="status" aria-live="polite">
                {draftStatus.value}
              </p>
            )}
            <AnswerHistory versions={props.selected?.answerVersions ?? []} items={question.items} />
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
      <legend>
        {props.item.header} · {props.item.required ? "Required" : "Optional"}
      </legend>
      <p id={`${key}:prompt`}>{props.item.promptMarkdown}</p>
      {props.item.contextMarkdown === null ? null : (
        <div class="question-context">{props.item.contextMarkdown}</div>
      )}
      {props.item.recommendation === null ? null : (
        <div class="question-recommendation">
          <strong>Recommendation · not selected</strong>
          <span>{props.item.recommendation.explanationMarkdown}</span>
        </div>
      )}
      <ArtifactNotice references={props.item.artifactRefs} />
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
          id={key}
          rows={5}
          aria-label={props.item.header}
          aria-describedby={`${key}:prompt`}
          maxLength={32_000}
          value={selected}
          onInput$={(_, element) => (props.selectedValues[key] = element.value)}
        />
      ) : (
        <input
          id={key}
          aria-label={props.item.header}
          aria-describedby={`${key}:prompt`}
          maxLength={4_000}
          value={selected}
          onInput$={(_, element) => (props.selectedValues[key] = element.value)}
        />
      )}
      {!props.item.required && selected.length === 0 && multiple.length === 0 ? (
        <small>Optional · leaving this blank records an explicit skip.</small>
      ) : null}
      {props.item.customAnswer === null ? null : (
        <small>
          Custom answer limit: {props.item.customAnswer.maximumLength.toLocaleString()} characters.
        </small>
      )}
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
      <ArtifactNotice references={props.option.artifactRefs} />
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
      <input
        type={props.kind}
        aria-label={props.label}
        checked={props.checked}
        onChange$={props.onSelect$}
      />
      <strong>{props.label}</strong>
    </label>
    {props.multiline ? (
      <textarea
        rows={3}
        aria-label={props.label}
        maxLength={32_000}
        disabled={!props.checked}
        value={props.value}
        onInput$={(_, element) => props.onInput$(element.value)}
      />
    ) : (
      <input
        aria-label={props.label}
        maxLength={4_000}
        disabled={!props.checked}
        value={props.value}
        onInput$={(_, element) => props.onInput$(element.value)}
      />
    )}
  </div>
));

const AnswerHistory = component$<{
  readonly versions: readonly QuestionAnswerVersion[];
  readonly items: readonly QuestionItem[];
}>((props) =>
  props.versions.length === 0 ? null : (
    <section class="answer-history">
      <h3>Answer history</h3>
      <ol>
        {props.versions.map((version) => (
          <li key={version.answerVersionId}>
            <strong>Revision {version.revision}</strong>
            <PersistedAnswerValues version={version} items={props.items} />
            {version.amendmentReasonMarkdown === null ? null : (
              <p>{version.amendmentReasonMarkdown}</p>
            )}
          </li>
        ))}
      </ol>
    </section>
  ),
);

const PersistedAnswers = component$<{
  readonly heading: string;
  readonly version: QuestionAnswerVersion;
  readonly items: readonly QuestionItem[];
}>((props) => (
  <section class="persisted-answer" aria-label={props.heading}>
    <h3>{props.heading}</h3>
    <PersistedAnswerValues version={props.version} items={props.items} />
  </section>
));

const PersistedAnswerValues = component$<{
  readonly version: QuestionAnswerVersion;
  readonly items: readonly QuestionItem[];
}>((props) => (
  <>
    <dl class="answer-values">
      {props.version.submission.answers.map((entry) => {
        const item = props.items.find(
          (candidate) => candidate.questionItemId === entry.questionItemId,
        );
        return (
          <div key={entry.questionItemId}>
            <dt>{item?.header ?? "Question"}</dt>
            <dd>{answerLabel(entry.answer, item)}</dd>
          </div>
        );
      })}
    </dl>
    {props.version.submission.noteMarkdown === null ? null : (
      <p class="answer-note">Note · {props.version.submission.noteMarkdown}</p>
    )}
  </>
));
