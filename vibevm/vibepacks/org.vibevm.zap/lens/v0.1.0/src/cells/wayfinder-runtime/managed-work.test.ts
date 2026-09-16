import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import { z } from "zod";
import {
  ActorIdSchema,
  ClientRequestIdSchema,
  DecimalSchema,
  PrincipalIdSchema,
} from "../protocol/index.ts";
import { openWayfinderAgentFoundation } from "./agent.ts";
import {
  createManagedAgentBackend,
  createManagedProviderDrivers,
  ManagedAgentProfileSchema,
  openManagedWorkStore,
} from "../managed-work/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import { createManagedWorkHttpClient, createNativeWorkHttpClient } from "../http/index.ts";
import { createWayfinderRuntime } from "./index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import {
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
  QuestionGroupIdSchema,
  QuestionItemIdSchema,
  ClientIdSchema,
  AttemptIdSchema,
  ManagedWorkViewSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import {
  attachments,
  createRequest,
  enroll,
  fakeHost,
  foundationAgent,
  managedAnnotationFixture,
  managedCredential,
  managedProfile,
  parentPort,
  projectRegistration,
  runtimeConfig,
  scriptedTerminalService,
  selectionPort,
} from "./managed-work.fixture.ts";

test("Wayfinder runs real managed backend over authenticated workspace and broker paths", async () => {
  const root = mkdtempSync(join(tmpdir(), "zap-managed-runtime-"));
  const brokerPath = join(root, "broker.sqlite");
  const workPath = join(root, "managed-work.sqlite");
  const workspacePath = join(root, "workspace.sqlite");
  const agentToken = enroll(brokerPath);
  const workspace = openWorkspaceStore({ databasePath: workspacePath });
  assert.equal(workspace.ok, true);
  if (!workspace.ok) return;
  const registered = workspace.value.registerProject(projectRegistration());
  assert.equal(registered.ok, true);
  const foundation = openWayfinderAgentFoundation(
    {
      databasePath: brokerPath,
      host: "127.0.0.1",
      port: 0,
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [],
      statusToken: "status.managed.runtime.synthetic.0001",
      scopes: [
        {
          workspaceId: "workspace.managed",
          conversationId: "conversation.managed",
          humanPrincipalToken: "human.managed.runtime.synthetic.0001",
          agentPrincipalToken: agentToken,
        },
      ],
    },
    workspace.value,
  );
  assert.equal(foundation.ok, true);
  if (!foundation.ok) return;
  const agentStarted = await foundation.value.start();
  assert.equal(agentStarted.ok, true);
  if (!agentStarted.ok) return;
  const terminalRuntime = scriptedTerminalService();
  const terminal = terminalRuntime.service;
  const workStore = openManagedWorkStore(workPath);
  assert.equal(workStore.ok, true);
  if (!workStore.ok) return;
  const annotations = managedAnnotationFixture(root);
  const profile = managedProfile(root);
  const ids = new Map<string, number>();
  const attachmentAcks: string[] = [];
  const backend = createManagedAgentBackend({
    store: workStore.value,
    terminals: terminal,
    profiles: [],
    drivers: createManagedProviderDrivers(),
    environment: { resolve: async () => ({ ok: true as const, value: {} }) },
    bindings: foundation.value.managedActors,
    selections: selectionPort(),
    parents: parentPort(),
    attachments: annotations.attachments,
    execution: {
      canStart: () => ({ ok: true, value: null }),
    },
    id: (kind) => {
      const next = (ids.get(kind) ?? 0) + 1;
      ids.set(kind, next);
      return `${kind}.managed.synthetic.${String(next)}`;
    },
  });
  assert.equal(backend.registerProfile(profile).ok, true);
  assert.equal(
    backend.registerProfile(ManagedAgentProfileSchema.parse({ ...profile, modelId: "drift" })).ok,
    false,
  );
  assert.equal(foundation.value.bindManagedWork(backend).ok, true);
  assert.equal(foundation.value.bindNativeWork(attachments(attachmentAcks)).ok, true);
  const runtime = createWayfinderRuntime(runtimeConfig(root), {
    store: workspace.value,
    managedWork: backend,
    hosts: [fakeHost()],
  });
  assert.equal(runtime.ok, true);
  if (!runtime.ok) return;
  try {
    const started = await runtime.value.start();
    assert.equal(started.ok, true, JSON.stringify(started));
    if (!started.ok) return;
    const ticket = runtime.value.issuePairingTicket();
    assert.equal(ticket.ok, true);
    if (!ticket.ok) return;
    const client = createWorkspaceHttpClient({
      baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
      pairingToken: ticket.value.ticket,
      origin: "http://quicklens.test",
    });
    assert.ok(client);
    const profiles = await client.read({
      operation: "managed-work.profile.list.v1",
      projectId: ProjectIdSchema.parse("project.managed"),
      contextId: WorkContextIdSchema.parse("context.managed"),
    });
    assert.equal(profiles.ok, true);
    if (profiles.ok && profiles.value.operation === "managed-work.profile.list.v1")
      assert.equal(profiles.value.profiles[0]?.profileId, "profile.codex.managed");
    const created = await client.command(createRequest());
    assert.equal(created.ok, true, JSON.stringify(created));
    if (!created.ok || created.value.operation !== "managed-work.create.v1") return;
    const selfNote = annotations.store.command(annotations.access, {
      operation: "annotation.note.create.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.managed.self-note"),
      projectId: ProjectIdSchema.parse("project.managed"),
      contextId: WorkContextIdSchema.parse("context.managed"),
      target: {
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        domain: "work_task",
        ref: created.value.work.taskId,
      },
      kind: "deferred",
      title: "Prepared task note",
      bodyMarkdown: "This exact prepared task instruction must reach start.",
      sourceBasisRef: "basis.managed.self",
      targetSnapshot: {
        basisRef: "basis.managed.self",
        capturedAt: "2026-09-16T00:00:00.000Z",
        value: { state: "prepared", taskId: created.value.work.taskId },
      },
    });
    assert.equal(selfNote.ok, true);
    const launched = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.start.v1",
        clientRequestId: "request.managed.start",
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        runId: created.value.work.runId,
        expectedRevision: created.value.work.revision,
      }),
    );
    assert.equal(launched.ok, true);
    if (!launched.ok || launched.value.operation !== "managed-work.start.v1") return;
    assert.equal(terminalRuntime.launches.flat().includes("selection-model"), true);
    assert.equal(terminalRuntime.launches.flat().includes("profile-model"), false);
    const mcpFiles = readdirSync(join(root, "mcp"));
    const mcpConfig = mcpFiles.find(
      (file) =>
        file.endsWith(".json") &&
        !file.endsWith(".credential.json") &&
        !file.endsWith(".packet.json"),
    );
    const credential = mcpFiles.find((file) => file.endsWith(".credential.json"));
    const packetFile = mcpFiles.find((file) => file.endsWith(".packet.json"));
    assert.ok(mcpConfig !== undefined && credential !== undefined && packetFile !== undefined);
    assert.match(readFileSync(join(root, "mcp", mcpConfig), "utf8"), /mcpServers|mcp_servers/);
    const packet = z
      .object({
        deferred: z.array(
          z.object({ attachmentId: z.string(), version: z.string(), bodyMarkdown: z.string() }),
        ),
      })
      .parse(JSON.parse(readFileSync(join(root, "mcp", packetFile), "utf8")));
    assert.match(packet.deferred[0]?.bodyMarkdown ?? "", /exact prepared task instruction/i);
    const managedToken = managedCredential(join(root, "mcp", credential));
    assert.notEqual(managedToken, agentToken);
    const agent = foundationAgent(agentStarted.value, managedToken);
    const managedAgent = createManagedWorkHttpClient({
      baseUrl: new URL(`http://${agentStarted.value.host}:${String(agentStarted.value.port)}`),
      principalToken: managedToken,
    });
    const nativeWork = createNativeWorkHttpClient({
      baseUrl: new URL(`http://${agentStarted.value.host}:${String(agentStarted.value.port)}`),
      principalToken: managedToken,
    });
    assert.equal(typeof agent.askUserQuestion, "function");
    if (agent.askUserQuestion === undefined) return;
    const question = await agent.askUserQuestion(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        clientRequestId: ClientRequestIdSchema.parse("request.managed.question"),
        draft: {
          title: "Managed worker question",
          introductionMarkdown: "Sent through /ZapAskUserQuestion.",
          items: [
            {
              questionItemId: QuestionItemIdSchema.parse("question-item.managed"),
              header: "Choice",
              promptMarkdown: "Continue?",
              contextMarkdown: null,
              artifactRefs: [],
              required: true,
              answerMode: "short_text",
              options: [],
              customAnswer: null,
              recommendation: null,
            },
          ],
          independentWorkAvailable: true,
          deadlineAt: null,
        },
      },
    );
    assert.equal(question.ok, true);
    const pendingQuestions = await client.read({
      operation: "question.list.v1",
      projectId: ProjectIdSchema.parse("project.managed"),
      contextId: WorkContextIdSchema.parse("context.managed"),
      state: "open",
      limit: 10,
    });
    assert.equal(pendingQuestions.ok, true);
    if (!pendingQuestions.ok || pendingQuestions.value.operation !== "question.list.v1") return;
    const pending = pendingQuestions.value.questions[0];
    assert.ok(pending !== undefined);
    if (pending === undefined) return;
    const pendingItem = pending.items[0];
    assert.ok(pendingItem !== undefined);
    if (pendingItem === undefined) return;
    const answered = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "question.answer.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.managed.answer"),
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        questionGroupId: QuestionGroupIdSchema.parse(pending.questionGroupId),
        expectedRevision: pending.revision,
        submission: {
          answers: [
            {
              questionItemId: QuestionItemIdSchema.parse(pendingItem.questionItemId),
              answer: { kind: "short_text", text: "yes" },
            },
          ],
          noteMarkdown: null,
        },
      }),
    );
    assert.equal(answered.ok, true);
    const waited = await agent.waitInbox(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      { afterSequence: "0", limit: 10, timeoutMilliseconds: 1_000 },
    );
    assert.equal(waited.ok, true, JSON.stringify(waited));
    if (waited.ok) {
      assert.equal(waited.value.deliveries.length > 0, true);
      const acknowledged = await agent.ack(
        AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
        {
          clientRequestId: ClientRequestIdSchema.parse("request.managed.answer.ack"),
          deliveryIds: waited.value.deliveries.map((delivery) => delivery.deliveryId),
        },
      );
      assert.equal(acknowledged.ok, true);
    }
    const secondQuestion = await agent.askUserQuestion(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        clientRequestId: ClientRequestIdSchema.parse("request.managed.cancel-question"),
        draft: {
          title: "Managed cancellation",
          introductionMarkdown: "This question is cancelled by the human UI.",
          items: [
            {
              questionItemId: QuestionItemIdSchema.parse("question-item.cancel-managed"),
              header: "Cancel",
              promptMarkdown: "Cancel this?",
              contextMarkdown: null,
              artifactRefs: [],
              required: true,
              answerMode: "short_text",
              options: [],
              customAnswer: null,
              recommendation: null,
            },
          ],
          independentWorkAvailable: false,
          deadlineAt: null,
        },
      },
    );
    assert.equal(secondQuestion.ok, true);
    const openAgain = await client.read({
      operation: "question.list.v1",
      projectId: ProjectIdSchema.parse("project.managed"),
      contextId: WorkContextIdSchema.parse("context.managed"),
      state: "open",
      limit: 10,
    });
    assert.equal(openAgain.ok, true);
    if (!openAgain.ok || openAgain.value.operation !== "question.list.v1") return;
    const cancellable = openAgain.value.questions[0];
    assert.ok(cancellable !== undefined);
    if (cancellable === undefined) return;
    const cancelled = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "question.cancel.v1",
        clientRequestId: ClientRequestIdSchema.parse("request.managed.cancel"),
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        questionGroupId: QuestionGroupIdSchema.parse(cancellable.questionGroupId),
        expectedRevision: cancellable.revision,
        reasonMarkdown: "The human chose to cancel this request.",
      }),
    );
    assert.equal(cancelled.ok, true);
    const impostor = await agent.askUserQuestion(
      AdapterSessionIdSchema.parse("adapter.managed.other.00000000000000000001"),
      {
        clientRequestId: ClientRequestIdSchema.parse("request.managed.impostor"),
        draft: {
          title: "Impostor",
          introductionMarkdown: "Should be refused.",
          items: [],
          independentWorkAvailable: false,
          deadlineAt: null,
        },
      },
    );
    assert.equal(impostor.ok, false);
    const managedInstruction = packet.deferred[0];
    assert.ok(managedInstruction !== undefined);
    if (managedInstruction === undefined) return;
    const attachmentAck = await managedAgent.acknowledgeAttachment(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        runId: launched.value.work.runId,
        attemptId: launched.value.work.attemptId,
        attachmentId: managedInstruction.attachmentId,
        version: managedInstruction.version,
      },
    );
    assert.equal(attachmentAck.ok, true);
    assert.deepEqual(attachmentAcks, []);
    const nativeAttempt = AttemptIdSchema.parse("attempt.native.managed.synthetic");
    const nativeBefore = await nativeWork.beforeWork(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        attemptId: nativeAttempt,
        targetRefs: [
          {
            projectId: "project.managed",
            contextId: "context.managed",
            domain: "work_task",
            ref: "task.native.declared",
          },
        ],
        sourceBasisRef: "plan.native.synthetic",
        planRevision: null,
      },
    );
    assert.equal(nativeBefore.ok, true);
    const nativeRead = await nativeWork.read(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      { attemptId: nativeAttempt },
    );
    assert.deepEqual(nativeRead, nativeBefore);
    const nativeAck = await nativeWork.acknowledgeAttachment(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        attemptId: nativeAttempt,
        attachmentId: "attachment.managed.note",
        version: "1",
      },
    );
    assert.equal(nativeAck.ok, true);
    assert.equal(attachmentAcks.at(-1), `${nativeAttempt}:attachment.managed.note:1`);
    const nativeCrossScope = await nativeWork.beforeWork(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        attemptId: AttemptIdSchema.parse("attempt.native.cross.synthetic"),
        targetRefs: [
          {
            projectId: "project.foreign",
            contextId: "context.foreign",
            domain: "work_task",
            ref: "task.foreign",
          },
        ],
        sourceBasisRef: "plan.foreign",
        planRevision: null,
      },
    );
    assert.equal(nativeCrossScope.ok, false);
    const childCreated = await managedAgent.create(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        clientRequestId: "request.managed.child.create",
        selection: {
          mode: "profile_override",
          profileId: "profile.codex.managed",
          reasonMarkdown: "Synthetic child explicitly reuses the managed fixture profile.",
        },
        goal: "Run bounded child work",
        expectedResult: "Typed child result",
        targetRefs: [
          {
            projectId: "project.managed",
            contextId: "context.managed",
            domain: "work_task",
            ref: "task.managed.child",
          },
        ],
      },
    );
    assert.equal(childCreated.ok, true);
    if (!childCreated.ok) return;
    assert.equal(JSON.stringify(childCreated.value).includes("controlLeaseId"), false);
    const child = ManagedWorkViewSchema.parse(childCreated.value);
    const childClaim = workStore.value.load(child.runId);
    assert.equal(childClaim.ok, true);
    if (childClaim.ok) {
      assert.equal(childClaim.value.creatorActorId, launched.value.work.actorId);
      assert.equal(childClaim.value.parentActorId, launched.value.work.actorId);
    }
    const childNetwork = await client.read({
      operation: "agent.network.v1",
      projectId: ProjectIdSchema.parse("project.managed"),
      contextId: WorkContextIdSchema.parse("context.managed"),
    });
    assert.equal(childNetwork.ok, true);
    if (childNetwork.ok && childNetwork.value.operation === "agent.network.v1")
      assert.equal(
        childNetwork.value.network.agents.find((actor) => actor.actorId === child.actorId)
          ?.parentActorId,
        launched.value.work.actorId,
      );
    const childContext = await agent.context(AdapterSessionIdSchema.parse(child.adapterSessionId));
    assert.equal(childContext.ok, true);
    if (childContext.ok)
      assert.equal(childContext.value.actor.parentActorId, launched.value.work.actorId);
    const childStarted = await managedAgent.start(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      { runId: child.runId, expectedRevision: child.revision },
    );
    assert.equal(childStarted.ok, true);
    if (!childStarted.ok) return;
    const childRunning = ManagedWorkViewSchema.parse(childStarted.value);
    const supervisorAccess = WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.managed.supervisor"),
      actorId: ActorIdSchema.parse(launched.value.work.actorId),
      clientId: ClientIdSchema.parse("client.managed.supervisor"),
      authorizedProjectIds: [ProjectIdSchema.parse("project.managed")],
    });
    const interrupted = await backend.interrupt(
      supervisorAccess,
      childRunning.runId,
      childRunning.revision,
    );
    assert.equal(interrupted.ok, true);
    if (!interrupted.ok) return;
    assert.notEqual(interrupted.value.controlLeaseId, null);
    const childCredentialFile = readdirSync(join(root, "mcp"))
      .filter((file) => file.endsWith(".credential.json"))
      .find((file) => file !== credential);
    assert.ok(childCredentialFile !== undefined);
    if (childCredentialFile === undefined) return;
    const childAgent = createManagedWorkHttpClient({
      baseUrl: new URL(`http://${agentStarted.value.host}:${String(agentStarted.value.port)}`),
      principalToken: managedCredential(join(root, "mcp", childCredentialFile)),
    });
    const childReported = await childAgent.report(
      AdapterSessionIdSchema.parse(child.adapterSessionId),
      {
        runId: child.runId,
        expectedRevision: interrupted.value.revision,
        summaryMarkdown: "Child reported through its authenticated actor session.",
        artifactRefs: [],
      },
    );
    assert.equal(childReported.ok, true);
    if (!childReported.ok) return;
    const childReport = ManagedWorkViewSchema.parse(childReported.value);
    const stopRequested = await backend.stop(
      supervisorAccess,
      childReport.runId,
      childReport.revision,
    );
    assert.equal(stopRequested.ok, true);
    if (stopRequested.ok) {
      assert.equal(stopRequested.value.state, "stopping");
      assert.equal(stopRequested.value.processExit, null);
      assert.equal(stopRequested.value.controlLeaseId, null);
    }
    const history = await client.events({
      cursor: {
        scope: {
          kind: "context",
          projectId: ProjectIdSchema.parse("project.managed"),
          contextId: WorkContextIdSchema.parse("context.managed"),
        },
        afterGlobalSequence: DecimalSchema.parse("0"),
      },
      limit: 100,
    });
    assert.equal(history.ok, true);
    if (history.ok)
      assert.deepEqual(
        history.value.events
          .filter((event) => JSON.stringify(event.payload).includes(child.runId))
          .map((event) => event.kind),
        ["managed-work.create.v1", "managed-work.start.v1", "managed-work.report.v1"],
      );
    const reported = await managedAgent.report(
      AdapterSessionIdSchema.parse(launched.value.work.adapterSessionId),
      {
        runId: launched.value.work.runId,
        expectedRevision: launched.value.work.revision,
        summaryMarkdown: "Scripted worker report.",
        artifactRefs: [],
      },
    );
    assert.equal(reported.ok, true);
    if (!reported.ok) return;
    const report = ManagedWorkViewSchema.parse(reported.value);
    const reviewed = await client.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "managed-work.review.v1",
        clientRequestId: "request.managed.review",
        projectId: ProjectIdSchema.parse("project.managed"),
        contextId: WorkContextIdSchema.parse("context.managed"),
        runId: report.runId,
        expectedRevision: report.revision,
        disposition: "accepted",
        commentMarkdown: "Reviewed scripted output.",
      }),
    );
    assert.equal(reviewed.ok, true);
    const reopened = openManagedWorkStore(workPath);
    assert.equal(reopened.ok, true);
    if (reopened.ok) {
      const retained = reopened.value.load(report.runId);
      assert.equal(retained.ok, true);
      if (retained.ok) assert.equal(retained.value.state, "accepted");
      reopened.value.close();
    }
  } finally {
    await runtime.value.close();
    await foundation.value.close();
    terminal.close();
    annotations.store.close();
    workStore.value.close();
    workspace.value.close();
    rmSync(root, { recursive: true, force: true });
  }
});
