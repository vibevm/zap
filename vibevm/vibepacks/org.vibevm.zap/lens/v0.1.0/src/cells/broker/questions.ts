/**
 * Question lifecycle and transactional compare-and-swap transitions.
 * @scope spec://org.vibevm.zap/lens/PROP-001#questions
 */
import { z } from "zod";

import {
  AmendAnswerInputSchema,
  AnswerQuestionInputSchema,
  AskInputSchema,
  BindingAuthSchema,
  CancelQuestionInputSchema,
  JsonValueSchema,
  PrincipalAuthSchema,
  QuestionListSchema,
  QuestionIdSchema,
  QuestionSchema,
  ScopedListInputSchema,
  type ActorId,
  type AmendAnswerInput,
  type AnswerQuestionInput,
  type AskInput,
  type BindingAuth,
  type CancelQuestionInput,
  type JsonValue,
  type MessageEnvelope,
  type PrincipalAuth,
  type PrincipalId,
  type Question,
  type QuestionList,
  type Result,
  type ScopedListInput,
} from "../protocol/index.ts";
import { fail, ok } from "./core.ts";
import { IdentityOperations } from "./identity.ts";

const QuestionMessageSchema = z.object({ messageId: z.string() });

export class QuestionOperations extends IdentityOperations {
  listQuestions(auth: PrincipalAuth, input: ScopedListInput): Result<QuestionList> {
    return this.safe(() => {
      const principal = this.principal(PrincipalAuthSchema.parse(auth));
      if (!principal.ok) return principal;
      const denied = this.requireCapability(principal.value, "events:read");
      if (denied !== null) return denied;
      const parsed = ScopedListInputSchema.parse(input);
      const scope = this.requireScope(principal.value, parsed.workspaceId, parsed.conversationId);
      if (scope !== null) return scope;
      const rows = this.database.all(
        `SELECT q.question_id AS questionId, a.host_kind AS hostKind,
                a.host_session_id AS hostSessionId, a.host_subagent_id AS hostSubagentId,
                q.origin_actor_id AS actorId,
                (SELECT COUNT(*) FROM question_answers qa WHERE qa.question_id = q.question_id)
                  AS answerCount
           FROM questions q JOIN actors a ON a.actor_id = q.origin_actor_id
          WHERE q.workspace_id = ? AND q.conversation_id = ?
          ORDER BY q.created_at, q.question_id LIMIT ?`,
        z
          .object({
            questionId: z.string(),
            hostKind: z.string(),
            hostSessionId: z.string().nullable(),
            hostSubagentId: z.string().nullable(),
            actorId: z.string(),
            answerCount: z.bigint(),
          })
          .strict(),
        [parsed.workspaceId, parsed.conversationId, parsed.limit + 1],
      );
      const questions = [];
      for (const row of rows.slice(0, parsed.limit)) {
        const question = this.question(row.questionId);
        if (question === null) {
          return fail(
            "not_found",
            "questions",
            "question disappeared during bounded listing",
            "retry the bounded scoped read",
          );
        }
        questions.push({
          question,
          addressedActorLabel:
            `${row.hostKind}:${row.hostSubagentId ?? row.hostSessionId ?? row.actorId}`.slice(
              0,
              256,
            ),
          amendmentCount: (row.answerCount > 0n ? row.answerCount - 1n : 0n).toString(),
        });
      }
      return ok(QuestionListSchema.parse({ questions, hasMore: rows.length > parsed.limit }));
    });
  }

  ask(auth: BindingAuth, input: AskInput): Result<Question> {
    return this.safe(() => {
      const parsed = AskInputSchema.parse(input);
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      const denied = this.requireCapability(actor.value, "question:ask");
      if (denied !== null) return denied;
      if (actor.value.state !== "active") return this.expiredActorFailure();
      return this.idempotent(
        actor.value.principalId,
        actor.value.actorId,
        parsed.clientRequestId,
        "ask",
        parsed,
        QuestionSchema,
        () => {
          const questionId = QuestionIdSchema.parse(this.id("qst"));
          const message = this.writeMessage({
            workspaceId: actor.value.workspaceId,
            conversationId: actor.value.conversationId,
            fromActorId: actor.value.actorId,
            toActorId: null,
            kind: "question.created",
            correlationId: questionId,
            causationId: null,
            payload: JsonValueSchema.parse({
              questionId,
              prompt: parsed.prompt,
              answerMode: parsed.answerMode,
              choices: parsed.choices,
              independentWorkAvailable: parsed.independentWorkAvailable,
            }),
          });
          const now = this.now();
          this.database.run(
            `INSERT INTO questions
               (question_id, message_id, workspace_id, conversation_id, origin_actor_id, prompt,
                answer_mode, choices_json, independent_work_available, reply_policy_json, state,
                revision, deadline_at, answer_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', 1, ?, NULL, ?, ?)`,
            [
              questionId,
              message.messageId,
              actor.value.workspaceId,
              actor.value.conversationId,
              actor.value.actorId,
              parsed.prompt,
              parsed.answerMode,
              JSON.stringify(parsed.choices),
              parsed.independentWorkAvailable ? 1n : 0n,
              JSON.stringify(actor.value.replyPolicy),
              parsed.deadlineAt ?? null,
              now,
              now,
            ],
          );
          const question = this.question(questionId);
          return question === null
            ? fail(
                "not_found",
                "questions",
                "created question disappeared",
                "inspect the question transaction",
              )
            : ok(question);
        },
      );
    });
  }

