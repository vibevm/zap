/** Durable managed-agent PTY backend. @scope spec://org.vibevm.zap/lens/PROP-010#managed-interaction */
import type { ModelSelection } from "../model-policy/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import { ActorIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  AgentSessionIdSchema,
  AttemptIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  TaskIdSchema,
  TerminalIdSchema,
  type WorkspaceAccessContext,
} from "../workspace-model/index.ts";
import {
  ManagedWorkClaimSchema,
  ManagedWorkRequestSchema,
  type ManagedAgentBackend,
  type ManagedExecutionFencePort,
  type ManagedWorkClaim,
  type ManagedWorkRequest,
  type ManagedWorkResult,
  type WorkAttachmentPort,
} from "./contracts.ts";
import type {
  ManagedAgentProfile,
  ManagedProviderDriver,
  ProtectedEnvironmentPort,
} from "./providers.ts";
import type { ManagedControlProvisioner } from "./control.ts";
import { controlTarget, managedControlView } from "./control-claim.ts";
import { ManagedAgentProfileSchema } from "./providers.ts";
import type { ManagedWorkStore } from "./store.ts";
import { resumeManagedWork } from "./resume.ts";
import { attachmentTargets, managedInstructions, writePacketFile } from "./packet.ts";
import { prepareManagedWorkspace, resolveManagedWorkspace } from "./workspace-backend.ts";
import {
  createLegacyManagedWorkspaceProvisioningPort,
  type ManagedWorkspaceProvisioningPort,
} from "./workspace.ts";

export interface ManagedActorBindingPort {
  prepare(input: {
    readonly request: ManagedWorkRequest;
    readonly taskId: string;
    readonly runId: string;
    readonly attemptId: string;
    readonly requesterActorId: string | null;
    readonly mcpConfigPath: string;
    readonly provider: ManagedAgentProfile["provider"];
    readonly mcpCommandPath: string | undefined;
    readonly mcpArgs: readonly string[] | undefined;
  }): Promise<
    ManagedWorkResult<{
      readonly actorId: string;
      readonly adapterSessionId: string;
      readonly mcpConfigPath: string;
      readonly environment: Readonly<Record<string, string>>;
    }>
  >;
  activate(input: {
    readonly runId: string;
    readonly actorId: string;
    readonly adapterSessionId: string;
    readonly mcpConfigPath: string;
    readonly provider: ManagedAgentProfile["provider"];
    readonly mcpCommandPath: string | undefined;
    readonly mcpArgs: readonly string[] | undefined;
  }): Promise<
    ManagedWorkResult<{
      readonly mcpConfigPath: string;
      readonly environment: Readonly<Record<string, string>>;
    }>
  >;
}
export interface ManagedSelectionPort {
  resolve(
    access: WorkspaceAccessContext,
    request: ManagedWorkRequest,
    profiles: readonly ManagedAgentProfile[],
    identity: { readonly runId: string; readonly attemptId: string },
  ): Promise<ManagedWorkResult<ModelSelection>>;
}
export interface ManagedParentPort {
  validate(
    access: WorkspaceAccessContext,
    request: ManagedWorkRequest,
  ): ManagedWorkResult<{
    readonly parentTaskId: string | null;
    readonly parentRunId: string | null;
    readonly parentActorId: string | null;
    readonly depth: number;
  }>;
}

