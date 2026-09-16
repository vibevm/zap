/** Canonical public-command helpers. @scope spec://org.vibevm.zap/lens/PLAN-AUTHORING-GUIDE#reconciliation */
import { createHash } from "node:crypto";
import { z } from "zod";
import {
  U32WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
  canonicalQueryInput,
  encodeCanonicalJson,
  type CanonicalJsonInput,
  type PreparedEffectComparisonView,
  type ProtectedCommand,
  type ZapDigest,
  type ZapId,
  type ZapStoreIdentity,
} from "../zap-client/index.ts";
import type { PlanAuthoringResult } from "./types.ts";

const ByteSchema = z.union([z.number(), z.string()]).transform((value, context) => {
  const number = typeof value === "number" ? value : Number(value);
  if (!Number.isInteger(number) || number < 0 || number > 255) {
    context.addIssue({ code: "custom", message: "invalid canonical byte" });
    return z.NEVER;
  }
  return number;
});
const StoredU32Schema = z
  .union([U32WireSchema, z.string().regex(/^(0|[1-9][0-9]*)$/)])
  .transform((value) => Number(value));
const SubjectSchema = z.object({ kind: z.string().min(1), id: ZapIdSchema }).strict();
const BasisSchema = z
  .object({
    purpose: z.object({ kind: z.string().min(1), subject: ZapIdSchema }).strict(),
    roots: z.array(SubjectSchema),
    policy: z.enum(["required", "not_applicable"]),
    capacity: z.enum(["required", "not_applicable"]),
    closure: z.enum(["known_graph", "allow_unknown"]),
  })
  .strict();
const PreparedEffectSchema = z
  .object({
    effect_id: ZapIdSchema,
    index: StoredU32Schema,
    kind: ZapIdSchema,
    payload: z
      .object({ codec: StoredU32Schema.pipe(z.literal(2)), canonical_json: z.array(ByteSchema) })
      .strict(),
    predecessors: z.array(ZapIdSchema),
    product_event_id: ZapIdSchema,
    basis: BasisSchema,
    declared_subjects: z.array(SubjectSchema),
    relevant_before: ZapDigestSchema,
    declared_relevant_after: ZapDigestSchema,
  })
  .strict();
const PreparedRequestSchema = z
  .object({
    alternative_id: ZapIdSchema,
    committed_prefix: z.array(ZapIdSchema),
    initial_basis: ZapDigestSchema,
    effects: z.array(PreparedEffectSchema).min(1),
    no_op_basis: BasisSchema.nullable(),
    request_digest: ZapDigestSchema,
  })
  .strict();
const NoOpPreflightSchema = z.looseObject({ initial_basis: ZapDigestSchema });
const CanonicalSchema = z.custom<CanonicalJsonInput>((value) => {
  try {
    encodeCanonicalJson(value);
    return true;
  } catch {
    return false;
  }
});

export interface CommandIdentity {
  readonly commandId: ZapId;
  readonly eventId: ZapId;
}

export function command(
  store: ZapStoreIdentity,
  identity: CommandIdentity,
  expectedRevision: bigint,
  kind: ZapId,
  payload: CanonicalJsonInput,
  basis:
    | { readonly kind: "not_applicable" }
    | { readonly kind: "exact"; readonly digest: ZapDigest },
  summary: string,
  change: ZapId | null,
): ProtectedCommand {
  return {
    frame: {
      header: {
        protocol: 1,
        store_id: store.store_id,
        campaign_id: store.campaign_id,
        base_id: store.base_id,
        command_id: identity.commandId,
        event_id: identity.eventId,
        expected_revision: expectedRevision,
        kind,
        causes: [],
        basis,
      },
      reason: { summary, evidence: [], decision: null, change },
      payload: canonicalQueryInput(payload),
    },
  };
}

export function preparedRequest(value: PreparedEffectComparisonView) {
  const alternative = value.alternatives[0];
  return alternative === undefined
    ? fail("invalid_input", "Prepared comparison has no alternative")
    : parse(PreparedRequestSchema, alternative.request, "Prepared effect request is malformed");
}

export function noOpBasis(value: unknown) {
  return parse(NoOpPreflightSchema, value, "No-op preparation basis is malformed");
}

export function comparisonBasis(value: unknown) {
  return parse(BasisSchema, value, "Comparison basis request is malformed");
}

export function canonicalValue(value: unknown) {
  return parse(CanonicalSchema, value, "Canonical JSON value is malformed");
}

export function canonicalDigest(value: CanonicalJsonInput): ZapDigest {
  return ZapDigestSchema.parse(
    createHash("sha256").update(encodeCanonicalJson(value)).digest("hex"),
  );
}

export function canonicalBytesDigest(bytes: Uint8Array): ZapDigest {
  return ZapDigestSchema.parse(createHash("sha256").update(bytes).digest("hex"));
}

export function derivedId(operationId: ZapId, purpose: string): ZapId {
  const digest = createHash("sha256")
    .update(encodeCanonicalJson([operationId, purpose]))
    .digest("hex");
  return ZapIdSchema.parse(`plan-authoring.${purpose}.${digest}`);
}

export function parse<T>(
  schema: z.ZodType<T>,
  value: unknown,
  message: string,
): PlanAuthoringResult<T> {
  const parsed = schema.safeParse(value);
  return parsed.success ? { ok: true, value: parsed.data } : fail("invalid_input", message);
}

export function fail(
  code:
    | "invalid_input"
    | "unavailable"
    | "stale_basis"
    | "economics_context_unavailable"
    | "ambiguous_baseline"
    | "refused"
    | "uncertain",
  message: string,
): PlanAuthoringResult<never> {
  return {
    ok: false,
    error: {
      code,
      message,
      recovery: "Refresh public ZAP context and specification basis before retrying.",
    },
  };
}
