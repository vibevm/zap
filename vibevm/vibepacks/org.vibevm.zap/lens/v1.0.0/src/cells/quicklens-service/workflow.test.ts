/** @verifies spec://org.vibevm.zap/lens/PROP-002#plan-control */
import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { QuicklensRefSchema } from "../quicklens-model/index.ts";
import { createLivePlanWorkflow, AgentPlanProposalSchema } from "./workflow.ts";
import { openSqlitePlanWorkflowStore } from "./workflow-store.ts";
import { createHumanOwnerWorkflow } from "./owner-workflow.ts";

import {
  connection,
  fixtureBasis,
  fixtureProposal,
  intent,
  workflowZap,
} from "./workflow-fixture.test.ts";
const workflowScope = {
  workspaceId: "workspace.workflow",
  conversationId: "conversation.workflow",
};
test("agent proposal persists, previews exactly and advances to Owner-decision hold", async () => {
  const directory = await mkdtemp(join(tmpdir(), "quicklens-workflow-"));
  const path = join(directory, "workflow.sqlite");
  const opened = openSqlitePlanWorkflowStore(path, workflowScope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const zap = workflowZap();
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const basis = fixtureBasis();
  const waiting = await workflow.preview({
    intentRef: QuicklensRefSchema.parse("intent.workflow"),
    basis,
  });
  assert.equal(waiting.ok && waiting.value.state, "requested");

  const proposal = fixtureProposal(basis);
  const submitted = await workflow.submitProposal(connection(), intent(proposal), proposal);
  assert.equal(submitted.ok && submitted.value.state, "prepared");
  opened.value.close();

  const reopened = openSqlitePlanWorkflowStore(path, workflowScope);
  assert.equal(reopened.ok, true);
  if (!reopened.ok) return;
  const resumed = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: reopened.value,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const preview = await resumed.preview({ intentRef: proposal.intentRef, basis });
  assert.equal(preview.ok && preview.value.previewRef, proposal.previewRef);
  const applied = await resumed.apply({
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis,
  });
  assert.equal(applied.ok && applied.value.state, "held");
  assert.equal(zap.advances, 1);
  reopened.value.close();
});

test("proposal identities, selected recipient and saved basis are immutable", async () => {
  const opened = openSqlitePlanWorkflowStore(":memory:", workflowScope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  let current = true;
  const zap = workflowZap();
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    validateBasis: async (value) =>
      current
        ? { ok: true, value }
        : {
            ok: false,
            error: {
              code: "stale_basis",
              message: "fixture changed",
              recovery: "refresh",
            },
          },
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const proposal = fixtureProposal(fixtureBasis());
  const wrongRecipient = await workflow.submitProposal(
    connection(),
    intent(proposal, "actor.other"),
    proposal,
  );
  assert.equal(wrongRecipient.ok, false);
  const first = await workflow.submitProposal(connection(), intent(proposal), proposal);
  assert.equal(first.ok && first.value.state, "prepared");
  const changed = AgentPlanProposalSchema.parse({
    ...proposal,
    previewRef: "preview.changed",
  });
  const retry = await workflow.submitProposal(connection(), intent(proposal), changed);
  assert.equal(retry.ok, false);
  current = false;
  const stale = await workflow.preview({ intentRef: proposal.intentRef, basis: proposal.basis });
  assert.equal(!stale.ok && stale.error.code, "stale_basis");
  opened.value.close();
});

test("mutation is persisted before transport and repeated apply requires reconcile", async () => {
  const opened = openSqlitePlanWorkflowStore(":memory:", workflowScope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const zap = workflowZap("uncertain");
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const proposal = fixtureProposal(fixtureBasis());
  await workflow.submitProposal(connection(), intent(proposal), proposal);
  const apply = {
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis: proposal.basis,
  };
  const first = await workflow.apply(apply);
  const repeated = await workflow.apply(apply);
  assert.equal(first.ok && first.value.state, "uncertain");
  assert.equal(repeated.ok && repeated.value.state, "uncertain");
  assert.equal(zap.advances, 1);
  await workflow.reconcile({ operationRef: proposal.operationRef, basis: proposal.basis });
  assert.equal(zap.advances, 2);
  opened.value.close();
});

test("typed admission refusal is terminal rather than uncertain", async () => {
  const opened = openSqlitePlanWorkflowStore(":memory:", workflowScope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const zap = workflowZap("refusal");
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const proposal = fixtureProposal(fixtureBasis());
  await workflow.submitProposal(connection(), intent(proposal), proposal);
  const result = await workflow.apply({
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis: proposal.basis,
  });
  assert.equal(result.ok && result.value.state, "rejected");
  opened.value.close();
});

test("typed product refusal is terminal rather than reconciled", async () => {
  const opened = openSqlitePlanWorkflowStore(":memory:", workflowScope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const zap = workflowZap("product_refusal");
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    ...workflowScope,
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  const proposal = fixtureProposal(fixtureBasis());
  await workflow.submitProposal(connection(), intent(proposal), proposal);
  const result = await workflow.apply({
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis: proposal.basis,
  });
  assert.equal(result.ok && result.value.state, "rejected");
  opened.value.close();
});

test("authenticated Owner decision is durable and resumes the held admission", async () => {
  const opened = openSqlitePlanWorkflowStore(":memory:", workflowScope);
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const zap = workflowZap("owner");
  const owner = createHumanOwnerWorkflow({
    ownerZap: zap,
    readerZap: zap,
    store: opened.value,
    validateExecution: async (value) => ({ ok: true, value }),
    idFactory: (() => {
      let value = 0;
      return () => `fixture${String(++value)}`;
    })(),
  });
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: opened.value,
    workspaceId: "workspace.workflow",
    conversationId: "conversation.workflow",
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
    owner,
  });
  const proposal = fixtureProposal(fixtureBasis());
  await workflow.submitProposal(connection(), intent(proposal), proposal);
  await workflow.apply({
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis: proposal.basis,
  });
  const held = workflow.decision();
  assert.equal(held.ok && held.value?.holdRef, "hold:hold.workflow");
  const decided = await workflow.decide({
    operationRef: proposal.operationRef,
    holdRef: QuicklensRefSchema.parse("hold:hold.workflow"),
    basis: proposal.basis,
    choice: "approve",
    reason: "Approve the exact prepared successor plan.",
  });
  assert.equal(decided.ok && decided.value.state, "completed");
  assert.equal(zap.advances, 2);
  assert.equal(zap.submits, 2);
  opened.value.close();
});

test("held decisions stay scoped across workflow connections", async () => {
  const directory = await mkdtemp(join(tmpdir(), "quicklens-scoped-workflow-"));
  const path = join(directory, "workflow.sqlite");
  const first = openSqlitePlanWorkflowStore(path, workflowScope);
  const secondScope = {
    workspaceId: "workspace.other",
    conversationId: "conversation.other",
  };
  const second = openSqlitePlanWorkflowStore(path, secondScope);
  assert.ok(first.ok && second.ok);
  if (!first.ok || !second.ok) return;
  const proposal = fixtureProposal(fixtureBasis());
  const zap = workflowZap();
  const workflow = createLivePlanWorkflow({
    reader: zap,
    coordinator: zap,
    store: first.value,
    ...workflowScope,
    validateBasis: async (value) => ({ ok: true, value }),
    validateExecution: async (value) => ({ ok: true, value }),
    validateAdmissionRetry: async (value) => ({ ok: true, value }),
  });
  await workflow.submitProposal(connection(), intent(proposal), proposal);
  await workflow.apply({
    operationRef: proposal.operationRef,
    previewRef: proposal.previewRef,
    basis: proposal.basis,
  });
  const foreignDecision = second.value.currentDecision();
  assert.equal(foreignDecision.ok ? foreignDecision.value : undefined, null);
  first.value.close();
  second.value.close();
});