export function createManagedAgentBackend(options: {
  readonly store: ManagedWorkStore;
  readonly terminals: ManagedTerminalServicePort;
  readonly profiles: readonly ManagedAgentProfile[];
  readonly drivers: ReadonlyMap<ManagedAgentProfile["provider"], ManagedProviderDriver>;
  readonly environment: ProtectedEnvironmentPort;
  readonly bindings: ManagedActorBindingPort;
  readonly selections: ManagedSelectionPort;
  readonly parents: ManagedParentPort;
  readonly attachments: WorkAttachmentPort;
  readonly execution: ManagedExecutionFencePort;
  readonly control: ManagedControlProvisioner;
  readonly workspaces?: ManagedWorkspaceProvisioningPort;
  readonly id: (kind: "task" | "run" | "attempt" | "session" | "terminal") => string;
  readonly clock?: () => Date;
}): ManagedAgentBackend {
  const clock = options.clock ?? (() => new Date());
  const workspaces =
    options.workspaces ??
    createLegacyManagedWorkspaceProvisioningPort({
      id: () => `workspace.assignment.${options.id("attempt")}`,
      now: () => clock().toISOString(),
    });
  const profiles = new Map(options.profiles.map((profile) => [profile.profileId, profile]));
  const preparationLocks = new Map<string, Promise<void>>();
  const observedClaim = (claim: ManagedWorkClaim): ManagedWorkClaim => {
    const target = controlTarget(claim);
    if (target === null) return claim;
    const observed = options.control.inspect(target);
    return observed.ok
      ? ManagedWorkClaimSchema.parse({
          ...claim,
          managedControl: managedControlView(target, observed.value, claim.provider),
        })
      : claim;
  };
  const scoped = (access: WorkspaceAccessContext, claim: ManagedWorkClaim) =>
    access.authorizedProjectIds.includes(claim.packet.projectId)
      ? ok(observedClaim(claim))
      : fail("forbidden", "managed work is outside authenticated project scope");
  const load = (access: WorkspaceAccessContext, runId: string) => {
    const claim = options.store.load(runId);
    return claim.ok ? scoped(access, claim.value) : claim;
  };
  return {
    control: options.control,
    get capabilities() {
      return [...profiles.values()].map((profile) => profile.capabilities);
    },
    registerProfile(raw) {
      const profile = ManagedAgentProfileSchema.safeParse(raw);
      if (!profile.success) return fail("forbidden", "managed profile is invalid");
      const current = profiles.get(profile.data.profileId);
      if (current !== undefined)
        return JSON.stringify(current) === JSON.stringify(profile.data)
          ? ok(current)
          : fail("conflict", "managed profile identity already has different content");
      profiles.set(profile.data.profileId, profile.data);
      return ok(profile.data);
    },
    get(access, runId) {
      return load(access, runId);
    },
    list(access, projectId, contextId) {
      const scopedProjectId = ProjectIdSchema.safeParse(projectId);
      if (!scopedProjectId.success || !access.authorizedProjectIds.includes(scopedProjectId.data))
        return fail("forbidden", "managed work is outside authenticated project scope");
      const listed = options.store.list(scopedProjectId.data, contextId);
      if (!listed.ok) return listed;
      return {
        ok: true,
        value: listed.value
          .filter(
            (claim) => claim.packet.projectId === projectId && claim.packet.contextId === contextId,
          )
          .map(observedClaim),
      };
    },
    profiles(access, projectId, contextId) {
      const scopedProjectId = ProjectIdSchema.safeParse(projectId);
      if (!scopedProjectId.success || !access.authorizedProjectIds.includes(scopedProjectId.data))
        return fail("forbidden", "managed profiles are outside authenticated project scope");
      return {
        ok: true,
        value: [...profiles.values()].filter(
          (profile) => profile.projectId === projectId && profile.contextId === contextId,
        ),
      };
    },
    async prepare(access, raw) {
      const request = ManagedWorkRequestSchema.safeParse(raw);
      if (!request.success || !access.authorizedProjectIds.includes(request.data.projectId))
        return fail("forbidden", "managed work request scope is invalid");
      const release = await acquirePreparationLock(
        preparationLocks,
        `${request.data.projectId}\0${request.data.contextId}\0${request.data.clientRequestId}`,
      );
      try {
        const prior = options.store.findRequest(request.data);
        if (!prior.ok) return prior;
        if (prior.value !== null) return scoped(access, prior.value);
        const candidates = [...profiles.values()].filter(
          (profile) =>
            profile.projectId === request.data.projectId &&
            profile.contextId === request.data.contextId,
        );
        if (candidates.length === 0)
          return fail("unavailable", "no managed worker profile is registered for this project");
        const parent = options.parents.validate(access, request.data);
        if (!parent.ok) return parent;
        const taskId = TaskIdSchema.parse(options.id("task"));
        const runId = RunIdSchema.parse(options.id("run"));
        const attemptId = AttemptIdSchema.parse(options.id("attempt"));
        const selection = await options.selections.resolve(access, request.data, candidates, {
          runId,
          attemptId,
        });
        if (!selection.ok) return selection;
        const profile = profiles.get(selection.value.profileId);
        if (
          profile === undefined ||
          profile.projectId !== request.data.projectId ||
          profile.contextId !== request.data.contextId
        )
          return fail("forbidden", "selected model policy profile is outside work scope");
        if (
          !profile.capabilities.installed ||
          !profile.capabilities.launchable ||
          profile.capabilities.interactiveTerminal !== "supported"
        )
          return fail(
            "unavailable",
            "managed provider is not observed launchable in a real terminal",
          );
        const binding = await options.bindings.prepare({
          request: request.data,
          taskId,
          runId,
          attemptId,
          requesterActorId: access.actorId,
          mcpConfigPath: profile.mcpConfigPath,
          provider: profile.provider,
          mcpCommandPath: profile.mcpCommandPath,
          mcpArgs: profile.mcpArgs,
        });
        if (!binding.ok) return binding;
        const workspace = await prepareManagedWorkspace({
          workspaces,
          store: options.store,
          access,
          request: request.data,
          taskId,
          runId,
          attemptId,
          actorId: binding.value.actorId,
        });
        if (!workspace.ok) return workspace;
        const claim = ManagedWorkClaimSchema.parse({
          taskId,
          runId,
          attemptId,
          actorId: ActorIdSchema.parse(binding.value.actorId),
          creatorActorId: access.actorId,
          parentActorId: parent.value.parentActorId,
          adapterSessionId: binding.value.adapterSessionId,
          sessionId: AgentSessionIdSchema.parse(options.id("session")),
          terminalId: TerminalIdSchema.parse(options.id("terminal")),
          controlLeaseId: null,
          controlEpoch: null,
          provider: profile.provider,
          profileId: profile.profileId,
          packet: {
            taskId,
            parentTaskId: parent.value.parentTaskId,
            projectId: request.data.projectId,
            contextId: request.data.contextId,
            planId: request.data.planId,
            workspaceAssignment: workspace.value,
            goal: request.data.goal,
            contextRefs: request.data.contextRefs,
            expectedResult: request.data.expectedResult,
            targetRefs: request.data.targetRefs,
            sourceBasisRef: request.data.sourceBasisRef,
            planRevision: request.data.planRevision,
            capabilities: profile.capabilities.evidence,
            depth: parent.value.depth,
            budgets: request.data.budgets,
            routing: { preferredProduct: profile.provider, executionMode: "managed" },
          },
          targetRefs: request.data.targetRefs,
          modelSelection: selection.value,
          state: "prepared",
          processExit: null,
          report: null,
          review: null,
          revision: "1",
        });
        return options.store.create(request.data, claim);
      } finally {
        release();
      }
    },
    async start(access, runId, expectedRevision) {
      const loaded = load(access, runId);
      if (!loaded.ok) return loaded;
      if (loaded.value.revision !== expectedRevision || loaded.value.state !== "prepared")
        return fail("conflict", "managed work is not at the prepared revision");
      const execution = options.execution.canStart(access, loaded.value);
      if (!execution.ok) return execution;
      const profile = profiles.get(loaded.value.profileId);
      const driver = profile === undefined ? undefined : options.drivers.get(profile.provider);
      if (profile === undefined || driver === undefined)
        return fail("unavailable", "managed provider driver is unavailable");
      const notes = await options.attachments.prepareBeforeWork({
        access,
        attemptId: loaded.value.attemptId,
        recipientActorId: loaded.value.actorId,
        targets: attachmentTargets(loaded.value),
        sourceBasisRef: loaded.value.packet.sourceBasisRef,
        planRevision: loaded.value.packet.planRevision,
      });
      if (!notes.ok || notes.value.state !== "ready")
        return notes.ok
          ? fail("unavailable", "declared work target is waiting for attachment resolution")
          : notes;
      const env = await options.environment.resolve(profile.environmentRef);
      if (!env.ok) return fail("unavailable", env.message);
      const activated = await options.bindings.activate({
        runId,
        actorId: loaded.value.actorId,
        adapterSessionId: loaded.value.adapterSessionId,
        mcpConfigPath: profile.mcpConfigPath,
        provider: profile.provider,
        mcpCommandPath: profile.mcpCommandPath,
        mcpArgs: profile.mcpArgs,
      });
      if (!activated.ok) return transition(options.store, loaded.value, "uncertain");
      const workspace = await resolveManagedWorkspace(
        workspaces,
        "initial",
        access,
        loaded.value,
        profile.cwd,
      );
      if (!workspace.ok) return workspace;
      const packet = writePacketFile(loaded.value, notes.value.instructions, profile.mcpConfigPath);
      if (!packet.ok) return transition(options.store, loaded.value, "uncertain");
      const launch = driver.launch({
        profile: ManagedAgentProfileSchema.parse({
          ...profile,
          mcpConfigPath: activated.value.mcpConfigPath,
        }),
        workspaceCwd: workspace.value.cwd,
        selection: loaded.value.modelSelection,
        instructions: managedInstructions(loaded.value, packet.value),
        environment: { ...env.value, ...activated.value.environment },
        trustedZapMcp: true,
      });
      const controlled = await options.control.prepare({
        runId: loaded.value.runId,
        actorId: ActorIdSchema.parse(loaded.value.actorId),
        sessionId: loaded.value.sessionId,
        terminalId: loaded.value.terminalId,
        provider: loaded.value.provider,
        launch,
      });
      if (!controlled.ok) return transition(options.store, loaded.value, "uncertain");
      const admitted = options.execution.canStart(access, loaded.value);
      if (!admitted.ok) {
        options.control.discard(controlled.value.target);
        return admitted;
      }
      const launching = transition(options.store, loaded.value, "launching");
      if (!launching.ok) {
        options.control.discard(controlled.value.target);
        return launching;
      }
      const started = await options.terminals.start({
        accessProjectId: launching.value.packet.projectId,
        accessContextId: launching.value.packet.contextId,
        spec: {
          terminalId: launching.value.terminalId,
          projectId: launching.value.packet.projectId,
          contextId: launching.value.packet.contextId,
          sessionId: launching.value.sessionId,
          runId: launching.value.runId,
          executable: controlled.value.launch.executable,
          args: [...controlled.value.launch.args],
          cwd: controlled.value.launch.cwd,
          env: controlled.value.launch.env,
        },
      });
      if (!started.ok) {
        options.control.discard(controlled.value.target);
        return transition(options.store, launching.value, "uncertain");
      }
      const lease = options.terminals.acquire(access, {
        terminalId: started.value.terminalId,
        expectedControlEpoch: started.value.controlEpoch,
        takeover: true,
      });
      if (!lease.ok) return transition(options.store, launching.value, "uncertain");
      if (lease.value.lease === null)
        return transition(options.store, launching.value, "uncertain");
      const control = await options.control.activate({
        target: controlled.value.target,
        access,
        leaseId: lease.value.lease.leaseId,
        automationControlEpoch: String(lease.value.lease.controlEpoch),
      });
      if (!control.ok) return transition(options.store, launching.value, "uncertain");
      const observed = options.control.inspect(controlled.value.target);
      return transition(options.store, launching.value, "running", {
        controlLeaseId: lease.value.lease.leaseId,
        controlEpoch: String(lease.value.lease.controlEpoch),
        managedControl: observed.ok
          ? managedControlView(controlled.value.target, observed.value, loaded.value.provider)
          : null,
      });
    },
    interrupt: async (access, runId, expectedRevision) => {
      await Promise.resolve();
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      if (claim.value.revision !== expectedRevision)
        return fail("conflict", "managed work revision changed");
      if (claim.value.controlLeaseId === null || claim.value.controlEpoch === null)
        return fail("unavailable", "managed work has no server automation lease");
      const epoch = Number(claim.value.controlEpoch);
      if (!Number.isSafeInteger(epoch))
        return fail("unavailable", "managed control epoch is unavailable");
      const interrupted = options.terminals.interrupt(
        access,
        claim.value.terminalId,
        claim.value.controlLeaseId,
        epoch,
      );
      return interrupted.ok
        ? transition(options.store, claim.value, "stopping")
        : transition(options.store, claim.value, "uncertain");
    },
    async stop(access, runId, expectedRevision) {
      await Promise.resolve();
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      if (claim.value.revision !== expectedRevision)
        return fail("conflict", "managed work revision changed");
      if (claim.value.controlLeaseId === null || claim.value.controlEpoch === null)
        return transition(options.store, claim.value, "uncertain");
      const epoch = Number(claim.value.controlEpoch);
      if (!Number.isSafeInteger(epoch)) return transition(options.store, claim.value, "uncertain");
      const stopped = options.terminals.stop(
        access,
        claim.value.terminalId,
        claim.value.controlLeaseId,
        epoch,
      );
      return transition(options.store, claim.value, stopped.ok ? "stopping" : "uncertain", {
        controlLeaseId: null,
        controlEpoch: null,
      });
    },
    async continueRun(access, runId, expectedRevision) {
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      const profile = profiles.get(claim.value.profileId);
      const driver = profile === undefined ? undefined : options.drivers.get(profile.provider);
      if (profile === undefined || driver === undefined)
        return fail("unavailable", "managed provider resume driver is unavailable");
      return resumeManagedWork({
        access,
        claim: claim.value,
        expectedRevision,
        profile,
        driver,
        environment: options.environment,
        bindings: options.bindings,
        control: options.control,
        workspaces,
        terminals: options.terminals,
        execution: options.execution,
        store: options.store,
        terminalId: TerminalIdSchema.parse(options.id("terminal")),
      });
    },
    async reconcile(access, runId) {
      await Promise.resolve();
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      const terminal = options.terminals.snapshot(access, claim.value.terminalId);
      if (!terminal.ok) return claim;
      if (terminal.value.state === "running")
        return claim.value.state === "running"
          ? claim
          : transition(options.store, claim.value, "running");
      const target = controlTarget(claim.value);
      if (target !== null) options.control.settleStopped(target);
      const observed = target === null ? null : options.control.inspect(target);
      const paused = observed?.ok === true && observed.value.pauseRequested;
      const next = transition(
        options.store,
        claim.value,
        terminal.value.state === "exited" ? (paused ? "paused" : "stopped") : "failed",
        {
          processExit: {
            code: terminal.value.state === "exited" ? 0 : null,
            observedAt: clock().toISOString(),
          },
          managedControl:
            target !== null && observed?.ok === true
              ? managedControlView(target, observed.value, claim.value.provider)
              : claim.value.managedControl,
          controlLeaseId: null,
          controlEpoch: null,
        },
      );
      return next;
    },
    async report(access, runId, expectedRevision, report) {
      await Promise.resolve();
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      if (claim.value.revision !== expectedRevision)
        return fail("conflict", "managed work revision changed");
      const observed = observedClaim(claim.value);
      if (observed.actorId !== access.actorId)
        return fail("forbidden", "only the managed actor can report this run");
      if (!["running", "waiting_for_user", "stopping"].includes(observed.state))
        return fail("conflict", "managed work is not in a reportable state");
      return transition(options.store, observed, "reported", {
        report: {
          summaryMarkdown: report.summaryMarkdown,
          artifactRefs: [...report.artifactRefs],
          reportedAt: clock().toISOString(),
        },
      });
    },
    async review(access, runId, expectedRevision, review) {
      await Promise.resolve();
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      if (claim.value.revision !== expectedRevision || claim.value.report === null)
        return fail("conflict", "managed work report is not reviewable at this revision");
      return transition(options.store, claim.value, review.disposition, {
        review: { ...review, reviewerActorId: access.actorId, reviewedAt: clock().toISOString() },
      });
    },
    async acknowledgeAttachment(access, runId, attemptId, attachmentId, version) {
      const claim = load(access, runId);
      if (!claim.ok) return claim;
      if (claim.value.attemptId !== attemptId)
        return fail("forbidden", "attachment acknowledgement attempt does not match managed run");
      return options.attachments.acknowledge({
        access,
        attemptId,
        attachmentId,
        version,
      });
    },
  };
}
function transition(
  store: ManagedWorkStore,
  claim: ManagedWorkClaim,
  state: ManagedWorkClaim["state"],
  patch: Partial<ManagedWorkClaim> = {},
) {
  const next = ManagedWorkClaimSchema.parse({
    ...claim,
    ...patch,
    state,
    revision: DecimalSchema.parse(String(BigInt(claim.revision) + 1n)),
  });
  return store.transition(claim.runId, claim.revision, next);
}
function ok<T>(value: T): ManagedWorkResult<T> {
  return { ok: true, value };
}
function fail(
  code: "forbidden" | "conflict" | "unavailable",
  message: string,
): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}

async function acquirePreparationLock(
  locks: Map<string, Promise<void>>,
  key: string,
): Promise<() => void> {
  const found = locks.get(key);
  const prior = found === undefined ? Promise.resolve() : found;
  let release = (): void => undefined;
  const current = new Promise<void>((resolveCurrent) => {
    release = resolveCurrent;
  });
  const tail = prior.then(() => current);
  locks.set(key, tail);
  await prior;
  return () => {
    release();
    if (locks.get(key) === tail) locks.delete(key);
  };
}
