/** Project-scoped shared planning wire. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { z } from "zod";
import {
  PlanBasisSchema,
  PlanDecisionInputSchema,
  PlanOperationResultSchema,
  QuicklensRefSchema,
} from "../quicklens-model/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "./ids.ts";

export const WorkspacePlanIntentInputSchema = z
  .object({
    text: z.string().min(1).max(16_384),
    basis: PlanBasisSchema,
    targetActorRef: QuicklensRefSchema,
  })
  .strict();
export const WorkspacePlanPreviewInputSchema = z
  .object({ intentRef: QuicklensRefSchema, basis: PlanBasisSchema })
  .strict();
export const WorkspacePlanApplyInputSchema = z
  .object({
    operationRef: QuicklensRefSchema,
    previewRef: QuicklensRefSchema,
    basis: PlanBasisSchema,
  })
  .strict();
export const WorkspacePlanReconcileInputSchema = z
  .object({ operationRef: QuicklensRefSchema, basis: PlanBasisSchema })
  .strict();

export { PlanDecisionInputSchema as WorkspacePlanDecisionInputSchema };
export { PlanOperationResultSchema as WorkspacePlanOperationResultSchema };
export type WorkspacePlanIntentInput = z.infer<typeof WorkspacePlanIntentInputSchema>;
export type WorkspacePlanPreviewInput = z.infer<typeof WorkspacePlanPreviewInputSchema>;
export type WorkspacePlanApplyInput = z.infer<typeof WorkspacePlanApplyInputSchema>;
export type WorkspacePlanReconcileInput = z.infer<typeof WorkspacePlanReconcileInputSchema>;

const ScopedPlanCommandSchema = z.object({
  clientRequestId: ClientRequestIdSchema,
  projectId: ProjectIdSchema,
  contextId: WorkContextIdSchema,
});
export const WorkspacePlanCommandRequestSchemas = [
  ScopedPlanCommandSchema.extend({
    operation: z.literal("plan.intent.v1"),
    input: WorkspacePlanIntentInputSchema,
  }).strict(),
  ScopedPlanCommandSchema.extend({
    operation: z.literal("plan.preview.v1"),
    input: WorkspacePlanPreviewInputSchema,
  }).strict(),
  ScopedPlanCommandSchema.extend({
    operation: z.literal("plan.apply.v1"),
    input: WorkspacePlanApplyInputSchema,
  }).strict(),
  ScopedPlanCommandSchema.extend({
    operation: z.literal("plan.reconcile.v1"),
    input: WorkspacePlanReconcileInputSchema,
  }).strict(),
  ScopedPlanCommandSchema.extend({
    operation: z.literal("plan.decide.v1"),
    input: PlanDecisionInputSchema,
  }).strict(),
] as const;

export const WorkspacePlanCommandResponseSchemas = [
  z.object({ operation: z.literal("plan.intent.v1"), result: PlanOperationResultSchema }).strict(),
  z.object({ operation: z.literal("plan.preview.v1"), result: PlanOperationResultSchema }).strict(),
  z.object({ operation: z.literal("plan.apply.v1"), result: PlanOperationResultSchema }).strict(),
  z
    .object({ operation: z.literal("plan.reconcile.v1"), result: PlanOperationResultSchema })
    .strict(),
  z.object({ operation: z.literal("plan.decide.v1"), result: PlanOperationResultSchema }).strict(),
] as const;
