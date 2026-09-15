/** Real harmless PTY proof for configured managed runtime. @scope spec://org.vibevm.zap/lens/PROP-006#delivery-order */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import type { AgentHost } from "../agent-runtime/index.ts";
import { ActorIdSchema, DecimalSchema, PrincipalIdSchema } from "../protocol/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import {
  ClientIdSchema,
  ExecutionHostIdSchema,
  ProjectIdSchema,
  WorkContextIdSchema,
  WorkspaceCommandRequestSchema,
  WorkspaceAccessContextSchema,
  type WorkspaceAccessContext,
  type WorkspaceClientPort,
  type TerminalId,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime } from "./index.ts";
import {
  createManagedRuntimeController,
  type ConfiguredManagedTerminalService,
} from "./managed.ts";

const powershell = "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";

test("disabled managed runtime leaves native-only composition independent", async () => {
  const created = createManagedRuntimeController({
    enabled: false,
    databasePath: "C:/unused/managed-terminal.sqlite",
    profiles: [],
    outputHistoryLimit: 128,
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const started = await created.value.start();
  assert.equal(started.ok, true);
  if (started.ok) assert.equal(started.value.service, undefined);
});

test("configured real PTYs fence control and project stop preserves another project", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "wayfinder-pty-"));
  const created = createManagedRuntimeController({
    enabled: true,
    databasePath: join(directory, "terminal.sqlite"),
    profiles: [profile("a"), profile("b")],
    outputHistoryLimit: 128,
  });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  const opened = await created.value.start();
  assert.equal(opened.ok, true);
  if (!opened.ok || opened.value.service === undefined) return;
  const service = opened.value.service;
  const clientA = access("a", "one");
  const clientA2 = access("a", "two");
  const clientB = access("b", "one");
  const startedA = await service.startRegistered(clientA, launch("a"));
  const startedB = await service.startRegistered(clientB, launch("b"));
  assert.equal(startedA.ok, true);
  assert.equal(startedB.ok, true);
  if (!startedA.ok || !startedB.ok) return;
  const processA = startedA.value.processId;
  const processB = startedB.value.processId;
  assert.ok(processA > 0);
  assert.ok(processB > 0);

  const leaseA = service.acquire(clientA, {
    terminalId: "terminal.a",
    expectedControlEpoch: startedA.value.controlEpoch,
  });
  assert.equal(leaseA.ok, true);
  if (!leaseA.ok || leaseA.value.lease === null) return;
  const denied = service.acquire(clientA2, {
    terminalId: "terminal.a",
    expectedControlEpoch: leaseA.value.controlEpoch,
  });
  assert.equal(denied.ok, false);
  const takeover = service.acquire(clientA2, {
    terminalId: "terminal.a",
    expectedControlEpoch: leaseA.value.controlEpoch,
    takeover: true,
  });
  assert.equal(takeover.ok, true);
  if (!takeover.ok || takeover.value.lease === null) return;
  assert.equal(
    service.input(
      clientA,
      "terminal.a",
      leaseA.value.lease.leaseId,
      leaseA.value.controlEpoch,
      "Write-Output 'STALE'\r",
    ).ok,
    false,
  );
  assert.equal(
    service.input(
      clientA2,
      "terminal.a",
      takeover.value.lease.leaseId,
      takeover.value.controlEpoch,
      "Write-Output 'WAYFINDER_MANAGED_READY'\r",
    ).ok,
    true,
  );
  await waitForOutput(service, clientA, "terminal.a", "WAYFINDER_MANAGED_READY");

  const leaseB = service.acquire(clientB, {
    terminalId: "terminal.b",
    expectedControlEpoch: startedB.value.controlEpoch,
  });
  assert.equal(leaseB.ok, true);
  const stopped = await service.stopProject(clientA2, "project.a", "context.a");
  assert.equal(stopped.ok, true);
  await waitForProcessAbsent(processA);
  assert.equal(service.snapshot(clientA2, "terminal.a").ok, true);
  const snapshotB = service.snapshot(clientB, "terminal.b");
  assert.equal(snapshotB.ok, true);
  if (snapshotB.ok) assert.equal(snapshotB.value.state, "running");
  if (leaseB.ok && leaseB.value.lease !== null) {
    service.input(
      clientB,
      "terminal.b",
      leaseB.value.lease.leaseId,
      leaseB.value.controlEpoch,
      "exit\r",
    );
  }
  await waitForExit(service, clientB, "terminal.b");
  await waitForProcessAbsent(processB);
  console.log(
    `MANAGED_PTY_RECEIPT ${JSON.stringify({ processA, processB, processAExited: true, processBExited: true })}`,
  );
  created.value.close();
  rmSync(directory, { recursive: true, force: true });
});

