/** ZAP admission and refusal schemas. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { z } from "zod";
import { normalizeLossless, wireU32, wireU64 } from "./codec.ts";
import { ZapDigestSchema, ZapIdSchema } from "./schemas-base.ts";

const U64WireSchema = transformUnknown(wireU64);
const U32WireSchema = transformUnknown(wireU32);
const LosslessJsonSchema = transformUnknown(normalizeLossless);
const IdentifierListSchema = z.array(ZapIdSchema);
const SubjectKinds = [
  "campaign",
  "intent",
  "outcome",
  "obligation",
  "work",
  "contract",
  "source",
  "evidence",
  "decision",
  "review",
  "deferral",
  "lowering",
  "dream",
  "job",
  "verification",
  "hold",
  "pause",
  "effect",
  "resource",
] as const;
const SubjectRefSchema = z.object({ kind: z.enum(SubjectKinds), id: ZapIdSchema }).strict();
const ErrorCodes = [
  "invalid_identity",
  "invalid_value",
  "invalid_fields",
  "unsupported_epoch",
  "unsupported_operation",
  "duplicate_identity",
  "missing_reference",
  "cycle",
  "stale_revision",
  "stale_basis",
  "idempotency_conflict",
  "unauthorized",
  "paused",
  "held",
  "needs_evidence",
  "conflict",
  "busy",
  "pending_effect",
  "unknown_effect",
  "corrupt_store",
  "legacy_incompatible",
  "limit_exceeded",
  "unavailable",
  "internal_invariant",
] as const;
const FixSurfaces = [
  "command",
  "payload",
  "source_capture",
  "authority",
  "policy",
  "store",
  "adapter",
  "configuration",
  "migration",
  "retry_after_reconcile",
] as const;
const ErrorDetailSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("none") }).strict(),
  z
    .object({
      kind: z.literal("invalid_identity"),
      identity_type: z.string(),
      byte_len: U64WireSchema,
    })
    .strict(),
  z
    .object({ kind: z.literal("stale_revision"), expected: U64WireSchema, actual: U64WireSchema })
    .strict(),
  z
    .object({ kind: z.literal("conflicting_ids"), command_id: ZapIdSchema, event_id: ZapIdSchema })
    .strict(),
  z.object({ kind: z.literal("missing_subjects"), subjects: z.array(SubjectRefSchema) }).strict(),
  z
    .object({
      kind: z.literal("unsupported_epoch"),
      family: z.string(),
      requested: U32WireSchema,
      supported: z.array(U32WireSchema),
    })
    .strict(),
  z
    .object({
      kind: z.literal("violated_limit"),
      name: z.string(),
      maximum: U64WireSchema,
      actual: U64WireSchema,
    })
    .strict(),
  z.object({ kind: z.literal("active_holds"), holds: IdentifierListSchema }).strict(),
  z.object({ kind: z.literal("active_pauses"), pauses: IdentifierListSchema }).strict(),
  z.object({ kind: z.literal("pending_effects"), jobs: IdentifierListSchema }).strict(),
]);
export const RefusalSchema = z
  .object({
    code: z.enum(ErrorCodes),
    requirement: z.string().startsWith("spec://").includes("#").max(2048),
    message: utf8String(1, 4096),
    fix: z.enum(FixSurfaces),
    detail: ErrorDetailSchema,
  })
  .strict();
export const ResyncSchema = z
  .object({ kind: z.literal("resync_required"), reason: z.string().min(1) })
  .strict();
export const ChangeAdmissionViewSchema = z.discriminatedUnion("status", [
  z
    .object({
      status: z.literal("ready"),
      operation_id: ZapIdSchema,
      assessment_id: ZapIdSchema,
      alternative_id: ZapIdSchema,
      observed_revision: U64WireSchema,
      adjudication: LosslessJsonSchema,
      admission: LosslessJsonSchema,
    })
    .strict(),
  z
    .object({
      status: z.literal("owner_decision_required"),
      operation_id: ZapIdSchema,
      assessment_id: ZapIdSchema,
      alternative_id: ZapIdSchema,
      observed_revision: U64WireSchema,
      hold_id: ZapIdSchema,
      assessment_digest: ZapDigestSchema,
      adjudication: LosslessJsonSchema,
      decision: z
        .object({
          assessment_digest: ZapDigestSchema,
          forecast_id: ZapIdSchema.nullable(),
          forecast_digest: ZapDigestSchema.nullable(),
          policy_id: ZapIdSchema,
          policy_revision: U64WireSchema,
          recommended_alternative_id: ZapIdSchema,
          effect_fingerprints: z.array(ZapDigestSchema),
          effect_preflight_digests: z.array(ZapDigestSchema),
          decision_revision: U64WireSchema,
        })
        .strict(),
    })
    .strict(),
]);
function transformUnknown<T>(transform: (value: unknown) => T) {
  return z.unknown().transform((value, context): T => {
    try {
      return transform(value);
    } catch {
      context.addIssue({ code: "custom", message: "invalid exact ZAP wire value" });
      return z.NEVER;
    }
  });
}
function utf8String(minimum: number, maximum: number) {
  return z.string().refine((value) => {
    const size = new TextEncoder().encode(value).length;
    return size >= minimum && size <= maximum;
  });
}
