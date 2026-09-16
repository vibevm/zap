/** Authenticated MCP plan adapter. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { createHash } from "node:crypto";
import { z } from "zod";
import {
  PlanBasisSchema,
  QuicklensRefSchema,
  type PlanOperationResult,
  type QuicklensResult,
} from "../quicklens-model/index.ts";
import {
  ClientRequestIdSchema,
  JsonValueSchema,
  type PublicConnection,
  type Result,
} from "../protocol/index.ts";
import type { AgentPlanProposalPort } from "../mcp/index.ts";
import { AdapterSessionIdSchema, type AgentTransportPort } from "../transport/index.ts";
import {
  MilestonePrecursorAuthoringInputSchema,
  SuccessorPlanAuthoringInputSchema,
  type PlanAuthoringPort,
  type PreparedSuccessor,
} from "../plan-authoring/index.ts";
import {
  PrepareBundleInputSchema,
  PrepareComparisonInputSchema,
  PrepareProjectedInputSchema,
} from "../zap-client/index.ts";
import { AgentPlanProposalSchema, type LivePlanWorkflow } from "./workflow.ts";
import type { DurablePlanAuthoring } from "./authoring-journal.ts";
import {
  CompositeSuccessorAuthoringInputSchema,
  type DurableCompositePlanAuthoring,
} from "./composite-journal.ts";

const IntentInputSchema = z
  .object({ text: z.string().min(1).max(16_384), basis: PlanBasisSchema })
  .strict();
const PreviewInputSchema = z
  .object({ intentRef: QuicklensRefSchema, basis: PlanBasisSchema })
  .strict();
const ApplyInputSchema = z
  .object({
    operationRef: QuicklensRefSchema,
    previewRef: QuicklensRefSchema,
    basis: PlanBasisSchema,
  })
  .strict();
const ReconcileInputSchema = z
  .object({ operationRef: QuicklensRefSchema, basis: PlanBasisSchema })
  .strict();
const AuthoredProposalSchema = z
  .object({
    intentRef: QuicklensRefSchema,
    intentMessageId: z.string().min(3).max(160),
    previewRef: QuicklensRefSchema,
    preparedOperationId: z.string().min(3).max(160),
  })
  .strict();

export function createAgentPlanProposalPort(
  workflow: LivePlanWorkflow,
  agent: AgentTransportPort,
  authoring: PlanAuthoringPort,
  journal: DurablePlanAuthoring,
  compositeJournal: DurableCompositePlanAuthoring,
): AgentPlanProposalPort {
  return {
    register: async (connection, session, raw) => {
      const input = IntentInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "plan intent input is invalid");
      const authorized = activePlanner(connection);
      if (!authorized.ok) return authorized;
      const identity = createHash("sha256")
        .update(connection.actor.actorId)
        .update(JSON.stringify(input.data))
        .digest("hex");
      const intentRef = QuicklensRefSchema.parse(`intent:${identity}`);
      const emitted = await agent.emit(AdapterSessionIdSchema.parse(session), {
        clientRequestId: ClientRequestIdSchema.parse(`plan-intent.${identity}`),
        toActorId: connection.actor.actorId,
        payload: { type: "plan_intent", intentRef, text: input.data.text, basis: input.data.basis },
      });
      return emitted.ok
        ? json({ intentRef, intentMessageId: emitted.value.messageId, basis: input.data.basis })
        : emitted;
    },
    submit: async (connection, session, raw) => {
      const authored = AuthoredProposalSchema.safeParse(raw);
      if (!authored.success) {
        return failure("invalid_input", "plan proposal requires a journaled operation identity");
      }
      const ordinary = journal.prepared(authored.data.preparedOperationId);
      const stored = ordinary.ok
        ? ordinary
        : compositeJournal.prepared(authored.data.preparedOperationId);
      const proposal = stored.ok
        ? await orderedProposal(authoring, authored.data, stored.value)
        : { ok: false as const, reason: stored.error.message };
      if (!proposal.ok) return failure("invalid_input", proposal.reason);
      const intent = await agent.planIntent(session, proposal.value.intentMessageId);
      if (!intent.ok) return intent;
      return result(await workflow.submitProposal(connection, intent.value, proposal.value));
    },
    preview: async (connection, _session, raw) =>
      authorized(connection, PreviewInputSchema, raw, (input) => workflow.preview(input)),
    apply: async (connection, _session, raw) =>
      authorized(connection, ApplyInputSchema, raw, (input) => workflow.apply(input)),
    reconcile: async (connection, _session, raw) =>
      authorized(connection, ReconcileInputSchema, raw, (input) => workflow.reconcile(input)),
    prepare: async (connection, _session, kind, raw) => {
      const allowed = activePlanner(connection);
      if (!allowed.ok) return allowed;
      const input = preparationInput(raw);
      if (!input.ok) return input;
      const prepared = await prepare(authoring, kind, input.value);
      return prepared.ok
        ? json(jsonSafe(prepared.value))
        : failure("conflict", `ZAP ${kind} preparation failed: ${prepared.error.kind}`);
    },
    discover: async (connection) => {
      const allowed = activePlanner(connection);
      if (!allowed.ok) return allowed;
      const discovered = await authoring.discover();
      return discovered.ok
        ? json(jsonSafe(discovered.value))
        : failure("conflict", discovered.error.message);
    },
    author: async (connection, _session, raw) => {
      const allowed = activePlanner(connection);
      if (!allowed.ok) return allowed;
      const input = SuccessorPlanAuthoringInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "successor authoring input is invalid");
      const authored = await journal.author(input.data);
      return authored.ok
        ? json({
            operationId: authored.value.operationId,
            intentBasis: authored.value.intentBasis,
            preparedBasis: authored.value.preparedBasis,
            effectCount: authored.value.effects.length,
          })
        : failure("conflict", authored.error.message);
    },
    authorComposite: async (connection, _session, raw) => {
      const allowed = activePlanner(connection);
      if (!allowed.ok) return allowed;
      const input = CompositeSuccessorAuthoringInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "composite authoring input is invalid");
      const authored = await compositeJournal.authorComposite(input.data);
      return authored.ok
        ? json({
            operationId: authored.value.operationId,
            intentBasis: authored.value.intentBasis,
            preparedBasis: authored.value.preparedBasis,
            effectCount: authored.value.effects.length,
          })
        : failure("conflict", authored.error.message);
    },
    prepareComposite: async (connection, _session, raw) => {
      const allowed = activePlanner(connection);
      if (!allowed.ok) return allowed;
      const input = MilestonePrecursorAuthoringInputSchema.safeParse(raw);
      if (!input.success) return failure("invalid_input", "milestone precursor input is invalid");
      const prepared = await compositeJournal.prepareComposite(input.data);
      return prepared.ok
        ? json(jsonSafe(prepared.value))
        : failure("conflict", prepared.error.message);
    },
  };
}

async function orderedProposal(
  authoring: PlanAuthoringPort,
  input: z.infer<typeof AuthoredProposalSchema>,
  prepared: PreparedSuccessor,
) {
  const step = await authoring.prepareEffect(prepared, {
    effectIndex: 0,
    completedPrefix: [],
    decisionId: null,
  });
  if (!step.ok) return { ok: false as const, reason: step.error.message };
  const proposal = AgentPlanProposalSchema.safeParse({
    intentRef: input.intentRef,
    intentMessageId: input.intentMessageId,
    operationRef: QuicklensRefSchema.parse(`operation:${prepared.operationId}`),
    previewRef: input.previewRef,
    intentBasis: prepared.intentBasis,
    basis: step.value.executionBasis,
    admission: step.value.advanceRequest,
    preparedSuccessor: prepared,
    preparedStep: step.value,
  });
  return proposal.success
    ? { ok: true as const, value: proposal.data }
    : { ok: false as const, reason: "trusted prepared proposal failed runtime validation" };
}

function preparationInput(raw: unknown): Result<Record<string, unknown>> {
  const input = z.record(z.string(), z.unknown()).safeParse(raw);
  if (!input.success) return failure("invalid_input", "plan preparation input is invalid");
  const at = z.record(z.string(), z.unknown()).safeParse(input.data["at"]);
  if (!at.success) return { ok: true, value: input.data };
  const revision = at.data["revision"];
  return {
    ok: true,
    value: {
      ...input.data,
      at: {
        ...at.data,
        ...(typeof revision === "string" && /^(0|[1-9][0-9]*)$/.test(revision)
          ? { revision: BigInt(revision) }
          : {}),
      },
    },
  };
}

async function prepare(
  authoring: PlanAuthoringPort,
  kind: "bundle" | "comparison" | "projected_record",
  raw: unknown,
) {
  if (kind === "bundle") {
    const input = PrepareBundleInputSchema.safeParse(raw);
    return input.success
      ? authoring.prepareBundle(input.data)
      : Promise.resolve({
          ok: false as const,
          error: { kind: "configuration" as const, message: "bundle input is invalid" },
        });
  }
  if (kind === "comparison") {
    const input = PrepareComparisonInputSchema.safeParse(raw);
    return input.success
      ? authoring.prepareComparison(input.data)
      : Promise.resolve({
          ok: false as const,
          error: { kind: "configuration" as const, message: "comparison input is invalid" },
        });
  }
  const input = PrepareProjectedInputSchema.safeParse(raw);
  return input.success
    ? authoring.prepareProjectedRecord(input.data)
    : Promise.resolve({
        ok: false as const,
        error: { kind: "configuration" as const, message: "projected input is invalid" },
      });
}

function jsonSafe(value: unknown): unknown {
  if (typeof value === "bigint") return value.toString();
  if (value instanceof Uint8Array) return [...value];
  if (Array.isArray(value)) return value.map(jsonSafe);
  if (typeof value === "object" && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, jsonSafe(item)]));
  }
  return value;
}

function activePlanner(connection: PublicConnection): Result<null> {
  const actor = connection.actor;
  return actor.state === "active" &&
    actor.parentActorId === null &&
    actor.capabilities.includes("plan:propose")
    ? { ok: true, value: null }
    : failure("forbidden", "only an active root plan:propose actor may use plan tools");
}

async function authorized<T>(
  connection: PublicConnection,
  schema: z.ZodType<T>,
  raw: unknown,
  call: (input: T) => Promise<QuicklensResult<PlanOperationResult>>,
): Promise<Result<z.infer<typeof JsonValueSchema>>> {
  const allowed = activePlanner(connection);
  if (!allowed.ok) return allowed;
  const input = schema.safeParse(raw);
  return input.success
    ? result(await call(input.data))
    : failure("invalid_input", "plan workflow input is invalid");
}

function result(value: QuicklensResult<PlanOperationResult>) {
  return value.ok
    ? json(value.value)
    : failure(value.error.code === "forbidden" ? "forbidden" : "conflict", value.error.message);
}

function json(value: unknown): Result<z.infer<typeof JsonValueSchema>> {
  return { ok: true, value: JsonValueSchema.parse(value) };
}

function failure(code: "invalid_input" | "forbidden" | "conflict", message: string): Result<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-002#plan-control: ${message}; fix surface: use the authenticated scoped plan intent and exact saved identities`,
    },
  };
}
