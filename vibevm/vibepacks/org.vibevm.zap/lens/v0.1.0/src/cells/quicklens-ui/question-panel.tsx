/** @scope spec://org.vibevm.zap/lens/PROP-002#interaction */
import { component$, useStore, useSignal, type NoSerialize, type QRL } from "@qwik.dev/core";

import type {
  ActionAvailability,
  QuicklensDataSource,
  QuestionView,
} from "../quicklens-model/index.ts";

export interface QuestionPanelProps {
  readonly questions: readonly QuestionView[];
  readonly answerAvailability: ActionAvailability;
  readonly source: NoSerialize<QuicklensDataSource>;
  readonly onRefresh$: QRL<() => void>;
}

export const QuestionPanel = component$<QuestionPanelProps>((props) => {
  const drafts = useStore<Record<string, string>>({});
  const message = useSignal<string | null>(null);
  const pending = props.questions.filter((question) => question.state === "pending");
  return (
    <section class="panel questions-panel" id="questions">
      <div class="panel-title-row">
        <div>
          <p class="eyebrow">Inbox</p>
          <h2>Questions</h2>
        </div>
        <span class="count-chip">{pending.length} pending</span>
      </div>
      {props.answerAvailability.enabled || props.answerAvailability.reason === null ? null : (
        <p class="unknown-copy">{props.answerAvailability.reason}</p>
      )}
      {props.questions.length === 0 ? (
        <div class="empty-state compact">
          <strong>No questions</strong>
          <span>New addressed questions will appear here.</span>
        </div>
      ) : (
        <div class="question-stack">
          {props.questions.map((question) => (
            <article class={`question question-${question.state}`} key={question.ref}>
              <div class="question-meta">
                <span>{question.addressedActorLabel}</span>
                <span>{question.state}</span>
              </div>
              <h3>{question.prompt}</h3>
              {question.state === "pending" ? (
                <div class="answer-row">
                  {question.answerMode === "single_choice" ? (
                    <select
                      disabled={!props.answerAvailability.enabled}
                      aria-label="Select answer"
                      value={drafts[question.ref] ?? ""}
                      onChange$={(_, element) => {
                        drafts[question.ref] = element.value;
                      }}
                    >
                      <option value="">Choose…</option>
                      {question.choices.map((choice) => (
                        <option value={choice.id} key={choice.id}>
                          {choice.label}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      disabled={!props.answerAvailability.enabled}
                      aria-label="Answer"
                      placeholder="Write an answer"
                      value={drafts[question.ref] ?? ""}
                      onInput$={(_, element) => {
                        drafts[question.ref] = element.value;
                      }}
                    />
                  )}
                  <button
                    class="button primary"
                    disabled={
                      !props.answerAvailability.enabled ||
                      (drafts[question.ref] ?? "").trim().length === 0
                    }
                    onClick$={async () => {
                      const source = props.source;
                      if (source === undefined) return;
                      const result = await source.answerQuestion({
                        questionRef: question.ref,
                        expectedRevision: question.revision,
                        answer: drafts[question.ref] ?? "",
                      });
                      message.value = result.ok ? "Answer committed." : result.error.message;
                      if (result.ok) void props.onRefresh$();
                    }}
                  >
                    Answer
                  </button>
                </div>
              ) : (
                <p class="question-answer">{question.answer ?? `Question ${question.state}.`}</p>
              )}
              {question.amendmentCount === "0" ? null : (
                <small>{question.amendmentCount} amendment(s)</small>
              )}
            </article>
          ))}
        </div>
      )}
      {message.value === null ? null : <p class="operation-message">{message.value}</p>}
    </section>
  );
});
