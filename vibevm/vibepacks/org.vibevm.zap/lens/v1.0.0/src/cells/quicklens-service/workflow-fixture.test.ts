/** Shared generated fixtures for workflow tests. */
import assert from "node:assert/strict";
import { PlanBasisSchema, QuicklensRefSchema, type PlanBasis } from "../quicklens-model/index.ts";
import {
  MessageEnvelopeSchema,
  PublicConnectionSchema,
  type PublicConnection,
} from "../protocol/index.ts";
import {
  LosslessJsonSchema,
  U64WireSchema,
  ZapDigestSchema,
  ZapIdSchema,
  canonicalQueryInput,
  encodeCanonicalJson,
  parseWireJson,
  type AdvanceChangeAdmissionRequest,
  type ZapClient,
} from "../zap-client/index.ts";
import { AgentPlanProposalSchema, type AgentPlanProposal } from "./workflow.ts";
export function workflowZap(
  mode: "hold" | "uncertain" | "refusal" | "owner" | "product_refusal" | "multi" = "hold",
): ZapClient & { advances: number; submits: number } {
  const unavailable = async () => ({
    ok: false as const,
    error: { kind: "configuration" as const, message: "unused workflow fixture route" },
  });
  return {
    advances: 0,
    submits: 0,
    capabilities: unavailable,
    snapshot: unavailable,
    events: unavailable,
    streamEvents: unavailable,
    query: unavailable,
    activeContext: unavailable,
    prepareBundle: unavailable,
    async prepareComparison(_request) {
      const observed = u64(mode === "owner" && this.submits > 0 ? "9" : "7");
      return {
        ok: true,
        value: {
          store: fixtureStore(),
          observed_revision: observed,
          alternatives: [_preparedAlternative(_request)],
          basis_request: {},
          relevant_basis: ZapDigestSchema.parse("2".repeat(64)),
          affected_scopes: [
            {
              request_digest: ZapDigestSchema.parse("6".repeat(64)),
              observed_revision: observed,
              affected_work_ids: [],
              dependent_work_ids: [],
              subjects: [],
              unknown_boundary: [],
              completeness: "complete",
              relevant_basis: ZapDigestSchema.parse("2".repeat(64)),
              jobs: {
                request_digest: ZapDigestSchema.parse("7".repeat(64)),
                observed_revision: observed,
                jobs: [],
                completeness: "complete",
              },
              digest: ZapDigestSchema.parse("8".repeat(64)),
            },
          ],
        },
      };
    },
    prepareProjectedRecord: unavailable,
    prepareCompositeSuccessor: unavailable,
    recordCompositeSuccessor: unavailable,
    async submit(_channel, submission) {
      this.submits += 1;
      if (mode === "product_refusal") {
        return {
          ok: false,
          error: {
            kind: "http_refusal",
            status: 403,
            refusal: {
              code: "unauthorized",
              requirement: "spec://org.vibevm.zap/zap/PROP-001#authority",
              message: "refused",
              fix: "authority",
              detail: { kind: "none" },
            },
          },
        };
      }
      return {
        ok: true,
        value: {
          status: "committed",
          receipt: {
            store: fixtureStore(),
            command_id: submission.command.frame.header.command_id,
            event_id: submission.command.frame.header.event_id,
            transaction_id: ZapIdSchema.parse(`transaction.${this.submits}`),
            revision: u64(String(8 + this.submits)),
            event_digest: ZapDigestSchema.parse("9".repeat(64)),
            output: new Uint8Array(),
            disposition: "committed",
          },
        },
      };
    },
    reconcile: unavailable,
    async advanceChangeAdmission(request) {
      this.advances += 1;
      if (mode === "uncertain") {
        return {
          ok: false,
          error: {
            kind: "uncertain_operation",
            operation_id: request.operation_id,
            route: "/v1/change/admission",
            retry_exact: true,
          },
        };
      }
      if (mode === "refusal") {
        return {
          ok: false,
          error: {
            kind: "http_refusal",
            status: 409,
            refusal: {
              code: "stale_revision",
              requirement: "spec://org.vibevm.zap/zap/PROP-001#revision",
              message: "stale",
              fix: "store",
              detail: { kind: "none" },
            },
          },
        };
      }
      if (mode === "owner" && this.advances > 1) {
        return {
          ok: true,
          value: {
            status: "ready",
            operation_id: request.operation_id,
            assessment_id: request.assessment_id,
            alternative_id: request.alternative_id,
            observed_revision: u64("10"),
            adjudication: {},
            admission: {},
          },
        };
      }
      if (mode === "product_refusal" || mode === "multi") {
        return {
          ok: true,
          value: {
            status: "ready",
            operation_id: request.operation_id,
            assessment_id: request.assessment_id,
            alternative_id: request.alternative_id,
            observed_revision: u64("8"),
            adjudication: {},
            admission: {},
          },
        };
      }
      return {
        ok: true,
        value: {
          status: "owner_decision_required",
          operation_id: request.operation_id,
          assessment_id: request.assessment_id,
          alternative_id: request.alternative_id,
          observed_revision: u64("8"),
          hold_id: ZapIdSchema.parse("hold.workflow"),
          assessment_digest: request.assessment_digest,
          adjudication: {},
          decision: {
            assessment_digest: request.assessment_digest,
            forecast_id: null,
            forecast_digest: null,
            policy_id: ZapIdSchema.parse("policy.workflow"),
            policy_revision: u64("1"),
            recommended_alternative_id: request.alternative_id,
            effect_fingerprints: [ZapDigestSchema.parse("4".repeat(64))],
            effect_preflight_digests: [ZapDigestSchema.parse("5".repeat(64))],
            decision_revision: u64("1"),
          },
        },
      };
    },
  };
}

