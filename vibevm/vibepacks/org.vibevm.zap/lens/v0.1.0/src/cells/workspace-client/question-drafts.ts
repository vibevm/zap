/** Client-tab-local ZapAskUserQuestion drafts. @scope spec://org.vibevm.zap/lens/PROP-010#human-questions */
import {
  QuestionDraftSchema,
  type QuestionDraft,
  type ProjectId,
  type QuestionGroupId,
  type WorkContextId,
} from "../workspace-model/index.ts";

export interface QuestionDraftStore {
  load(input: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly questionGroupId: QuestionGroupId;
  }):
    | { readonly ok: true; readonly value: QuestionDraft | null }
    | { readonly ok: false; readonly message: string };
  save(input: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly draft: QuestionDraft;
  }): { readonly ok: true } | { readonly ok: false; readonly message: string };
  remove(input: {
    readonly projectId: ProjectId;
    readonly contextId: WorkContextId;
    readonly questionGroupId: QuestionGroupId;
  }): { readonly ok: true } | { readonly ok: false; readonly message: string };
}

const DRAFT_PREFIX = "zap.quicklens.question-draft.v1";

export function createLocalQuestionDraftStore(storage?: Storage): QuestionDraftStore {
  let resolvedStorage = storage;
  if (resolvedStorage === undefined && typeof window !== "undefined") {
    try {
      resolvedStorage = window.sessionStorage;
    } catch {
      resolvedStorage = undefined;
    }
  }
  return {
    load(input) {
      if (resolvedStorage === undefined) return unavailable();
      try {
        const raw = resolvedStorage.getItem(
          key(input.projectId, input.contextId, input.questionGroupId),
        );
        if (raw === null) return { ok: true, value: null };
        const parsed = QuestionDraftSchema.safeParse(JSON.parse(raw));
        return parsed.success
          ? { ok: true, value: parsed.data }
          : { ok: false, message: "Saved draft is invalid and was ignored." };
      } catch {
        return { ok: false, message: "Saved draft could not be read from this browser." };
      }
    },
    save(input) {
      if (resolvedStorage === undefined) return unavailable();
      try {
        resolvedStorage.setItem(
          key(input.projectId, input.contextId, input.draft.questionGroupId),
          JSON.stringify(QuestionDraftSchema.parse(input.draft)),
        );
        return { ok: true };
      } catch {
        return { ok: false, message: "Draft could not be saved in this browser." };
      }
    },
    remove(input) {
      if (resolvedStorage === undefined) return unavailable();
      try {
        resolvedStorage.removeItem(key(input.projectId, input.contextId, input.questionGroupId));
        return { ok: true };
      } catch {
        return { ok: false, message: "Saved draft could not be removed from this browser." };
      }
    },
  };
}

function key(
  projectId: ProjectId,
  contextId: WorkContextId,
  questionGroupId: QuestionGroupId,
): string {
  return `${DRAFT_PREFIX}:${projectId}:${contextId}:${questionGroupId}`;
}

function unavailable(): { readonly ok: false; readonly message: string } {
  return { ok: false, message: "Draft storage is unavailable in this client." };
}