test("authenticated HTTP clients share one real managed PTY and fenced control", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "wayfinder-http-pty-"));
  const config = {
    version: 1,
    state: { databasePath: join(directory, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "managed",
      pairingToken: "synthetic-managed-pairing-token-0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
    },
    profiles: [nativeProfile()],
    projects: [runtimeProject()],
    modelPolicies: [],
    managedTerminals: {
      enabled: true,
      databasePath: join(directory, "terminal.sqlite"),
      profiles: [profile("a")],
      outputHistoryLimit: 128,
    },
  };
  const fakeHost: AgentHost = {
    hostId: ExecutionHostIdSchema.parse("host.managed.fake"),
    profileIds: ["profile.codex.native"],
    openCoordinator: () =>
      Promise.resolve({
        ok: false,
        error: { code: "unsupported", message: "managed test starts no model", retry: "never" },
      }),
  };
  const created = createWayfinderRuntime(config, { hosts: [fakeHost] });
  assert.equal(created.ok, true);
  if (!created.ok) return;
  try {
    const started = await created.value.start();
    assert.equal(started.ok, true);
    if (!started.ok) return;
    const clientA = httpClient(created.value, started.value.port, started.value.basePath);
    assert.notEqual(clientA, null);
    if (clientA === null) return;
    const terminal = await clientA.command(
      WorkspaceCommandRequestSchema.parse({
        operation: "terminal.start.v1",
        clientRequestId: "request.managed.start",
        projectId: "project.a",
        contextId: "context.a",
        profileId: "managed.profile.a",
        terminalId: "terminal.http",
        sessionId: "session.http",
        runId: "run.http",
      }),
    );
    assert.equal(terminal.ok, true, JSON.stringify(terminal));
    if (!terminal.ok || terminal.value.operation !== "terminal.start.v1") return;
    const clientB = httpClient(created.value, started.value.port, started.value.basePath);
    assert.notEqual(clientB, null);
    if (clientB === null) return;
    const listed = await clientB.read({
      operation: "terminal.list.v1",
      projectId: ProjectIdSchema.parse("project.a"),
      contextId: WorkContextIdSchema.parse("context.a"),
    });
    assert.equal(listed.ok, true);
    if (!listed.ok || listed.value.operation !== "terminal.list.v1") return;
    assert.equal(listed.value.terminals.length, 1);
    const discovered = listed.value.terminals[0];
    assert.notEqual(discovered, undefined);
    if (discovered === undefined) return;
    const network = await clientB.read({
      operation: "agent.network.v1",
      projectId: ProjectIdSchema.parse("project.a"),
      contextId: WorkContextIdSchema.parse("context.a"),
    });
    assert.equal(network.ok, true);
    if (!network.ok || network.value.operation !== "agent.network.v1") return;
    const managedAgent = network.value.network.agents.find(
      (agent) => agent.executionMode === "managed",
    );
    assert.equal(managedAgent?.terminalId, discovered.terminalId);
    assert.equal(managedAgent?.state, "active");
    const leaseA = await clientA.command(
      terminalCommand(
        "terminal.acquire.v1",
        "request.managed.acquire-a",
        {
          expectedControlEpoch: terminal.value.terminal.controlEpoch,
          takeover: false,
        },
        discovered.terminalId,
      ),
    );
    assert.equal(leaseA.ok, true);
    if (!leaseA.ok || leaseA.value.operation !== "terminal.acquire.v1") return;
    const denied = await clientB.command(
      terminalCommand(
        "terminal.acquire.v1",
        "request.managed.acquire-b",
        {
          expectedControlEpoch: leaseA.value.lease.controlEpoch,
          takeover: false,
        },
        discovered.terminalId,
      ),
    );
    assert.equal(denied.ok, false);
    const takeover = await clientB.command(
      terminalCommand(
        "terminal.acquire.v1",
        "request.managed.takeover",
        {
          expectedControlEpoch: leaseA.value.lease.controlEpoch,
          takeover: true,
        },
        discovered.terminalId,
      ),
    );
    assert.equal(takeover.ok, true);
    if (!takeover.ok || takeover.value.operation !== "terminal.acquire.v1") return;
    const stale = await clientA.command(
      terminalCommand(
        "terminal.input.v1",
        "request.managed.stale",
        {
          expectedControlEpoch: leaseA.value.lease.controlEpoch,
          leaseId: leaseA.value.lease.leaseId,
          data: "Write-Output 'STALE'\r",
        },
        discovered.terminalId,
      ),
    );
    assert.equal(stale.ok, false);
    const echoed = await clientB.command(
      terminalCommand(
        "terminal.input.v1",
        "request.managed.echo",
        {
          expectedControlEpoch: takeover.value.lease.controlEpoch,
          leaseId: takeover.value.lease.leaseId,
          data: "Write-Output 'WAYFINDER_HTTP_PTY_READY'\r",
        },
        discovered.terminalId,
      ),
    );
    assert.equal(echoed.ok, true);
    await waitForHttpOutput(clientA, discovered.terminalId, "WAYFINDER_HTTP_PTY_READY");
    await clientB.command(
      terminalCommand(
        "terminal.input.v1",
        "request.managed.exit",
        {
          expectedControlEpoch: takeover.value.lease.controlEpoch,
          leaseId: takeover.value.lease.leaseId,
          data: "exit\r",
        },
        discovered.terminalId,
      ),
    );
    await waitForHttpTerminalState(clientB, discovered.terminalId, ["exited", "stopped"]);
    const settledNetwork = await clientA.read({
      operation: "agent.network.v1",
      projectId: ProjectIdSchema.parse("project.a"),
      contextId: WorkContextIdSchema.parse("context.a"),
    });
    assert.equal(settledNetwork.ok, true);
    if (settledNetwork.ok && settledNetwork.value.operation === "agent.network.v1") {
      assert.equal(
        settledNetwork.value.network.agents.find((agent) => agent.executionMode === "managed")
          ?.state,
        "stopped",
      );
    }
  } finally {
    await created.value.close();
    rmSync(directory, { recursive: true, force: true });
  }
});

