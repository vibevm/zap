/** Main project workspace navigation. @scope spec://org.vibevm.zap/lens/PROP-007#primary-experience */
import { component$, type QRL } from "@qwik.dev/core";

export type WorkspaceTab = "agents" | "chat" | "questions" | "workspaces" | "plan";

export const WorkspaceTabs = component$<{
  readonly selected: WorkspaceTab;
  readonly onSelect$: QRL<(tab: WorkspaceTab) => void>;
}>((props) => (
  <nav class="workspace-tabs" aria-label="Project workspace views">
    {(["agents", "chat", "questions", "workspaces", "plan"] as const).map((item) => (
      <button
        key={item}
        class={props.selected === item ? "selected" : ""}
        onClick$={() => props.onSelect$(item)}
      >
        {item === "agents"
          ? "Agents"
          : item === "workspaces"
            ? "Workspaces"
            : item.charAt(0).toUpperCase() + item.slice(1)}
      </button>
    ))}
  </nav>
));

export const PlanUnavailable = component$<{ readonly reason: string }>((props) => (
  <div class="workspace-panel workspace-empty">
    <strong>Plan view unavailable</strong>
    <span>{props.reason}</span>
  </div>
));
