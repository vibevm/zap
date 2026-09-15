/** Durable composite milestone and successor authoring. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { deserialize, serialize } from "node:v8";
import { z } from "zod";
import {
  MetadataReceiptSchema,
  MilestonePrecursorAuthoringInputSchema,
  PreparedAssessmentProposalSchema,
  PreparedCompositePlanProposalSchema,
  PreparedMilestonePrecursorsSchema,
  PreparedSuccessorSchema,
  SuccessorPlanAuthoringInputSchema,
  type MetadataReceipt,
  type MilestonePrecursorAuthoringInput,
  type PlanAuthoringPort,
  type PreparedMilestonePrecursors,
  type PreparedSuccessor,
} from "../plan-authoring/index.ts";
import { ConversationIdSchema, WorkspaceIdSchema, type Result } from "../protocol/index.ts";
import { encodeCanonicalJson, type SubmissionStatus } from "../zap-client/index.ts";

export const CompositeSuccessorAuthoringInputSchema = z
  .object({
    successor: SuccessorPlanAuthoringInputSchema,
  })
  .strict();
const FullCompositeInputSchema = z
  .object({
    precursors: MilestonePrecursorAuthoringInputSchema,
    successor: SuccessorPlanAuthoringInputSchema,
  })
  .strict()
  .superRefine((input, context) => {
    if (
      input.precursors.operationId !== input.successor.operationId ||
      encodeCanonicalJson(input.precursors.intentBasis).toString() !==
        encodeCanonicalJson(input.successor.intentBasis).toString()
    ) {
      context.addIssue({
        code: "custom",
        message: "composite stages must share identity and basis",
      });
    }
  });
export type CompositeSuccessorAuthoringInput = z.infer<
  typeof CompositeSuccessorAuthoringInputSchema
>;

const RecordSchema = z
  .object({
    input: FullCompositeInputSchema,
    inputDigest: z.string().regex(/^[0-9a-f]{64}$/),
    phase: z.enum([
      "composite_prepared",
      "composite_uncertain",
      "composite_committed",
      "assessment_prepared",
      "assessment_uncertain",
      "assessment_committed",
      "finished",
      "rejected",
    ]),
    composite: PreparedCompositePlanProposalSchema,
    planReceipt: MetadataReceiptSchema.nullable(),
    assessment: PreparedAssessmentProposalSchema.nullable(),
    assessmentReceipt: MetadataReceiptSchema.nullable(),
    successor: PreparedSuccessorSchema.nullable(),
    message: z.string(),
  })
  .strict();
type Record = z.infer<typeof RecordSchema>;
type Phase = Record["phase"];

export interface DurableCompositePlanAuthoring {
  prepareComposite(
    input: MilestonePrecursorAuthoringInput,
  ): Promise<Result<PreparedMilestonePrecursors>>;
  authorComposite(input: CompositeSuccessorAuthoringInput): Promise<Result<PreparedSuccessor>>;
  prepared(operationId: string): Result<PreparedSuccessor>;
  close(): void;
}

export function openDurableCompositePlanAuthoring(
  path: string,
  scope: { readonly workspaceId: string; readonly conversationId: string },
  port: PlanAuthoringPort,
): Result<DurableCompositePlanAuthoring> {
  try {
    const workspace = WorkspaceIdSchema.parse(scope.workspaceId);
    const conversation = ConversationIdSchema.parse(scope.conversationId);
    const database = new DatabaseSync(path);
    database.exec("PRAGMA busy_timeout = 5000");
    database.exec("PRAGMA journal_mode = WAL");
    database.exec(
      "CREATE TABLE IF NOT EXISTS quicklens_composite_authoring (operation_id TEXT NOT NULL, workspace_id TEXT NOT NULL, conversation_id TEXT NOT NULL, phase TEXT NOT NULL, value_blob BLOB NOT NULL, PRIMARY KEY(operation_id,workspace_id,conversation_id)) STRICT",
    );
    database.exec(
      "CREATE TABLE IF NOT EXISTS quicklens_composite_precursors (operation_id TEXT NOT NULL, workspace_id TEXT NOT NULL, conversation_id TEXT NOT NULL, input_digest TEXT NOT NULL, input_blob BLOB NOT NULL, value_blob BLOB NOT NULL, PRIMARY KEY(operation_id,workspace_id,conversation_id)) STRICT",
    );
    const get = database.prepare(
      "SELECT value_blob FROM quicklens_composite_authoring WHERE operation_id=? AND workspace_id=? AND conversation_id=?",
    );
    const insert = database.prepare(
      "INSERT INTO quicklens_composite_authoring(operation_id,workspace_id,conversation_id,phase,value_blob) VALUES(?,?,?,?,?)",
    );
    const update = database.prepare(
      "UPDATE quicklens_composite_authoring SET phase=?,value_blob=? WHERE operation_id=? AND workspace_id=? AND conversation_id=? AND phase=?",
    );
    const precursorGet = database.prepare(
      "SELECT input_digest,input_blob,value_blob FROM quicklens_composite_precursors WHERE operation_id=? AND workspace_id=? AND conversation_id=?",
    );
    const precursorInsert = database.prepare(
      "INSERT INTO quicklens_composite_precursors(operation_id,workspace_id,conversation_id,input_digest,input_blob,value_blob) VALUES(?,?,?,?,?,?)",
    );
    const load = (operationId: string): Result<Record | null> => {
      const raw = get.get(operationId, workspace, conversation);
      if (raw === undefined) return ok(null);
      const row = z.looseObject({ value_blob: z.instanceof(Uint8Array) }).safeParse(raw);
      if (!row.success) return fail("composite journal row is malformed");
      try {
        const value = RecordSchema.safeParse(deserialize(row.data.value_blob));
        return value.success ? ok(value.data) : fail("composite journal value is malformed");
      } catch {
        return fail("composite journal value cannot be decoded");
      }
    };
    const create = (record: Record): Result<Record> => {
      try {
        insert.run(
          record.input.successor.operationId,
          workspace,
          conversation,
          record.phase,
          serialize(record),
        );
        return ok(record);
      } catch {
        const current = load(record.input.successor.operationId);
        return current.ok && current.value?.inputDigest === record.inputDigest
          ? ok(current.value)
          : fail("operation identity is bound to different composite authoring bytes");
      }
    };
    const transition = (record: Record, phase: Phase, patch: Partial<Record>): Result<Record> => {
      const next = RecordSchema.parse({ ...record, ...patch, phase });
      try {
        const changed = update.run(
          next.phase,
          serialize(next),
          record.input.successor.operationId,
          workspace,
          conversation,
          record.phase,
        );
        return changed.changes === 1 ? ok(next) : fail("composite journal compare-and-set failed");
      } catch {
        return fail("composite journal transition failed");
      }
    };
    return ok({
      async prepareComposite(raw) {
        const input = MilestonePrecursorAuthoringInputSchema.safeParse(raw);
        if (!input.success) return fail("milestone precursor input is invalid");
        const digest = hash(input.data);
        const existing = readPrecursors(
          precursorGet.get(input.data.operationId, workspace, conversation),
          digest,
        );
        if (!existing.ok) return existing;
        if (existing.value !== null) return ok(existing.value.prepared);
        const prepared = await port.prepareMilestonePrecursors(input.data);
        if (!prepared.ok) return fail(prepared.error.message);
        try {
          precursorInsert.run(
            input.data.operationId,
            workspace,
            conversation,
            digest,
            serialize(input.data),
            serialize(prepared.value),
          );
          return ok(prepared.value);
        } catch {
          const raced = readPrecursors(
            precursorGet.get(input.data.operationId, workspace, conversation),
            digest,
          );
          return raced.ok && raced.value !== null
            ? ok(raced.value.prepared)
            : fail("milestone precursor journal write failed");
        }
      },
      async authorComposite(raw) {
        const input = CompositeSuccessorAuthoringInputSchema.safeParse(raw);
        if (!input.success) return fail("composite successor input is invalid");
        const stored = readPrecursors(
          precursorGet.get(input.data.successor.operationId, workspace, conversation),
        );
        if (!stored.ok) return stored;
        if (stored.value === null) return fail("milestone precursors must be prepared first");
        return run(
          port,
          { precursors: stored.value.input, successor: input.data.successor },
          stored.value.prepared,
          load,
          create,
          transition,
        );
      },
      prepared(operationId) {
        const record = load(operationId);
        return record.ok && record.value?.phase === "finished" && record.value.successor !== null
          ? ok(record.value.successor)
          : fail("prepared composite successor is unavailable for this operation identity");
      },
      close: () => {
        database.close();
      },
    });
  } catch {
    return fail("composite authoring journal could not be opened");
  }
}

async function run(
  port: PlanAuthoringPort,
  raw: z.infer<typeof FullCompositeInputSchema>,
  preparedPrecursors: PreparedMilestonePrecursors,
  load: (id: string) => Result<Record | null>,
  create: (record: Record) => Result<Record>,
  transition: (record: Record, phase: Phase, patch: Partial<Record>) => Result<Record>,
): Promise<Result<PreparedSuccessor>> {
  const input = FullCompositeInputSchema.safeParse(raw);
  if (!input.success) return fail("composite successor input is invalid");
  const digest = createHash("sha256").update(encodeCanonicalJson(input.data)).digest("hex");
  const loaded = load(input.data.successor.operationId);
  if (!loaded.ok) return loaded;
  let record = loaded.value;
  if (record !== null && record.inputDigest !== digest)
    return fail("operation identity is bound to different composite authoring bytes");
  if (record === null) {
    const composite = await port.prepareCompositeSuccessor(
      input.data.successor,
      preparedPrecursors,
    );
    if (!composite.ok) return fail(composite.error.message);
    const created = create({
      input: input.data,
      inputDigest: digest,
      phase: "composite_prepared",
      composite: composite.value,
      planReceipt: null,
      assessment: null,
      assessmentReceipt: null,
      successor: null,
      message: "Composite candidate is durably prepared.",
    });
    if (!created.ok) return created;
    record = created.value;
  }
  for (let budget = 0; budget < 7; budget += 1) {
    if (record.phase === "finished" && record.successor !== null) return ok(record.successor);
    if (record.phase === "rejected") return fail(record.message);
    if (record.phase === "composite_prepared" || record.phase === "composite_uncertain") {
      const pending = ensurePending(record, "composite_uncertain", transition);
      if (!pending.ok) return pending;
      const status = await compositeMetadata(port, pending.value);
      if (!status.ok) {
        if (status.error.code === "conflict")
          transition(pending.value, "rejected", { message: status.error.message });
        return status;
      }
      if (status.value === null) return fail("composite candidate outcome is uncertain");
      const changed = transition(pending.value, "composite_committed", {
        planReceipt: status.value,
        message: "Composite candidate is committed.",
      });
      if (!changed.ok) return changed;
      record = changed.value;
    }
    if (record.phase === "composite_committed") {
      if (record.planReceipt === null) return fail("composite candidate receipt is unavailable");
      const assessment = await port.prepareCompositeAssessment(
        record.composite,
        record.planReceipt,
      );
      if (!assessment.ok) return fail(assessment.error.message);
      const changed = transition(record, "assessment_prepared", {
        assessment: assessment.value,
        message: "Composite assessment is durably prepared.",
      });
      if (!changed.ok) return changed;
      record = changed.value;
    }
    if (record.phase === "assessment_prepared" || record.phase === "assessment_uncertain") {
      if (record.assessment === null) return fail("composite assessment is unavailable");
      const pending = ensurePending(record, "assessment_uncertain", transition);
      if (!pending.ok) return pending;
      const assessment = pending.value.assessment;
      if (assessment === null) return fail("composite assessment is unavailable after transition");
      const status = await commandMetadata(port, assessment.command);
      if (!status.ok) {
        if (status.error.code === "conflict")
          transition(pending.value, "rejected", { message: status.error.message });
        return status;
      }
      if (status.value === null) return fail("composite assessment outcome is uncertain");
      const changed = transition(pending.value, "assessment_committed", {
        assessmentReceipt: status.value,
        message: "Composite assessment is committed.",
      });
      if (!changed.ok) return changed;
      record = changed.value;
    }
    if (record.phase === "assessment_committed") {
      if (record.assessment === null || record.assessmentReceipt === null)
        return fail("composite assessment completion state is incomplete");
      const successor = await port.finishSuccessor(record.assessment, record.assessmentReceipt);
      if (!successor.ok) return fail(successor.error.message);
      const changed = transition(record, "finished", {
        successor: successor.value,
        message: "Composite successor is complete.",
      });
      return changed.ok ? ok(successor.value) : changed;
    }
  }
  return fail("composite authoring stage budget was exhausted");
}

function ensurePending(
  record: Record,
  phase: "composite_uncertain" | "assessment_uncertain",
  transition: (record: Record, phase: Phase, patch: Partial<Record>) => Result<Record>,
): Result<Record> {
  return record.phase === phase
    ? ok(record)
    : transition(record, phase, { message: "Mutation is durably pending reconciliation." });
}

async function compositeMetadata(
  port: PlanAuthoringPort,
  record: Record,
): Promise<Result<MetadataReceipt | null>> {
  const reconciliation = record.composite.composite.reconciliation;
  const reconciled = await port.reconcileMetadata(reconciliation);
  if (!reconciled.ok) return ok(null);
  let status: SubmissionStatus = reconciled.value;
  if (status.status === "not_committed") {
    const submitted = await port.recordCompositeSuccessor(record.composite);
    if (!submitted.ok)
      return submitted.error.code === "refused" ? fail(submitted.error.message) : ok(null);
    status = submitted.value.submission;
  }
  return status.status === "committed"
    ? ok(receipt(status, reconciliation.command_digest))
    : ok(null);
}

async function commandMetadata(
  port: PlanAuthoringPort,
  command: z.infer<typeof PreparedAssessmentProposalSchema>["command"],
): Promise<Result<MetadataReceipt | null>> {
  const request = {
    command_id: command.command.frame.header.command_id,
    command_digest: command.commandDigest,
  };
  const reconciled = await port.reconcileMetadata(request);
  if (!reconciled.ok) return ok(null);
  const status =
    reconciled.value.status === "not_committed" ? await port.submitMetadata(command) : reconciled;
  if (!status.ok) return status.error.code === "refused" ? fail(status.error.message) : ok(null);
  return status.value.status === "committed"
    ? ok(receipt(status.value, command.commandDigest))
    : ok(null);
}

function receipt(status: Extract<SubmissionStatus, { status: "committed" }>, digest: string) {
  return MetadataReceiptSchema.parse({
    commandId: status.receipt.command_id,
    commandDigest: digest,
    revision: status.receipt.revision,
  });
}

function readPrecursors(
  raw: unknown,
  expectedDigest?: string,
): Result<{
  input: MilestonePrecursorAuthoringInput;
  prepared: PreparedMilestonePrecursors;
} | null> {
  if (raw === undefined) return ok(null);
  const row = z
    .looseObject({
      input_digest: z.string().regex(/^[0-9a-f]{64}$/),
      input_blob: z.instanceof(Uint8Array),
      value_blob: z.instanceof(Uint8Array),
    })
    .safeParse(raw);
  if (!row.success) return fail("milestone precursor journal row is malformed");
  if (expectedDigest !== undefined && row.data.input_digest !== expectedDigest)
    return fail("operation identity is bound to different milestone precursor bytes");
  try {
    const input = MilestonePrecursorAuthoringInputSchema.safeParse(
      deserialize(row.data.input_blob),
    );
    const prepared = PreparedMilestonePrecursorsSchema.safeParse(deserialize(row.data.value_blob));
    return input.success && prepared.success
      ? ok({ input: input.data, prepared: prepared.data })
      : fail("milestone precursor journal value is malformed");
  } catch {
    return fail("milestone precursor journal value cannot be decoded");
  }
}

function hash(value: unknown): string {
  return createHash("sha256").update(encodeCanonicalJson(value)).digest("hex");
}

function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

function fail(message: string): Result<never> {
  return {
    ok: false,
    error: {
      code: "conflict",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: ${message}; fix surface: reconcile the same durable operation identity`,
    },
  };
}