function profile(name: string) {
  return {
    profileId: `managed.profile.${name}`,
    projectId: `project.${name}`,
    contextId: `context.${name}`,
    executable: powershell,
    args: ["-NoLogo", "-NoProfile"],
    cwd: "C:/Windows",
    label: `Managed ${name}`,
  };
}

function launch(name: string) {
  return {
    profileId: `managed.profile.${name}`,
    projectId: `project.${name}`,
    contextId: `context.${name}`,
    terminalId: `terminal.${name}`,
    sessionId: `session.${name}`,
    runId: `run.${name}`,
  };
}

function access(project: string, client: string) {
  return WorkspaceAccessContextSchema.parse({
    principalId: PrincipalIdSchema.parse(`principal.${project}`),
    actorId: ActorIdSchema.parse(`actor.${project}`),
    clientId: ClientIdSchema.parse(`client.${project}.${client}`),
    authorizedProjectIds: [ProjectIdSchema.parse(`project.${project}`)],
  });
}

async function waitForOutput(
  service: ConfiguredManagedTerminalService,
  access: WorkspaceAccessContext,
  terminalId: string,
  marker: string,
): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const read = service.read(access, {
      projectId: terminalId.replace("terminal", "project"),
      contextId: terminalId.replace("terminal", "context"),
      terminalId,
      afterSequence: 0,
      limit: 128,
    });
    if (read.ok && read.value.events.some((event) => event.data.includes(marker))) return;
    await delay(20);
  }
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-006#terminal-evidence: PTY marker was not observed; fix surface: repair managed output propagation",
  );
}