  answer(auth: PrincipalAuth, input: AnswerQuestionInput): Result<Question> {
    return this.answerOrAmend(auth, input, false);
  }

  amendAnswer(auth: PrincipalAuth, input: AmendAnswerInput): Result<Question> {
    return this.answerOrAmend(auth, input, true);
  }

  cancelQuestion(auth: BindingAuth, input: CancelQuestionInput): Result<Question> {
    return this.safe(() => {
      const parsed = CancelQuestionInputSchema.parse(input);
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      const denied = this.requireCapability(actor.value, "question:cancel");
      if (denied !== null) return denied;
      return this.idempotent(
        actor.value.principalId,
        actor.value.actorId,
        parsed.clientRequestId,
        "cancel_question",
        parsed,
        QuestionSchema,
        () => {
          const question = this.question(parsed.questionId);
          if (question === null) {
            return fail(
              "not_found",
              "questions",
              "question does not exist",
              "use a durable question ID from ask",
            );
          }
          if (question.originActorId !== actor.value.actorId) {
            return fail(
              "forbidden",
              "questions",
              "only the origin actor can cancel this question",
              "route cancellation through the actor that asked it",
            );
          }
          if (question.state !== "open") {
            return fail(
              "conflict",
              "questions",
              `question is already ${question.state}`,
              "read the current question state before retrying",
            );
          }
          if (question.revision !== parsed.expectedRevision) return this.staleQuestion(question);
          this.database.run(
            `UPDATE questions SET state = 'cancelled', revision = revision + 1, updated_at = ?
              WHERE question_id = ?`,
            [this.now(), question.questionId],
          );
          this.writeQuestionEvent(question, "question.cancelled", null, actor.value.actorId);
          const updated = this.question(question.questionId);
          return updated === null
            ? fail(
                "not_found",
                "questions",
                "cancelled question disappeared",
                "inspect the question transaction",
              )
            : ok(updated);
        },
      );
    });
  }

  expireQuestions(limit = 100): Result<readonly Question[]> {
    return this.safe(() =>
      this.database.transaction(() => {
        const boundedLimit = z.number().int().min(1).max(100).parse(limit);
        const ids = this.database.all(
          `SELECT question_id AS questionId FROM questions
            WHERE state = 'open' AND deadline_at IS NOT NULL AND deadline_at <= ?
            ORDER BY deadline_at, question_id LIMIT ?`,
          z.object({ questionId: z.string() }),
          [this.now(), boundedLimit],
        );
        const expired: Question[] = [];
        for (const item of ids) {
          const question = this.question(item.questionId);
          if (question === null) continue;
          this.database.run(
            `UPDATE questions SET state = 'expired', revision = revision + 1, updated_at = ?
              WHERE question_id = ? AND state = 'open'`,
            [this.now(), item.questionId],
          );
          const event = this.writeQuestionEvent(question, "question.expired", null, null);
          this.writeDelivery(event, question.originActorId, question.originActorId);
          const updated = this.question(item.questionId);
          if (updated !== null) expired.push(updated);
        }
        return ok(expired);
      }),
    );
  }

  private answerOrAmend(
    auth: PrincipalAuth,
    input: AnswerQuestionInput,
    amend: boolean,
  ): Result<Question> {
    return this.safe(() => {
      const parsed = (amend ? AmendAnswerInputSchema : AnswerQuestionInputSchema).parse(input);
      const principal = this.principal(PrincipalAuthSchema.parse(auth));
      if (!principal.ok) return principal;
      const denied = this.requireCapability(
        principal.value,
        amend ? "question:amend" : "question:answer",
      );
      if (denied !== null) return denied;
      const scopeError = this.requireScope(
        principal.value,
        parsed.workspaceId,
        parsed.conversationId,
      );
      if (scopeError !== null) return scopeError;
      return this.idempotent(
        principal.value.principalId,
        parsed.questionId,
        parsed.clientRequestId,
        amend ? "amend_answer" : "answer",
        parsed,
        QuestionSchema,
        () => this.commitAnswer(principal.value.principalId, parsed, amend),
      );
    });
  }

