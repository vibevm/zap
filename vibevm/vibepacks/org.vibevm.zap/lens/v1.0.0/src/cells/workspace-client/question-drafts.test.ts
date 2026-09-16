import assert from "node:assert/strict";
import test from "node:test";

import {
  ProjectIdSchema,
  QuestionDraftSchema,
  QuestionGroupIdSchema,
  WorkContextIdSchema,
} from "../workspace-model/index.ts";
import { createLocalQuestionDraftStore } from "./question-drafts.ts";

test("local question drafts are isolated by project/context/group and survive store recreation", () => {
  const storage = new MemoryStorage();
  const store = createLocalQuestionDraftStore(storage);
  const projectId = ProjectIdSchema.parse("project.draft");
  const contextId = WorkContextIdSchema.parse("context.draft");
  const questionGroupId = QuestionGroupIdSchema.parse("question-group.draft");
  const draft = QuestionDraftSchema.parse({
    questionGroupId,
    expectedRevision: "3",
    submission: {
      answers: [{ questionItemId: "question-item.draft", answer: { kind: "skipped" } }],
      noteMarkdown: null,
    },
    savedAt: "2026-09-15T20:00:00.000Z",
  });
  assert.equal(store.save({ projectId, contextId, draft }).ok, true);
  const reopened = createLocalQuestionDraftStore(storage).load({
    projectId,
    contextId,
    questionGroupId,
  });
  assert.deepEqual(reopened, { ok: true, value: draft });
  const other = createLocalQuestionDraftStore(storage).load({
    projectId: ProjectIdSchema.parse("project.other-draft"),
    contextId,
    questionGroupId,
  });
  assert.deepEqual(other, { ok: true, value: null });
  assert.equal(store.remove({ projectId, contextId, questionGroupId }).ok, true);
  assert.deepEqual(store.load({ projectId, contextId, questionGroupId }), {
    ok: true,
    value: null,
  });
});

test("separate client tabs do not overwrite each other's same question draft", () => {
  const firstTab = createLocalQuestionDraftStore(new MemoryStorage());
  const secondTab = createLocalQuestionDraftStore(new MemoryStorage());
  const scope = {
    projectId: ProjectIdSchema.parse("project.same"),
    contextId: WorkContextIdSchema.parse("context.same"),
    questionGroupId: QuestionGroupIdSchema.parse("question-group.same"),
  };
  const draft = QuestionDraftSchema.parse({
    questionGroupId: scope.questionGroupId,
    expectedRevision: "1",
    submission: {
      answers: [{ questionItemId: "question-item.same", answer: { kind: "skipped" } }],
      noteMarkdown: null,
    },
    savedAt: "2026-09-15T20:00:00.000Z",
  });
  assert.equal(firstTab.save({ ...scope, draft }).ok, true);
  assert.deepEqual(secondTab.load(scope), { ok: true, value: null });
});

class MemoryStorage implements Storage {
  readonly #values = new Map<string, string>();
  get length(): number {
    return this.#values.size;
  }
  clear(): void {
    this.#values.clear();
  }
  getItem(key: string): string | null {
    return this.#values.get(key) ?? null;
  }
  key(index: number): string | null {
    return [...this.#values.keys()][index] ?? null;
  }
  removeItem(key: string): void {
    this.#values.delete(key);
  }
  setItem(key: string, value: string): void {
    this.#values.set(key, value);
  }
}
