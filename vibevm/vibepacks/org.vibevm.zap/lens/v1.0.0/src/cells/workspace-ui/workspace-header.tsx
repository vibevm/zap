/** @scope spec://org.vibevm.zap/lens/PROP-005#project-views */
/** Shared workspace heading, context selector and theme control. */
import { component$, type QRL } from "@qwik.dev/core";
import type { WorkContextDescriptor, WorkContextId } from "../workspace-model/index.ts";

export const WorkspaceHeader = component$<{
  readonly demoLabel?: string | undefined;
  readonly projectName?: string | undefined;
  readonly contexts: readonly WorkContextDescriptor[];
  readonly selectedContextId: WorkContextId | null;
  readonly theme: "light" | "dark";
  readonly headerActionLabel?: string | undefined;
  readonly onHeaderAction$?: QRL<() => void | Promise<void>> | undefined;
  readonly onContext$: QRL<(contextId: string) => void>;
  readonly onTheme$: QRL<() => void>;
}>((props) => (
  <header class="workspace-header">
    <div class="workspace-brand">
      <span class="brand-mark">L</span>
      <div>
        <strong>Zap Quick Lens</strong>
        <span>Multi-project planning and agent coordination</span>
      </div>
    </div>
    <div class="workspace-header-context">
      {props.demoLabel === undefined ? null : <span class="source-mode source-demo">demo</span>}
      <strong>{props.demoLabel ?? props.projectName ?? "Shared workspace"}</strong>
      {props.contexts.length < 2 ? null : (
        <select
          aria-label="Work context"
          value={props.selectedContextId ?? ""}
          onChange$={(_, element) => props.onContext$(element.value)}
        >
          {props.contexts.map((context) => (
            <option
              key={context.contextId}
              value={context.contextId}
              selected={context.contextId === props.selectedContextId}
            >
              {context.displayName}
            </option>
          ))}
        </select>
      )}
    </div>
    {props.headerActionLabel === undefined || props.onHeaderAction$ === undefined ? null : (
      <button class="header-session-action" onClick$={props.onHeaderAction$}>
        {props.headerActionLabel}
      </button>
    )}
    <button
      class="theme-toggle"
      aria-label={`Switch to ${props.theme === "light" ? "dark" : "light"} theme`}
      onClick$={props.onTheme$}
    >
      {props.theme === "light" ? "◐" : "◑"}
    </button>
  </header>
));
