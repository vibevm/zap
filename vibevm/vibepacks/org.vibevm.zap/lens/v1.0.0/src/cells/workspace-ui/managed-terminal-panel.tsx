/** @scope spec://org.vibevm.zap/lens/PROP-006#shared-clients */
/** Capability-aware managed terminal presentation. Native sessions stay visibly terminal-free. */
import { component$, type QRL } from "@qwik.dev/core";

import type { TerminalCapability } from "../workspace-model/index.ts";
import { XtermTerminal } from "./xterm-terminal.tsx";

interface PublicTerminalOutput {
  readonly data: string;
}

export const ManagedTerminalPanel = component$<{
  readonly capability: TerminalCapability;
  readonly output: readonly PublicTerminalOutput[];
  readonly profileLabel?: string | undefined;
  readonly terminalLabel?: string | undefined;
  readonly terminalState?: string | undefined;
  readonly connectionState?: "connected" | "reconnecting" | undefined;
  readonly gapMessage?: string | null | undefined;
  readonly outputError?: string | null | undefined;
  readonly onStart$?: QRL<() => void> | undefined;
  readonly onAcquire$?: QRL<() => void> | undefined;
  readonly onRelease$?: QRL<() => void> | undefined;
  readonly onInput$?: QRL<(data: string) => void> | undefined;
  readonly onResize$?: QRL<(columns: number, rows: number) => void> | undefined;
  readonly onInterrupt$?: QRL<() => void> | undefined;
  readonly onStop$?: QRL<() => void> | undefined;
}>((props) => (
  <section class="workspace-panel managed-terminal-panel">
    <div class="workspace-panel-heading">
      <div>
        <p class="eyebrow">Managed terminal</p>
        <h2>
          {props.capability.state === "available"
            ? "Owned terminal output"
            : props.onStart$ === undefined
              ? "Unavailable"
              : "Ready to start"}
        </h2>
      </div>
      {props.capability.state === "available" ? (
        <span class="agent-mode-chip">
          {props.capability.inputEnabled ? "controller" : "read-only"}
        </span>
      ) : null}
      {props.onStart$ === undefined ? null : (
        <button class="button secondary" onClick$={props.onStart$}>
          Start {props.profileLabel ?? "managed worker"}
        </button>
      )}
    </div>
    {props.capability.state === "available" ? (
      <div class="managed-terminal-facts">
        <span>{props.terminalLabel ?? "Managed terminal"}</span>
        <span>{props.terminalState?.replaceAll("_", " ") ?? "unknown state"}</span>
        <span>{props.connectionState ?? "connected"}</span>
      </div>
    ) : null}
    {props.gapMessage === null || props.gapMessage === undefined ? null : (
      <p class="workspace-notice">{props.gapMessage}</p>
    )}
    {props.outputError === null || props.outputError === undefined ? null : (
      <p class="workspace-notice">{props.outputError}</p>
    )}
    {props.capability.state !== "available" ? (
      <p class="workspace-muted">
        {props.onStart$ === undefined
          ? "This session does not expose a managed terminal."
          : "Start the selected registered profile to open its managed terminal."}
      </p>
    ) : (
      <>
        <XtermTerminal
          enabled
          output={props.output.map((item) => item.data).join("")}
          inputEnabled={props.capability.inputEnabled && props.onRelease$ !== undefined}
          onInput$={props.onInput$}
          onResize$={props.onResize$}
        />
        <div class="execution-actions">
          {props.onAcquire$ === undefined ? null : (
            <button class="button secondary" onClick$={props.onAcquire$}>
              Take control
            </button>
          )}
          {props.onRelease$ === undefined ? null : (
            <button class="button secondary" onClick$={props.onRelease$}>
              Return control
            </button>
          )}
          {props.onInterrupt$ === undefined ? null : (
            <button class="button secondary" onClick$={props.onInterrupt$}>
              Interrupt
            </button>
          )}
          {props.onStop$ === undefined ? null : (
            <button class="button secondary danger" onClick$={props.onStop$}>
              Stop
            </button>
          )}
        </div>
      </>
    )}
  </section>
));
