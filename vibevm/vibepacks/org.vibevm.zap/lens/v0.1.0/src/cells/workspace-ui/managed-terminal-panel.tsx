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
  readonly onStart$?: QRL<() => void> | undefined;
  readonly onAcquire$?: QRL<() => void> | undefined;
  readonly onRelease$?: QRL<() => void> | undefined;
  readonly onInput$?: QRL<(data: string) => void> | undefined;
  readonly onResize$?: QRL<(columns: number, rows: number) => void> | undefined;
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
        </div>
      </>
    )}
  </section>
));