export function fixtureBasis(): PlanBasis {
  return PlanBasisSchema.parse({
    storeRef: "store:store.workflow",
    baseRef: "base:base.workflow",
    revision: "7",
    sourceBasisRef: `source-basis:${"d".repeat(64)}`,
  });
}

export function fixtureProposal(basis: PlanBasis): AgentPlanProposal {
  return AgentPlanProposalSchema.parse({
    intentRef: QuicklensRefSchema.parse("intent.workflow"),
    intentMessageId: "message.workflow",
    operationRef: QuicklensRefSchema.parse("operation.workflow"),
    previewRef: QuicklensRefSchema.parse("preview.workflow"),
    intentBasis: basis,
    basis,
    admission: fixtureAdmission(),
  });
}

export function connection(): PublicConnection {
  return PublicConnectionSchema.parse({
    actor: {
      principalId: "principal.workflow",
      actorId: "actor.workflow",
      workspaceId: "workspace.workflow",
      conversationId: "conversation.workflow",
      parentActorId: null,
      state: "active",
      capabilities: ["plan:propose"],
      hostKind: "test",
      hostProvenance: "attested",
    },
    handle: {
      actorId: "actor.workflow",
      bindingId: "binding.workflow",
      workspaceId: "workspace.workflow",
      conversationId: "conversation.workflow",
      generation: "1",
    },
  });
}

export function intent(proposal: AgentPlanProposal, toActorId = "actor.workflow") {
  return MessageEnvelopeSchema.parse({
    protocol: "lens/1",
    messageId: proposal.intentMessageId,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    fromActorId: null,
    toActorId,
    kind: "notice.created",
    correlationId: null,
    causationId: null,
    sequence: "1",
    payload: {
      type: "plan_intent",
      intentRef: proposal.intentRef,
      text: "Update the milestone plan",
      basis: proposal.basis,
    },
    createdAt: "2026-09-15T00:00:00.000Z",
  });
}

function fixtureAdmission(): AdvanceChangeAdmissionRequest {
  const alternative = ZapIdSchema.parse("alternative.workflow");
  const payload = canonicalQueryInput({ schema: "milestone-plan-adopted/1" });
  return {
    operation_id: ZapIdSchema.parse("operation.workflow"),
    store: fixtureStore(),
    expected_revision: 7n,
    action: ZapIdSchema.parse("plan.lower"),
    assessment_id: ZapIdSchema.parse("assessment.workflow"),
    alternative_id: alternative,
    source_assessment_digest: ZapDigestSchema.parse("1".repeat(64)),
    assessment_digest: ZapDigestSchema.parse("1".repeat(64)),
    relevant_basis: ZapDigestSchema.parse("2".repeat(64)),
    comparison: {
      at: { kind: "current" },
      actor: null,
      draft: {
        assessment_id: ZapIdSchema.parse("assessment.workflow"),
        alternatives: [
          {
            alternative_id: alternative,
            committed_prefix: [],
            effects: [
              {
                effect_id: ZapIdSchema.parse("effect.workflow"),
                index: 0,
                kind: ZapIdSchema.parse("milestone.plan-adopted"),
                payload,
                predecessors: [],
                product_event_id: ZapIdSchema.parse("event.workflow"),
              },
            ],
            no_op_basis: null,
          },
        ],
        policy: "required",
        capacity: "not_applicable",
        closure: "known_graph",
      },
    },
    product: {
      frame: {
        header: {
          protocol: 1,
          store_id: ZapIdSchema.parse("store.workflow"),
          campaign_id: ZapIdSchema.parse("campaign.workflow"),
          base_id: ZapIdSchema.parse("base.workflow"),
          command_id: ZapIdSchema.parse("command.workflow"),
          event_id: ZapIdSchema.parse("event.workflow"),
          expected_revision: 8n,
          kind: ZapIdSchema.parse("milestone.plan-adopted"),
          causes: [],
          basis: { kind: "exact", digest: ZapDigestSchema.parse("2".repeat(64)) },
        },
        reason: { summary: "Apply admitted plan", evidence: [], decision: null, change: null },
        payload,
      },
    },
    decision_id: null,
    exception_id: null,
  };
}

function fixtureStore() {
  return {
    store_id: ZapIdSchema.parse("store.workflow"),
    campaign_id: ZapIdSchema.parse("campaign.workflow"),
    base_id: ZapIdSchema.parse("base.workflow"),
    store_epoch: "zap/2",
    codec_epoch: 2,
    reducer_epoch: 1,
  };
}

export function u64(value: string) {
  return U64WireSchema.parse(parseWireJson(new TextEncoder().encode(value)));
}

function _preparedAlternative(request: AdvanceChangeAdmissionRequest["comparison"]) {
  const selected = request.draft.alternatives[0];
  assert.ok(selected);
  return {
    store: fixtureStore(),
    observed_revision: u64("7"),
    request: LosslessJsonSchema.parse(
      parseWireJson(
        encodeCanonicalJson({
          alternative_id: selected.alternative_id,
          committed_prefix: selected.committed_prefix,
          initial_basis: ZapDigestSchema.parse("2".repeat(64)),
          effects: selected.effects.map((effect) => ({
            ...effect,
            basis: { kind: "exact", digest: ZapDigestSchema.parse("2".repeat(64)) },
            declared_subjects: [{ kind: "outcome", id: "outcome.workflow" }],
            relevant_before: ZapDigestSchema.parse("2".repeat(64)),
            declared_relevant_after: ZapDigestSchema.parse("2".repeat(64)),
          })),
          no_op_basis: null,
          request_digest: ZapDigestSchema.parse("6".repeat(64)),
        }),
      ),
    ),
    preflight: {},
    affected_scopes: selected.effects.map(() => null),
  };
}