  private commitAnswer(
    principalId: PrincipalId,
    parsed: z.output<typeof AnswerQuestionInputSchema>,
    amend: boolean,
  ): Result<Question> {
    const question = this.question(parsed.questionId);
    if (question === null) {
      return fail(
        "not_found",
        "questions",
        "question does not exist",
        "use a durable question ID from an event",
      );
    }
    if (
      question.workspaceId !== parsed.workspaceId ||
      question.conversationId !== parsed.conversationId
    ) {
      return fail(
        "forbidden",
        "authority",
        "question is outside the requested scope",
        "answer it through the matching workspace and conversation",
      );
    }
    if (question.revision !== parsed.expectedRevision) return this.staleQuestion(question);
    if (!amend && question.state === "answered") {
      return fail(
        "already_answered",
        "questions",
        "question already has an accepted answer",
        "use amendAnswer with the current revision when authorized",
      );
    }
    if (amend ? question.state !== "answered" : question.state !== "open") {
      return fail(
        "conflict",
        "questions",
        `question is ${question.state}`,
        "read the current question state before retrying",
      );
    }
    if (!amend && question.deadlineAt !== null && question.deadlineAt <= this.now()) {
      this.database.run(
        `UPDATE questions SET state = 'expired', revision = revision + 1, updated_at = ?
          WHERE question_id = ? AND state = 'open'`,
        [this.now(), question.questionId],
      );
      const event = this.writeQuestionEvent(question, "question.expired", null, null);
      this.writeDelivery(event, question.originActorId, question.originActorId);
      return fail(
        "conflict",
        "questions",
        "question deadline elapsed before the answer transaction",
        "refresh the expired question instead of applying a late answer",
      );
    }
    if (
      question.answerMode === "single_choice" &&
      (typeof parsed.answer !== "string" ||
        !question.choices.some((choice) => choice.id === parsed.answer))
    ) {
      return fail(
        "invalid_input",
        "questions",
        "answer is not one of the declared choices",
        "submit one stable choice ID from the question",
      );
    }
    const revision = BigInt(question.revision) + 1n;
    const now = this.now();
    this.database.run(
      `INSERT INTO question_answers(question_id, revision, principal_id, answer_json, created_at)
       VALUES (?, ?, ?, ?, ?)`,
      [question.questionId, revision, principalId, JSON.stringify(parsed.answer), now],
    );
    this.database.run(
      `UPDATE questions SET state = 'answered', revision = ?, answer_json = ?, updated_at = ?
        WHERE question_id = ?`,
      [revision, JSON.stringify(parsed.answer), now, question.questionId],
    );
    const message = this.writeQuestionEvent(
      question,
      amend ? "question.amended" : "question.answered",
      parsed.answer,
      null,
    );
    this.deliverAnswer(question, message);
    const updated = this.question(question.questionId);
    return updated === null
      ? fail(
          "not_found",
          "questions",
          "answered question disappeared",
          "inspect the question transaction",
        )
      : ok(updated);
  }

  private writeQuestionEvent(
    question: Question,
    kind: "question.answered" | "question.amended" | "question.cancelled" | "question.expired",
    answer: JsonValue | null,
    fromActorId: ActorId | null,
  ): MessageEnvelope {
    const source = this.database.get(
      `SELECT message_id AS messageId FROM questions WHERE question_id = ?`,
      QuestionMessageSchema,
      [question.questionId],
    );
    return this.writeMessage({
      workspaceId: question.workspaceId,
      conversationId: question.conversationId,
      fromActorId,
      toActorId: question.originActorId,
      kind,
      correlationId: question.questionId,
      causationId: source === null ? null : z.string().brand<"MessageId">().parse(source.messageId),
      payload: JsonValueSchema.parse({
        questionId: question.questionId,
        revision: (BigInt(question.revision) + 1n).toString(),
        answer,
      }),
    });
  }

  private deliverAnswer(question: Question, message: MessageEnvelope): void {
    const original = this.writeDelivery(message, question.originActorId, question.originActorId);
    const origin = this.actor(question.originActorId);
    if (
      origin !== null &&
      origin.state === "expired" &&
      question.replyPolicy.kind === "forward_parent" &&
      origin.parentActorId !== null
    ) {
      this.writeDelivery(message, question.originActorId, origin.parentActorId, original);
    }
  }

  private staleQuestion(question: Question): Result<Question> {
    return fail(
      "stale_revision",
      "questions",
      `expected revision does not match current revision ${question.revision}`,
      "refresh the question and retry only against its exact current revision",
    );
  }
}
