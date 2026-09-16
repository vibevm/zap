/** Notes, deferred instructions and recoverable Trash UI. @scope spec://org.vibevm.zap/lens/PROP-011#anchored-notes */
import {
  $,
  component$,
  noSerialize,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import { workspaceRequestId, type WorkspaceClientPort } from "../workspace-client/index.ts";
import {
  projectObjectReferenceKey,
  type AnnotationNote,
  type AnnotationNoteVersion,
  type AnnotationTrashEntry,
  type ProjectObjectReference,
} from "../workspace-model/index.ts";
import { ScopedRequestFence } from "./request-fence.ts";

export const AnnotationPanel = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly target: ProjectObjectReference;
  readonly sourceBasisRef: string;
  readonly mode: "notes" | "trash";
  readonly onClose$: QRL<() => void>;
}>((props) => {
  const notes = useSignal<readonly AnnotationNote[]>([]);
  const trash = useSignal<readonly AnnotationTrashEntry[]>([]);
  const trashNext = useSignal<string | null>(null);
  const selected = useSignal<AnnotationNote | null>(null);
  const versions = useSignal<readonly AnnotationNoteVersion[]>([]);
  const kind = useSignal<"passive" | "deferred">("passive");
  const title = useSignal("");
  const body = useSignal("");
  const reason = useSignal("");
  const busy = useSignal(false);
  const message = useSignal<string | null>(null);
  const fence = useSignal<NoSerialize<ScopedRequestFence>>(noSerialize(new ScopedRequestFence()));
  const load = $(async () => {
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined) return;
    const token = activeFence.begin(`load:${scopeKey(props.mode, props.target)}`);
    const [noteResult, trashResult] = await Promise.all([
      port.read({
        operation: "annotation.note.list.v1",
        projectId: props.target.projectId,
        contextId: props.target.contextId,
        includeArchived: true,
      }),
      port.read({
        operation: "annotation.trash.list.v1",
        projectId: props.target.projectId,
        contextId: props.target.contextId,
        limit: 256,
        afterTrashId: null,
      }),
    ]);
    if (!activeFence.isCurrent(token)) return;
    if (!noteResult.ok || noteResult.value.operation !== "annotation.note.list.v1") {
      message.value = noteResult.ok
        ? "Note service returned an unexpected response."
        : noteResult.error.message;
      return;
    }
    if (!trashResult.ok || trashResult.value.operation !== "annotation.trash.list.v1") {
      message.value = trashResult.ok
        ? "Trash service returned an unexpected response."
        : trashResult.error.message;
      return;
    }
    const targetKey = projectObjectReferenceKey(props.target);
    notes.value =
      props.mode === "trash"
        ? noteResult.value.notes
        : noteResult.value.notes.filter(
            (note) => projectObjectReferenceKey(note.target) === targetKey,
          );
    trash.value = trashResult.value.entries;
    trashNext.value = trashResult.value.nextTrashId;
  });
  useVisibleTask$(({ track, cleanup }) => {
    track(() => `${props.mode}:${projectObjectReferenceKey(props.target)}`);
    fence.value?.cancel();
    selected.value = null;
    versions.value = [];
    trashNext.value = null;
    title.value = "";
    body.value = "";
    reason.value = "";
    busy.value = false;
    message.value = null;
    void load();
    cleanup(() => {
      fence.value?.cancel();
    });
  });
  const run = $(async (request: Parameters<WorkspaceClientPort["command"]>[0], success: string) => {
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined) return false;
    const token = activeFence.begin(`command:${scopeKey(props.mode, props.target)}`);
    const selectedNoteId = selected.value?.noteId ?? null;
    busy.value = true;
    const result = await port.command(request);
    if (!activeFence.isCurrent(token)) return false;
    busy.value = false;
    if (!result.ok) {
      message.value = result.error.message;
      return false;
    }
    message.value = success;
    const updatedNote = "note" in result.value ? result.value.note : null;
    const refreshSelected = updatedNote !== null && updatedNote.noteId === selectedNoteId;
    if (refreshSelected) selected.value = updatedNote;
    await load();
    if (refreshSelected) {
      const refreshToken = activeFence.begin(
        `note:${scopeKey(props.mode, props.target)}:${updatedNote.noteId}`,
      );
      const detail = await port.read({
        operation: "annotation.note.get.v1",
        projectId: updatedNote.projectId,
        contextId: updatedNote.contextId,
        noteId: updatedNote.noteId,
      });
      if (
        activeFence.isCurrent(refreshToken) &&
        detail.ok &&
        detail.value.operation === "annotation.note.get.v1"
      ) {
        selected.value = detail.value.note;
        versions.value = detail.value.versions;
        kind.value = detail.value.note.kind;
        title.value = detail.value.note.title;
        body.value = detail.value.note.bodyMarkdown;
      }
    }
    return true;
  });
  const inspect = $(async (note: AnnotationNote) => {
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined) return;
    const token = activeFence.begin(`note:${scopeKey(props.mode, props.target)}:${note.noteId}`);
    const result = await port.read({
      operation: "annotation.note.get.v1",
      projectId: note.projectId,
      contextId: note.contextId,
      noteId: note.noteId,
    });
    if (!activeFence.isCurrent(token)) return;
    if (!result.ok || result.value.operation !== "annotation.note.get.v1") {
      message.value = result.ok
        ? "Note history returned an unexpected response."
        : result.error.message;
      return;
    }
    selected.value = result.value.note;
    versions.value = result.value.versions;
    kind.value = result.value.note.kind;
    title.value = result.value.note.title;
    body.value = result.value.note.bodyMarkdown;
  });
  const loadMoreTrash = $(async () => {
    const port = props.port;
    const activeFence = fence.value;
    const cursor = trashNext.value;
    if (port === undefined || activeFence === undefined || cursor === null) return;
    const token = activeFence.begin(`trash-page:${scopeKey(props.mode, props.target)}:${cursor}`);
    const result = await port.read({
      operation: "annotation.trash.list.v1",
      projectId: props.target.projectId,
      contextId: props.target.contextId,
      limit: 256,
      afterTrashId: cursor,
    });
    if (!activeFence.isCurrent(token)) return;
    if (!result.ok || result.value.operation !== "annotation.trash.list.v1") {
      message.value = result.ok
        ? "Trash page returned an unexpected response."
        : result.error.message;
      return;
    }
    trash.value = [...trash.value, ...result.value.entries];
    trashNext.value = result.value.nextTrashId;
  });
  return (
    <section class="workspace-panel annotation-panel">
      <div class="workspace-section-heading">
        <div>
          <p class="eyebrow">{props.mode === "notes" ? "Anchored notes" : "Recoverable Trash"}</p>
          <h2>{props.mode === "notes" ? "Context for future work" : "Removed project context"}</h2>
        </div>
        <button class="button secondary" onClick$={props.onClose$}>
          Close
        </button>
      </div>
      <details class="canvas-scope annotation-anchor">
        <summary>Exact anchor · {props.target.domain.replaceAll("_", " ")}</summary>
        <span>{props.target.ref}</span>
        <span>
          {props.target.projectId} · {props.target.contextId}
        </span>
      </details>
      {props.mode === "trash" ? (
        <TrashList
          entries={trash.value}
          notes={notes.value}
          busy={busy.value}
          hasMore={trashNext.value !== null}
          onLoadMore$={loadMoreTrash}
          onRestore$={$(async (entry) => {
            const operation =
              entry.entryKind === "note"
                ? "annotation.note.restore.v1"
                : "annotation.object.restore.intent.v1";
            await run(
              {
                operation,
                clientRequestId: workspaceRequestId("annotation-restore"),
                projectId: entry.projectId,
                contextId: entry.contextId,
                trashId: entry.trashId,
                expectedRevision: entry.revision,
              },
              entry.entryKind === "note"
                ? "Note restored."
                : "Object restoration intent created for planning review.",
            );
          })}
          onRelink$={$(async (entry) => {
            const note = notes.value.find((candidate) =>
              entry.relatedNoteIds.includes(candidate.noteId),
            );
            if (note === undefined) {
              message.value = "The archived note is unavailable; refresh Trash.";
              return;
            }
            await run(
              {
                operation: "annotation.note.relink.v1",
                clientRequestId: workspaceRequestId("annotation-relink"),
                projectId: note.projectId,
                contextId: note.contextId,
                noteId: note.noteId,
                expectedRevision: note.currentVersion,
                target: props.target,
                sourceBasisRef: props.sourceBasisRef,
                targetSnapshot: null,
              },
              "Note relinked to the selected map object with its history retained.",
            );
          })}
        />
      ) : (
        <>
          <div class="annotation-list">
            {notes.value.length === 0 ? (
              <p class="workspace-muted">No notes on this exact anchor.</p>
            ) : (
              notes.value.map((note) => (
                <button
                  key={note.noteId}
                  class={selected.value?.noteId === note.noteId ? "selected" : ""}
                  onClick$={() => inspect(note)}
                >
                  <strong>{note.title}</strong>
                  <span>
                    {note.kind} · {note.state} · revision {note.currentVersion}
                  </span>
                </button>
              ))
            )}
          </div>
          <div class="annotation-editor">
            <label class="field-label" for="annotation-kind">
              Note behavior
            </label>
            <select
              id="annotation-kind"
              value={kind.value}
              disabled={selected.value !== null}
              onChange$={(_, element) =>
                (kind.value = element.value === "deferred" ? "deferred" : "passive")
              }
            >
              <option value="passive">Passive note</option>
              <option value="deferred">Deferred instruction before future work</option>
            </select>
            <label class="field-label" for="annotation-title">
              Title
            </label>
            <input
              id="annotation-title"
              value={title.value}
              onInput$={(_, element) => (title.value = element.value)}
            />
            <label class="field-label" for="annotation-body">
              Details
            </label>
            <textarea
              id="annotation-body"
              rows={5}
              value={body.value}
              onInput$={(_, element) => (body.value = element.value)}
            />
            <div class="execution-actions">
              {selected.value === null ? (
                <button
                  class="button primary"
                  disabled={busy.value || title.value.trim() === "" || body.value.trim() === ""}
                  onClick$={async () => {
                    if (
                      await run(
                        {
                          operation: "annotation.note.create.v1",
                          clientRequestId: workspaceRequestId("annotation-create"),
                          projectId: props.target.projectId,
                          contextId: props.target.contextId,
                          target: props.target,
                          kind: kind.value,
                          title: title.value.trim(),
                          bodyMarkdown: body.value.trim(),
                          sourceBasisRef: props.sourceBasisRef,
                          targetSnapshot: null,
                        },
                        "Note saved.",
                      )
                    ) {
                      title.value = "";
                      body.value = "";
                    }
                  }}
                >
                  Save note
                </button>
              ) : (
                <button
                  class="button primary"
                  disabled={busy.value || title.value.trim() === "" || body.value.trim() === ""}
                  onClick$={async () => {
                    const note = selected.value;
                    if (note === null) return;
                    await run(
                      {
                        operation: "annotation.note.update.v1",
                        clientRequestId: workspaceRequestId("annotation-update"),
                        projectId: note.projectId,
                        contextId: note.contextId,
                        noteId: note.noteId,
                        expectedRevision: note.currentVersion,
                        title: title.value.trim(),
                        bodyMarkdown: body.value.trim(),
                        sourceBasisRef: props.sourceBasisRef,
                        targetSnapshot: null,
                      },
                      "Note updated.",
                    );
                  }}
                >
                  Save changes
                </button>
              )}
              {selected.value?.kind !== "deferred" || selected.value.state !== "active" ? null : (
                <button
                  class="button secondary"
                  disabled={busy.value}
                  onClick$={async () => {
                    const note = selected.value;
                    if (note !== null)
                      await run(
                        {
                          operation: "annotation.note.send.v1",
                          clientRequestId: workspaceRequestId("annotation-send"),
                          projectId: note.projectId,
                          contextId: note.contextId,
                          noteId: note.noteId,
                          expectedRevision: note.currentVersion,
                        },
                        "Deferred instruction offered to the addressed work context.",
                      );
                  }}
                >
                  Offer now
                </button>
              )}
              {selected.value === null ? null : (
                <button
                  class="button secondary"
                  onClick$={() => {
                    selected.value = null;
                    versions.value = [];
                    title.value = "";
                    body.value = "";
                  }}
                >
                  New note
                </button>
              )}
            </div>
            {selected.value === null ? null : (
              <>
                <label class="field-label" for="annotation-reason">
                  Archive reason
                </label>
                <input
                  id="annotation-reason"
                  value={reason.value}
                  onInput$={(_, element) => (reason.value = element.value)}
                />
                <button
                  class="button danger"
                  disabled={
                    busy.value || selected.value.state === "archived" || reason.value.trim() === ""
                  }
                  onClick$={async () => {
                    const note = selected.value;
                    if (note !== null)
                      await run(
                        {
                          operation: "annotation.note.archive.v1",
                          clientRequestId: workspaceRequestId("annotation-archive"),
                          projectId: note.projectId,
                          contextId: note.contextId,
                          noteId: note.noteId,
                          expectedRevision: note.currentVersion,
                          reasonMarkdown: reason.value.trim(),
                        },
                        "Note moved to Trash; deferred delivery is suppressed.",
                      );
                  }}
                >
                  Move to Trash
                </button>
                <details>
                  <summary>Version history · {versions.value.length}</summary>
                  <ol>
                    {versions.value.map((version) => (
                      <li key={`${version.noteId}:${version.version}`}>
                        <strong>Revision {version.version}</strong> · {version.kind}
                        <br />
                        {version.bodyMarkdown}
                      </li>
                    ))}
                  </ol>
                </details>
              </>
            )}
          </div>
        </>
      )}
      {message.value === null ? null : (
        <p class="workspace-notice" role="status">
          {message.value}
        </p>
      )}
    </section>
  );
});

