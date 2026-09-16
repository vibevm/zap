/** Durable staged metadata authoring across independent MCP processes. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { deserialize, serialize } from "node:v8";
import { z } from "zod";
import {
  MetadataReceiptSchema,
  PreparedAssessmentProposalSchema,
  PreparedPlanProposalSchema,
  PreparedSuccessorSchema,
  SuccessorPlanAuthoringInputSchema,
  type MetadataReceipt,
  type PlanAuthoringPort,
  type PreparedSuccessor,
  type SuccessorPlanAuthoringInput,
} from "../plan-authoring/index.ts";
import { ConversationIdSchema, WorkspaceIdSchema, type Result } from "../protocol/index.ts";
import { encodeCanonicalJson, type SubmissionStatus } from "../zap-client/index.ts";

const JournalSchema = z
  .object({
    input: SuccessorPlanAuthoringInputSchema,
    inputDigest: z.string().regex(/^[0-9a-f]{64}$/),
    phase: z.enum([
      "plan_prepared",
      "plan_uncertain",
      "plan_committed",
      "assessment_prepared",
      "assessment_uncertain",
      "assessment_committed",
      "finished",
      "rejected",
    ]),
    plan: PreparedPlanProposalSchema,
    planReceipt: MetadataReceiptSchema.nullable(),
    assessment: PreparedAssessmentProposalSchema.nullable(),
    assessmentReceipt: MetadataReceiptSchema.nullable(),
    successor: PreparedSuccessorSchema.nullable(),
    message: z.string(),
  })
  .strict();
type Journal = z.infer<typeof JournalSchema>;
type Phase = Journal["phase"];

export interface DurablePlanAuthoring {
  author(input: SuccessorPlanAuthoringInput): Promise<Result<PreparedSuccessor>>;
  prepared(operationId: string): Result<PreparedSuccessor>;
  close(): void;
}

export function openDurablePlanAuthoring(
  path: string,
  scope: { readonly workspaceId: string; readonly conversationId: string },
  authoring: PlanAuthoringPort,
): Result<DurablePlanAuthoring> {
  try {
    const workspaceId = WorkspaceIdSchema.parse(scope.workspaceId);
    const conversationId = ConversationIdSchema.parse(scope.conversationId);
    const database = new DatabaseSync(path);
    database.exec("PRAGMA busy_timeout = 5000");
    database.exec("PRAGMA journal_mode = WAL");
    database.exec(
      "CREATE TABLE IF NOT EXISTS quicklens_plan_authoring (operation_id TEXT NOT NULL, workspace_id TEXT NOT NULL, conversation_id TEXT NOT NULL, phase TEXT NOT NULL, value_blob BLOB NOT NULL, PRIMARY KEY(operation_id,workspace_id,conversation_id)) STRICT",
    );
    const get = database.prepare(
      "SELECT value_blob FROM quicklens_plan_authoring WHERE operation_id=? AND workspace_id=? AND conversation_id=?",
    );
    const insert = database.prepare(
      "INSERT INTO quicklens_plan_authoring(operation_id,workspace_id,conversation_id,phase,value_blob) VALUES(?,?,?,?,?)",
    );
    const update = database.prepare(
      "UPDATE quicklens_plan_authoring SET phase=?,value_blob=? WHERE operation_id=? AND workspace_id=? AND conversation_id=? AND phase=?",
    );
    const load = (operationId: string): Result<Journal | null> => {
      const raw = get.get(operationId, workspaceId, conversationId);
      if (raw === undefined) return ok(null);
      const row = z.looseObject({ value_blob: z.instanceof(Uint8Array) }).safeParse(raw);
      if (!row.success) return failure("journal row is malformed");
      try {
        const parsed = JournalSchema.safeParse(deserialize(row.data.value_blob));
        return parsed.success ? ok(parsed.data) : failure("journal value is malformed");
      } catch {
        return failure("journal value cannot be decoded");
      }
    };
    const create = (record: Journal): Result<Journal> => {
      try {
        insert.run(
          record.input.operationId,
          workspaceId,
          conversationId,
          record.phase,
          serialize(record),
        );
        return ok(record);
      } catch {
        const current = load(record.input.operationId);
        return current.ok && current.value?.inputDigest === record.inputDigest
          ? ok(current.value)
          : failure("operation identity is already bound to different authoring bytes");
      }
    };
    const transition = (
      record: Journal,
      phase: Phase,
      patch: Partial<Journal>,
    ): Result<Journal> => {
      const next = JournalSchema.parse({ ...record, ...patch, phase });
      try {
        const changed = update.run(
          next.phase,
          serialize(next),
          record.input.operationId,
          workspaceId,
          conversationId,
          record.phase,
        );
        return changed.changes === 1
          ? ok(next)
          : failure("authoring stage lost its compare-and-set; reload before retry");
      } catch {
        return failure("authoring journal transition failed");
      }
    };
    return ok({
      author: (input) => run(authoring, input, load, create, transition),
      prepared: (operationId) => {
        const record = load(operationId);
        return record.ok && record.value?.phase === "finished" && record.value.successor !== null
          ? ok(record.value.successor)
          : failure("prepared successor is unavailable for this operation identity");
      },
      close: () => {
        database.close();
      },
    });
  } catch {
    return failure("authoring journal could not be opened");
  }
}

async function run(
  port: PlanAuthoringPort,
  input: SuccessorPlanAuthoringInput,
  load: (operationId: string) => Result<Journal | null>,
  create: (record: Journal) => Result<Journal>,
  transition: (record: Journal, phase: Phase, patch: Partial<Journal>) => Result<Journal>,
): Promise<Result<PreparedSuccessor>> {
  const parsed = SuccessorPlanAuthoringInputSchema.safeParse(input);
  if (!parsed.success) return failure("successor authoring input is invalid");
  const digest = createHash("sha256").update(encodeCanonicalJson(parsed.data)).digest("hex");
  const loaded = load(parsed.data.operationId);
  if (!loaded.ok) return loaded;
  let record = loaded.value;
  if (record !== null && record.inputDigest !== digest) {
    return failure("operation identity is already bound to different authoring bytes");
  }
  if (record === null) {
    const plan = await port.prepareSuccessor(parsed.data);
    if (!plan.ok) return failure(plan.error.message);
    const created = create({
      input: parsed.data,
      inputDigest: digest,
      phase: "plan_prepared",
      plan: plan.value,
      planReceipt: null,
      assessment: null,
      assessmentReceipt: null,
      successor: null,
      message: "Plan metadata command is durably prepared.",
    });
    if (!created.ok) return created;
    record = created.value;
  }
  for (let step = 0; step < 6; step += 1) {
    if (record.phase === "finished" && record.successor !== null) return ok(record.successor);
    if (record.phase === "rejected") return failure(record.message);
    if (record.phase === "plan_prepared" || record.phase === "plan_uncertain") {
      const result = await metadata(
        port,
        record,
        record.plan.command,
        "plan_uncertain",
        transition,
      );
      if (!result.ok) return result;
      if (result.value.refused !== null) {
        transition(result.value.pending, "rejected", { message: result.value.refused });
        return failure(result.value.refused);
      }
      if (result.value.receipt === null) return failure("plan metadata outcome is uncertain");
      const changed = transition(result.value.pending, "plan_committed", {
        planReceipt: result.value.receipt,
        message: "Plan metadata is committed.",
      });
      if (!changed.ok) return changed;
      record = changed.value;
    }
    if (record.phase === "plan_committed") {
      if (record.planReceipt === null) return failure("plan receipt is unavailable");
      const assessment = await port.prepareAssessment(record.plan, record.planReceipt);
      if (!assessment.ok) return failure(assessment.error.message);
      const changed = transition(record, "assessment_prepared", {
        assessment: assessment.value,
        message: "Assessment metadata command is durably prepared.",
      });
      if (!changed.ok) return changed;
      record = changed.value;
    }
    if (record.phase === "assessment_prepared" || record.phase === "assessment_uncertain") {
      if (record.assessment === null) return failure("assessment command is unavailable");
      const result = await metadata(
        port,
        record,
        record.assessment.command,
        "assessment_uncertain",
        transition,
      );
      if (!result.ok) return result;
      if (result.value.refused !== null) {
        transition(result.value.pending, "rejected", { message: result.value.refused });
        return failure(result.value.refused);
      }
      if (result.value.receipt === null) return failure("assessment metadata outcome is uncertain");
      const changed = transition(result.value.pending, "assessment_committed", {
        assessmentReceipt: result.value.receipt,
        message: "Assessment metadata is committed.",
      });
      if (!changed.ok) return changed;
      record = changed.value;
    }
    if (record.phase === "assessment_committed") {
      if (record.assessment === null || record.assessmentReceipt === null) {
        return failure("assessment completion state is incomplete");
      }
      const successor = await port.finishSuccessor(record.assessment, record.assessmentReceipt);
      if (!successor.ok) return failure(successor.error.message);
      const changed = transition(record, "finished", {
        successor: successor.value,
        message: "Successor metadata is durably complete.",
      });
      return changed.ok ? ok(successor.value) : changed;
    }
  }
  return failure("authoring stage budget was exhausted");
}

async function metadata(
  port: PlanAuthoringPort,
  record: Journal,
  command: Journal["plan"]["command"],
  uncertainPhase: "plan_uncertain" | "assessment_uncertain",
  transition: (record: Journal, phase: Phase, patch: Partial<Journal>) => Result<Journal>,
): Promise<Result<{ receipt: MetadataReceipt | null; refused: string | null; pending: Journal }>> {
  let pending = record;
  if (record.phase !== uncertainPhase) {
    const changed = transition(record, uncertainPhase, {
      message: "Metadata command is durably pending reconciliation.",
    });
    if (!changed.ok) return changed;
    pending = changed.value;
  }
  let status = await port.reconcileMetadata({
    command_id: command.command.frame.header.command_id,
    command_digest: command.commandDigest,
  });
  if (!status.ok) return ok({ receipt: null, refused: null, pending });
  if (status.value.status === "not_committed") status = await port.submitMetadata(command);
  if (!status.ok) {
    return ok({
      receipt: null,
      refused: status.error.code === "refused" ? status.error.message : null,
      pending,
    });
  }
  if (status.value.status !== "committed") {
    return ok({ receipt: null, refused: null, pending });
  }
  return ok({ receipt: receipt(status.value, command.commandDigest), refused: null, pending });
}

function receipt(status: Extract<SubmissionStatus, { status: "committed" }>, digest: string) {
  return MetadataReceiptSchema.parse({
    commandId: status.receipt.command_id,
    commandDigest: digest,
    revision: status.receipt.revision,
  });
}

function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

function failure(message: string): Result<never> {
  return {
    ok: false,
    error: {
      code: "conflict",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: ${message}; fix surface: retry the same operation identity through durable metadata reconciliation`,
    },
  };
}
