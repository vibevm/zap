/** Browser-safe projections. @scope spec://org.vibevm.zap/lens/PROP-002#shared-client */
import { randomUUID } from "node:crypto";
import {
  QuicklensRefSchema,
  type AgentTargetView,
  type PlanBasis,
  type PlanStatus,
} from "../quicklens-model/index.ts";
import {
  ClientRequestIdSchema,
  type ActorId,
  type ActorList,
  type ConversationId,
  type PrincipalEmitInput,
  type WorkspaceId,
} from "../protocol/index.ts";
import type { ActiveContextView } from "../zap-client/index.ts";
import type { PlanWorkflowPort } from "./types.ts";
import type { MilestonePlanView } from "./wire.ts";

export function planStatus(
  context: ActiveContextView,
  milestone: MilestonePlanView | null,
  basis: PlanBasis,
  actors: ActorList,
  workflow: PlanWorkflowPort,
  held: ReturnType<PlanWorkflowPort["decision"]>,
): PlanStatus | null {
  if (context.active_outcome.state === "uninitialized") return null;
  const reassessment =
    context.adopted_milestone_plan.state === "needs_reassessment" ||
    milestone?.status === "adopted_needs_reassessment";
  const eligible = actors.actors.some((row) => row.eligiblePlanTarget);
  const unavailable = workflow.unavailableReason ?? "Plan workflow is not configured.";
  const decision = held.ok ? held.value : null;
  return {
    outcomeLabel: context.active_outcome.outcome_id,
    strategyLabel:
      context.current_strategy.state === "present"
        ? context.current_strategy.strategic_revision_id
        : "No current strategy",
    planLabel:
      context.adopted_milestone_plan.state === "absent"
        ? "No adopted milestone plan"
        : `${context.adopted_milestone_plan.plan_key.outcome_id} generation ${context.adopted_milestone_plan.plan_key.generation}`,
    basis,
    state: decision !== null ? "held" : reassessment ? "reassessment" : "current",
    detail:
      decision !== null
        ? "A held plan operation needs an authenticated human Owner decision."
        : reassessment
          ? "The adopted plan needs reassessment against current records."
          : null,
    decision,
    actions: {
      propose: { enabled: eligible, reason: eligible ? null : "No eligible root planning agent." },
      preview: { enabled: workflow.available, reason: workflow.available ? null : unavailable },
      apply: { enabled: workflow.available, reason: workflow.available ? null : unavailable },
      reconcile: { enabled: workflow.available, reason: workflow.available ? null : unavailable },
      decide: {
        enabled: decision !== null,
        reason:
          decision !== null
            ? null
            : held.ok
              ? "No held owner decision is available."
              : "Held decision state is unavailable.",
      },
    },
  };
}

export function agentTarget(row: ActorList["actors"][number]): AgentTargetView {
  return {
    ref: actorRef(row.actor.actorId),
    label: row.label,
    host: row.actor.hostKind,
    parentRef: row.actor.parentActorId === null ? null : actorRef(row.actor.parentActorId),
    state: row.actor.state === "active" ? "active" : "offline",
    eligible: row.eligiblePlanTarget,
    reason: row.eligiblePlanTarget ? null : "Actor is not an active root with plan:propose.",
  };
}

export function principalNotice(
  actorId: ActorId,
  text: string,
  basis: PlanBasis,
  intentRef: ReturnType<typeof QuicklensRefSchema.parse>,
  workspaceId: WorkspaceId,
  conversationId: ConversationId,
  clientRequestId = requestId("plan-intent"),
): PrincipalEmitInput {
  return {
    clientRequestId,
    workspaceId,
    conversationId,
    toActorId: actorId,
    payload: { type: "plan_intent", intentRef, text, basis },
  };
}

export function requestId(prefix: string) {
  return ClientRequestIdSchema.parse(`${prefix}.${randomUUID().replaceAll("-", "")}`);
}

export function actorRef(actorId: string) {
  return QuicklensRefSchema.parse(`actor:${actorId}`);
}
