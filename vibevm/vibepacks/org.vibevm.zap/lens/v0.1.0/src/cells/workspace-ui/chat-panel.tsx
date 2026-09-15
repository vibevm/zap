/** Durable coordinator chat. @scope spec://org.vibevm.zap/lens/PROP-005#chat */
import { component$, useSignal, type QRL } from "@qwik.dev/core";

import type { ChatMessage, CoordinatorSession } from "../workspace-model/index.ts";

export const ChatPanel = component$<{
  readonly coordinator: CoordinatorSession | null;
  readonly messages: readonly ChatMessage[];
  readonly loading: boolean;
  readonly error: string | null;
  readonly onSend$: QRL<(message: string) => Promise<boolean>>;
}>((props) => {
  const draft = useSignal("");
  const sending = useSignal(false);
  return (
    <section class="workspace-panel chat-panel">
      <div class="workspace-panel-heading">
        <div>
          <p class="eyebrow">Coordinator chat</p>
          <h2>{props.coordinator === null ? "No coordinator session" : "Project coordinator"}</h2>
        </div>
        <span class={`coordinator-state state-${props.coordinator?.state ?? "stopped"}`}>
          {props.coordinator?.state.replaceAll("_", " ") ?? "not started"}
        </span>
      </div>
      <p class="workspace-muted">
        Messages are persisted before dispatch. Queued input does not silently interrupt a running
        turn.
      </p>
      {props.loading ? (
        <p class="workspace-muted">Loading conversation…</p>
      ) : props.error !== null ? (
        <p class="workspace-notice">{props.error}</p>
      ) : props.messages.length === 0 ? (
        <div class="workspace-empty compact">No public coordinator messages yet.</div>
      ) : (
        <ol class="chat-list">
          {props.messages.map((message) => (
            <li class={`chat-message chat-${message.role}`} key={message.messageId}>
              <div class="chat-meta">
                <strong>
                  {message.role === "assistant" ? "Coordinator" : humanRole(message.role)}
                </strong>
                <span class={`delivery delivery-${message.deliveryState}`}>
                  {message.deliveryState.replaceAll("_", " ")}
                </span>
              </div>
              <p>{message.bodyMarkdown}</p>
              <time dateTime={message.createdAt}>{formatTime(message.createdAt)}</time>
            </li>
          ))}
        </ol>
      )}
      <div class="chat-composer">
        <label class="field-label" for="workspace-chat-message">
          Message coordinator
        </label>
        <textarea
          id="workspace-chat-message"
          rows={4}
          disabled={props.coordinator === null || sending.value}
          placeholder={
            props.coordinator === null
              ? "Start the project coordinator before sending a message."
              : "Describe the next question or planning intent"
          }
          value={draft.value}
          onInput$={(_, element) => {
            draft.value = element.value;
          }}
        />
        <button
          class="button primary"
          disabled={props.coordinator === null || sending.value || draft.value.trim().length === 0}
          onClick$={async () => {
            sending.value = true;
            const sent = await props.onSend$(draft.value.trim());
            if (sent) draft.value = "";
            sending.value = false;
          }}
        >
          {sending.value ? "Persisting…" : "Send"}
        </button>
      </div>
    </section>
  );
});

function humanRole(value: ChatMessage["role"]): string {
  if (value === "user") return "You";
  if (value === "execution") return "Execution";
  if (value === "system") return "Zap Quick Lens";
  return "Coordinator";
}

function formatTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(
    new Date(value),
  );
}