const TrashList = component$<{
  readonly entries: readonly AnnotationTrashEntry[];
  readonly notes: readonly AnnotationNote[];
  readonly busy: boolean;
  readonly hasMore: boolean;
  readonly onRestore$: QRL<(entry: AnnotationTrashEntry) => void>;
  readonly onRelink$: QRL<(entry: AnnotationTrashEntry) => void>;
  readonly onLoadMore$: QRL<() => void>;
}>((props) =>
  props.entries.length === 0 ? (
    <p class="workspace-muted">Trash is empty for this project.</p>
  ) : (
    <div class="trash-list">
      {props.entries.map((entry) => (
        <article key={entry.trashId}>
          <strong>{trashLabel(entry, props.notes)}</strong>
          <span>
            {entry.entryKind} · revision {entry.revision} · {entry.state}
          </span>
          <p>{entry.removalReason}</p>
          <small>{new Date(entry.removedAt).toLocaleString()}</small>
          <details>
            <summary>Technical identity</summary>
            <code>{entry.formerIdentity}</code>
          </details>
          {entry.state === "restored" ? null : (
            <div class="execution-actions">
              <button
                class="button secondary"
                disabled={props.busy}
                onClick$={() => props.onRestore$(entry)}
              >
                {entry.entryKind === "note" ? "Restore note" : "Propose restore"}
              </button>
              {entry.entryKind !== "note" ? null : (
                <button
                  class="button secondary"
                  disabled={props.busy}
                  onClick$={() => props.onRelink$(entry)}
                >
                  Relink note here
                </button>
              )}
            </div>
          )}
        </article>
      ))}
      {props.hasMore ? (
        <button class="button secondary" disabled={props.busy} onClick$={props.onLoadMore$}>
          Load more Trash
        </button>
      ) : null}
    </div>
  ),
);

function scopeKey(mode: "notes" | "trash", target: ProjectObjectReference): string {
  return `${mode}:${projectObjectReferenceKey(target)}`;
}

function trashLabel(entry: AnnotationTrashEntry, notes: readonly AnnotationNote[]): string {
  const note = notes.find((candidate) => entry.relatedNoteIds.includes(candidate.noteId));
  if (note !== undefined) return note.title;
  const value = entry.snapshot?.value;
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    const title = value["title"];
    if (typeof title === "string" && title.trim() !== "") return title;
  }
  return entry.entryKind === "object" ? "Removed project object" : "Archived note";
}
