/**
 * Password-login shell for the explicit remote Quicklens profile.
 * @scope spec://org.vibevm.zap/lens/PROP-003#password-authentication
 */
import {
  component$,
  noSerialize,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
} from "@qwik.dev/core";

import type { QuicklensDataSource } from "../../cells/quicklens-model/index.ts";
import { QuicklensApp } from "../../cells/quicklens-ui/app.tsx";
import { WorkspaceApp } from "../../cells/workspace-ui/index.tsx";
import type { QuicklensWebClient } from "./web-gateway.ts";

export const QuicklensWebRoot = component$<{
  readonly client: NoSerialize<QuicklensWebClient>;
  readonly source: NoSerialize<QuicklensDataSource>;
}>((props) => {
  const client = props.client;
  const password = useSignal("");
  const authenticated = useSignal(false);
  const busy = useSignal(false);
  const message = useSignal<string | null>(null);
  const theme = useSignal<"light" | "dark">("light");
  useVisibleTask$(({ cleanup }) => {
    const client = props.client;
    if (client === undefined) return;
    theme.value = window.localStorage.getItem("quicklens.theme") === "dark" ? "dark" : "light";
    cleanup(
      client.onAuthenticationLost(() => {
        authenticated.value = false;
        message.value = "Your web session expired or was revoked. Sign in again.";
      }),
    );
  });
  if (!authenticated.value) {
    return (
      <main class={`web-login-shell theme-${theme.value}`}>
        <section class="web-login-card">
          <button
            class="theme-toggle web-login-theme-toggle"
            aria-label={`Switch to ${theme.value === "light" ? "dark" : "light"} theme`}
            onClick$={() => {
              theme.value = theme.value === "light" ? "dark" : "light";
              window.localStorage.setItem("quicklens.theme", theme.value);
            }}
          >
            {theme.value === "light" ? "◐" : "◑"}
          </button>
          <span class="brand-mark">Q</span>
          <p class="eyebrow">Secure web profile</p>
          <h1>Open Quicklens</h1>
          <p>
            Authenticate to the configured human UI scope. Agent and ZAP credentials stay local.
          </p>
          <label class="field-label" for="web-password">
            Password
          </label>
          <input
            id="web-password"
            type="password"
            autocomplete="current-password"
            value={password.value}
            onInput$={(_, element) => {
              password.value = element.value;
            }}
          />
          <button
            class="button primary"
            disabled={busy.value || password.value.length === 0}
            onClick$={async () => {
              const client = props.client;
              if (client === undefined) return;
              busy.value = true;
              const result = await client.login(password.value);
              password.value = "";
              busy.value = false;
              authenticated.value = result.ok;
              message.value = result.ok ? null : result.message;
            }}
          >
            {busy.value ? "Checking…" : "Sign in"}
          </button>
          {message.value === null ? null : <p class="web-login-error">{message.value}</p>}
        </section>
      </main>
    );
  }
  if (client?.workspace !== null && client?.workspace !== undefined) {
    return (
      <WorkspaceApp
        port={noSerialize(client.workspace)}
        headerActionLabel="Sign out"
        onHeaderAction$={async () => {
          await client.logout();
          authenticated.value = false;
        }}
      />
    );
  }
  return (
    <QuicklensApp
      source={props.source}
      headerActionLabel="Sign out"
      onHeaderAction$={async () => {
        const client = props.client;
        if (client === undefined) return;
        await client.logout();
        authenticated.value = false;
      }}
    />
  );
});
