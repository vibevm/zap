/** @scope spec://org.vibevm.zap/lens/PROP-006#shared-clients */
/** Workspace controls for a registered local managed-worker profile. */
import { $, component$, useSignal, useVisibleTask$, type NoSerialize } from "@qwik.dev/core";

import {
  workspaceRequestId,
  type ProjectWorkspaceView,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import {
  AgentSessionIdSchema,
  RunIdSchema,
  TerminalCapabilitySchema,
  TerminalIdSchema,
  type AgentDescriptor,
  type ManagedTerminalView,
  type ProjectId,
  type TerminalLeaseView,
  type WorkContextId,
} from "../workspace-model/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import { ManagedTerminalPanel } from "./managed-terminal-panel.tsx";
import { ownedTerminalOptions, selectedAgent } from "./workspace-helpers.ts";

export const ManagedTerminalWorkspace = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly view: ProjectWorkspaceView;
  readonly selectedActorId: AgentDescriptor["actorId"] | null;
}>((props) => {
  const options = ownedTerminalOptions(props.view);
  const selectedProfile = useSignal(options[0]?.profileId ?? "");
  const known = useSignal<readonly ManagedTerminalView[]>([]);
  const terminal = useSignal<ManagedTerminalView | null>(null);
  const lease = useSignal<TerminalLeaseView | null>(null);
  const output = useSignal<readonly { readonly data: string }[]>([]);
  const message = useSignal<string | null>(null);

  useVisibleTask$(({ track }) => {
    const preferred = track(
      () => selectedAgent(props.view, props.selectedActorId)?.terminalId ?? null,
    );
    const port = props.port;
    if (port === undefined) return;
    const load = async (): Promise<void> => {
      const listed = await port.read({
        operation: "terminal.list.v1",
        projectId: props.projectId,
        contextId: props.contextId,
      });
      if (!listed.ok || listed.value.operation !== "terminal.list.v1") return;
      known.value = listed.value.terminals;
      terminal.value =
        listed.value.terminals.find((item) => item.terminalId === preferred) ??
        listed.value.terminals.find((item) => item.state === "running") ??
        null;
    };
    void load();
  });

  useVisibleTask$(({ track, cleanup }) => {
    const terminalId = track(() => terminal.value?.terminalId ?? null);
    if (terminalId === null) return;
    let active = true;
    let after = DecimalSchema.parse("0");
    const poll = async (): Promise<void> => {
      const port = props.port;
      if (!active || port === undefined) return;
      const read = await port.read({
        operation: "terminal.output.page.v1",
        projectId: props.projectId,
        contextId: props.contextId,
        terminalId,
        afterSequence: after,
        limit: 128,
      });
      if (!read.ok || read.value.operation !== "terminal.output.page.v1") return;
      output.value = [...output.value, ...read.value.page.events];
      after = read.value.page.nextSequence ?? after;
    };
    void poll();
    const timer = setInterval(() => void poll(), 250);
    cleanup(() => {
      active = false;
      clearInterval(timer);
    });
  });

  if (options.length === 0) return null;

  const capability = TerminalCapabilitySchema.parse(
    terminal.value === null
      ? { state: "unavailable", reason: "policy_disabled" }
      : {
          state: "available",
          terminalId: terminal.value.terminalId,
          ownerClientId: lease.value?.clientId ?? null,
          controlEpoch: lease.value?.controlEpoch ?? terminal.value.controlEpoch,
          leaseId: lease.value?.leaseId ?? null,
          leaseExpiresAt: null,
          inputEnabled: lease.value !== null,
        },
  );
  const selected = options.find((option) => option.profileId === selectedProfile.value);

  return (
    <div>
      {known.value.length < 2 ? null : (
        <label class="field-label">
          Active terminal
          <select
            value={terminal.value?.terminalId ?? ""}
            onChange$={(_, element) => {
              terminal.value =
                known.value.find((item) => item.terminalId === element.value) ?? null;
              lease.value = null;
              output.value = [];
            }}
          >
            {known.value.map((item) => (
              <option key={item.terminalId} value={item.terminalId}>
                {`${item.terminalId} · ${item.state}`}
              </option>
            ))}
          </select>
        </label>
      )}
      {options.length < 2 || terminal.value !== null ? null : (
        <label class="field-label">
          Managed profile
          <select
            value={selectedProfile.value}
            onChange$={(_, element) => {
              selectedProfile.value = element.value;
            }}
          >
            {options.map((option) => (
              <option key={option.profileId} value={option.profileId}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      )}
      <ManagedTerminalPanel
        capability={capability}
        output={output.value}
        profileLabel={selected?.label}
        onStart$={
          terminal.value === null
            ? $(async () => {
                const port = props.port;
                if (port === undefined || selected === undefined) return;
                const identity = crypto.randomUUID();
                const started = await port.command({
                  operation: "terminal.start.v1",
                  clientRequestId: workspaceRequestId("managed-start"),
                  projectId: props.projectId,
                  contextId: props.contextId,
                  profileId: selected.profileId,
                  terminalId: TerminalIdSchema.parse(`terminal.${identity}`),
                  sessionId: AgentSessionIdSchema.parse(`session.${identity}`),
                  runId: RunIdSchema.parse(`run.${identity}`),
                });
                if (!started.ok || started.value.operation !== "terminal.start.v1") {
                  message.value = started.ok
                    ? "Managed start returned no terminal."
                    : started.error.message;
                  return;
                }
                terminal.value = started.value.terminal;
                known.value = [...known.value, started.value.terminal];
                const acquired = await port.command({
                  operation: "terminal.acquire.v1",
                  clientRequestId: workspaceRequestId("managed-acquire"),
                  projectId: props.projectId,
                  contextId: props.contextId,
                  terminalId: started.value.terminal.terminalId,
                  expectedControlEpoch: started.value.terminal.controlEpoch,
                  takeover: false,
                });
                if (acquired.ok && acquired.value.operation === "terminal.acquire.v1") {
                  lease.value = acquired.value.lease;
                  message.value = "Managed worker started. Input lease acquired.";
                } else
                  message.value = acquired.ok ? "Input lease unavailable." : acquired.error.message;
              })
            : undefined
        }
        onAcquire$={
          terminal.value === null || lease.value !== null
            ? undefined
            : $(async () => {
                const port = props.port;
                if (port === undefined || terminal.value === null) return;
                const acquired = await port.command({
                  operation: "terminal.acquire.v1",
                  clientRequestId: workspaceRequestId("managed-takeover"),
                  projectId: props.projectId,
                  contextId: props.contextId,
                  terminalId: terminal.value.terminalId,
                  expectedControlEpoch: terminal.value.controlEpoch,
                  takeover: true,
                });
                if (acquired.ok && acquired.value.operation === "terminal.acquire.v1")
                  lease.value = acquired.value.lease;
                else
                  message.value = acquired.ok ? "Input lease unavailable." : acquired.error.message;
              })
        }
        onRelease$={
          lease.value === null
            ? undefined
            : $(async () => {
                const port = props.port;
                const currentLease = lease.value;
                const currentTerminal = terminal.value;
                if (port === undefined || currentLease === null || currentTerminal === null) return;
                const released = await port.command({
                  operation: "terminal.release.v1",
                  clientRequestId: workspaceRequestId("managed-release"),
                  projectId: props.projectId,
                  contextId: props.contextId,
                  terminalId: currentTerminal.terminalId,
                  leaseId: currentLease.leaseId,
                  expectedControlEpoch: currentLease.controlEpoch,
                });
                if (!released.ok) {
                  message.value = released.error.message;
                  return;
                }
                terminal.value = {
                  ...currentTerminal,
                  controlEpoch: DecimalSchema.parse(
                    (BigInt(currentLease.controlEpoch) + 1n).toString(),
                  ),
                };
                lease.value = null;
                message.value = "Input control returned.";
              })
        }
        onInput$={$(async (data) => {
          const port = props.port;
          const current = lease.value;
          if (port === undefined || current === null) return;
          const result = await port.command({
            operation: "terminal.input.v1",
            clientRequestId: workspaceRequestId("managed-input"),
            projectId: props.projectId,
            contextId: props.contextId,
            terminalId: current.terminalId,
            leaseId: current.leaseId,
            expectedControlEpoch: current.controlEpoch,
            data,
          });
          if (!result.ok) message.value = result.error.message;
        })}
        onResize$={$(async (columns, rows) => {
          const port = props.port;
          const current = lease.value;
          if (port === undefined || current === null) return;
          const result = await port.command({
            operation: "terminal.resize.v1",
            clientRequestId: workspaceRequestId("managed-resize"),
            projectId: props.projectId,
            contextId: props.contextId,
            terminalId: current.terminalId,
            leaseId: current.leaseId,
            expectedControlEpoch: current.controlEpoch,
            columns,
            rows,
          });
          if (!result.ok) message.value = result.error.message;
        })}
      />
      {message.value === null ? null : <p class="workspace-muted">{message.value}</p>}
    </div>
  );
});
