/** Coordinator lifecycle controls. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import { component$, type QRL } from "@qwik.dev/core";

import type {
  CoordinatorLaunchOption,
  CoordinatorSession,
  ProjectExecutionState,
} from "../workspace-model/index.ts";

export const CoordinatorControls = component$<{
  readonly coordinator: CoordinatorSession | null;
  readonly sessions: readonly CoordinatorSession[];
  readonly launchOptions: readonly CoordinatorLaunchOption[];
  readonly starting: boolean;
  readonly message: string | null;
  readonly onStart$: QRL<(option: CoordinatorLaunchOption) => void>;
  readonly execution: ProjectExecutionState;
  readonly onLifecycle$: QRL<(action: "pause" | "stop" | "continue") => void>;
}>((props) => (
  <section class="workspace-panel coordinator-controls">
    <div class="workspace-panel-heading">
      <div>
        <p class="eyebrow">Project coordinator</p>
        <h2>{props.coordinator === null ? "Start development" : "Coordinator session"}</h2>
      </div>
      <span class={`coordinator-state state-${props.coordinator?.state ?? "stopped"}`}>
        {props.coordinator?.state.replaceAll("_", " ") ?? "not started"}
      </span>
    </div>
    {props.coordinator === null ? (
      <>
        <p>
          Start a configured coordinator for this project. Zap Quick Lens requests the launch; the
          trusted host adapter performs the project’s full boot.
        </p>
        <div class="launch-options">
          {props.launchOptions.length === 0 ? (
            <div class="workspace-empty compact">No coordinator launch profile is configured.</div>
          ) : (
            props.launchOptions.map((option) => (
              <div class="launch-option" key={option.profileId}>
                <div>
                  <strong>{option.label}</strong>
                  <small>{interactionLabel(option.interactionKind)}</small>
                </div>
                <button
                  class="button primary"
                  disabled={props.starting || option.availability.state === "unavailable"}
                  title={
                    option.availability.state === "unavailable"
                      ? option.availability.reason
                      : "Start this coordinator profile"
                  }
                  onClick$={() => props.onStart$(option)}
                >
                  {props.starting ? "Starting…" : "Start coordinator"}
                </button>
                {option.availability.state === "unavailable" ? (
                  <small class="workspace-muted">{option.availability.reason}</small>
                ) : null}
              </div>
            ))
          )}
        </div>
      </>
    ) : (
      <dl class="coordinator-facts">
        <div>
          <dt>Launch</dt>
          <dd>
            {props.coordinator.launchOrigin === "lens"
              ? "Zap Wayfinder supervised"
              : "User started"}
          </dd>
        </div>
        <div>
          <dt>Interaction</dt>
          <dd>{interactionLabel(props.coordinator.interactionKind)}</dd>
        </div>
        <div>
          <dt>Boot basis</dt>
          <dd>{props.coordinator.bootstrapBasis ?? "Not reported"}</dd>
        </div>
      </dl>
    )}
    {props.message === null ? null : <p class="operation-message">{props.message}</p>}
    <div class="execution-controls">
      <div>
        <strong>Project execution</strong>
        <span class={`coordinator-state state-${props.execution.state}`}>
          {props.execution.state.replaceAll("_", " ")}
        </span>
      </div>
      {props.execution.pendingAction === null ? null : (
        <small class="workspace-muted">
          {props.execution.pendingAction.action} requested ·{" "}
          {props.execution.pendingAction.observation}
        </small>
      )}
      <div class="execution-actions">
        {props.execution.state === "running" || props.execution.state === "continuing" ? (
          <button class="button secondary" onClick$={() => props.onLifecycle$("pause")}>
            Pause project
          </button>
        ) : null}
        {props.execution.state === "paused" || props.execution.state === "stopped" ? (
          <button class="button secondary" onClick$={() => props.onLifecycle$("continue")}>
            Continue project
          </button>
        ) : null}
        {props.execution.state !== "stopped" && props.execution.state !== "uninitialized" ? (
          <button class="button secondary" onClick$={() => props.onLifecycle$("stop")}>
            Stop project
          </button>
        ) : null}
      </div>
    </div>
    <div class="terminal-capability-slot">
      <strong>Managed terminal</strong>
      {props.coordinator?.terminal.state === "available" ? (
        <span>Available through a separate terminal-control capability.</span>
      ) : (
        <span>
          {props.coordinator?.terminal.state === "unavailable"
            ? terminalReason(props.coordinator.terminal.reason)
            : "No active session exposes an owned terminal."}
        </span>
      )}
    </div>
    {props.sessions.length <= 1 ? null : (
      <p class="workspace-muted">
        {props.sessions.length} recorded coordinator sessions in this context.
      </p>
    )}
  </section>
));

function interactionLabel(value: CoordinatorSession["interactionKind"]): string {
  if (value === "native_harness") return "Native host harness";
  if (value === "owned_terminal") return "Owned managed terminal";
  return "Structured host session";
}

function terminalReason(
  value: "native_session" | "host_unsupported" | "policy_disabled" | "not_owned",
) {
  switch (value) {
    case "native_session":
      return "Native session · public output is available, no terminal is claimed.";
    case "host_unsupported":
      return "This host does not provide an owned terminal.";
    case "policy_disabled":
      return "Managed terminals are disabled by availability policy.";
    case "not_owned":
      return "Zap Quick Lens does not own this session’s terminal.";
  }
}