async function waitForExit(
  service: ConfiguredManagedTerminalService,
  access: WorkspaceAccessContext,
  terminalId: string,
): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const snapshot = service.snapshot(access, terminalId);
    if (snapshot.ok && (snapshot.value.state === "exited" || snapshot.value.state === "stopped")) {
      return;
    }
    await delay(20);
  }
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-006#supervision: PTY exit was not observed; fix surface: repair managed process exit observation",
  );
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitForProcessAbsent(processId: number): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (!processAlive(processId)) return;
    await delay(20);
  }
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-006#supervision: managed PTY process ${String(processId)} remained alive after observed exit; fix surface: await exact child-process settlement`,
  );
}

function processAlive(processId: number): boolean {
  try {
    process.kill(processId, 0);
    return true;
  } catch {
    return false;
  }
}

function httpClient(
  runtime: {
    issuePairingTicket(): { readonly ok: boolean; readonly value?: { readonly ticket: string } };
  },
  port: number,
  basePath: string,
) {
  const ticket = runtime.issuePairingTicket();
  if (!ticket.ok || ticket.value === undefined) return null;
  return createWorkspaceHttpClient({
    baseUrl: `http://127.0.0.1:${String(port)}${basePath}`,
    pairingToken: ticket.value.ticket,
    origin: "http://quicklens.test",
  });
}

function terminalCommand(
  operation: "terminal.acquire.v1" | "terminal.input.v1",
  clientRequestId: string,
  fields: Readonly<Record<string, unknown>>,
  terminalId: TerminalId,
) {
  return WorkspaceCommandRequestSchema.parse({
    operation,
    clientRequestId,
    projectId: "project.a",
    contextId: "context.a",
    terminalId,
    ...fields,
  });
}

async function waitForHttpOutput(
  client: WorkspaceClientPort,
  terminalId: TerminalId,
  marker: string,
): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const read = await client.read({
      operation: "terminal.output.page.v1",
      projectId: ProjectIdSchema.parse("project.a"),
      contextId: WorkContextIdSchema.parse("context.a"),
      terminalId,
      afterSequence: DecimalSchema.parse("0"),
      limit: 128,
    });
    if (
      read.ok &&
      read.value.operation === "terminal.output.page.v1" &&
      read.value.page.events.some((event) => event.data.includes(marker))
    ) {
      return;
    }
    await delay(20);
  }
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-006#network-and-control: HTTP PTY marker was not observed; fix surface: repair authenticated managed terminal routing",
  );
}

async function waitForHttpTerminalState(
  client: WorkspaceClientPort,
  terminalId: TerminalId,
  states: readonly string[],
): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const listed = await client.read({
      operation: "terminal.list.v1",
      projectId: ProjectIdSchema.parse("project.a"),
      contextId: WorkContextIdSchema.parse("context.a"),
    });
    if (
      listed.ok &&
      listed.value.operation === "terminal.list.v1" &&
      listed.value.terminals.some(
        (terminal) => terminal.terminalId === terminalId && states.includes(terminal.state),
      )
    ) {
      return;
    }
    await delay(20);
  }
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-006#supervision: terminal settlement was not discoverable; fix surface: retain the observed terminal exit state",
  );
}

function nativeProfile() {
  return {
    profileId: "profile.codex.native",
    executablePath: "C:/placeholder/codex.exe",
    requestTimeoutMs: 30_000,
    model: "gpt-5.6-luna",
    effort: "low",
    approvalPolicy: "on-request",
    sandbox: "workspace-write",
    personality: "pragmatic",
    serviceName: "managed-test",
  };
}

function runtimeProject() {
  return {
    registrationId: "registration.managed.a",
    projectId: "project.a",
    displayName: "Managed A",
    repositoryRootRefs: ["repository.a"],
    actions: { startCoordinator: { state: "available" } },
    context: {
      contextId: "context.a",
      displayName: "Managed context A",
      workspaceRef: "workspace.a",
      branchLabel: "main",
      revisionBinding: "revision.a",
      planning: { state: "unavailable", reason: "Synthetic managed fixture" },
      coordinatorConversationId: "conversation.a",
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.codex.native",
        label: "Native",
        interactionKind: "structured",
        availability: { state: "available" },
      },
    ],
    protected: { cwd: "C:/placeholder/a", launchProfileRef: "profile.codex.native" },
  };
}
