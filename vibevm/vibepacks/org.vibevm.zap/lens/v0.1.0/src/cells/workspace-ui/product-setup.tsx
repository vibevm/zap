/** Ordinary project setup UI. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import {
  $,
  component$,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import { workspaceRequestId, type WorkspaceClientPort } from "../workspace-client/index.ts";
import type {
  ProductProject,
  ProductProviderProfile,
  ProductSetupPort,
  ProductSetupSnapshot,
} from "../workspace-model/index.ts";

export interface ProductDirectoryPicker {
  chooseDirectory(): Promise<string | null>;
}

export interface ProductSetupProps {
  readonly product: NoSerialize<ProductSetupPort>;
  readonly workspace: NoSerialize<WorkspaceClientPort>;
  readonly directoryPicker?: NoSerialize<ProductDirectoryPicker> | undefined;
  readonly canClose: boolean;
  readonly onRegistered$: QRL<(project: ProductProject) => void>;
  readonly onOpen$: QRL<(project: ProductProject) => void>;
  readonly onClose$: QRL<() => void>;
}

export const ProductSetup = component$<ProductSetupProps>((props) => {
  const snapshot = useSignal<ProductSetupSnapshot | null>(null);
  const directoryPath = useSignal("");
  const displayName = useSignal("");
  const profileId = useSignal("");
  const registered = useSignal<ProductProject | null>(null);
  const busy = useSignal(false);
  const message = useSignal<string | null>(null);
  const load = $(async () => {
    const product = props.product;
    if (product === undefined) return;
    const result = await product.request({ operation: "product.setup.get.v1" });
    if (!result.ok || result.value.operation !== "product.setup.get.v1") {
      message.value = result.ok
        ? "Product setup returned an unexpected response."
        : result.error.message;
      return;
    }
    snapshot.value = result.value.snapshot;
    const first = availableProfiles(result.value.snapshot.providers)[0];
    if (profileId.value === "" && first !== undefined) profileId.value = first.profileId;
  });
  useVisibleTask$(() => {
    void load();
  });
  const selectedProfile = (): ProductProviderProfile | undefined =>
    snapshot.value?.providers.find((profile) => profile.profileId === profileId.value);
  return (
    <section class="workspace-panel product-setup" aria-labelledby="product-setup-title">
      <div class="workspace-section-heading">
        <div>
          <p class="eyebrow">Project setup</p>
          <h1 id="product-setup-title">
            {snapshot.value?.projects.length === 0
              ? "Add your first project"
              : "Add another project"}
          </h1>
        </div>
        {props.canClose ? (
          <button class="button secondary" onClick$={props.onClose$}>
            Close
          </button>
        ) : null}
      </div>
      <p>
        Choose an existing directory and a protected local agent profile. Adding the project does
        not start the agent.
      </p>
      {snapshot.value === null ? (
        <p class="workspace-muted">Loading local setup…</p>
      ) : snapshot.value.providers.length === 0 ? (
        <div class="workspace-notice">
          No agent profile is configured. Add a protected provider profile in advanced Wayfinder
          configuration, then refresh setup.
        </div>
      ) : (
        <>
          <label class="field-label" for="product-directory">
            Existing project directory
          </label>
          <div class="product-directory-row">
            <input
              id="product-directory"
              value={directoryPath.value}
              placeholder="C:\\work\\my-project"
              onInput$={(_, element) => (directoryPath.value = element.value)}
            />
            {props.directoryPicker === undefined ? null : (
              <button
                class="button secondary"
                onClick$={async () => {
                  const chosen = await props.directoryPicker?.chooseDirectory();
                  if (chosen !== null && chosen !== undefined) directoryPath.value = chosen;
                }}
              >
                Browse…
              </button>
            )}
          </div>
          <label class="field-label" for="product-name">
            Project name (optional)
          </label>
          <input
            id="product-name"
            value={displayName.value}
            placeholder="Uses the directory name"
            onInput$={(_, element) => (displayName.value = element.value)}
          />
          <label class="field-label" for="product-profile">
            Agent profile
          </label>
          <select
            id="product-profile"
            value={profileId.value}
            onChange$={(_, element) => (profileId.value = element.value)}
          >
            {snapshot.value.providers.map((profile) => (
              <option key={profile.profileId} value={profile.profileId} disabled={!usable(profile)}>
                {`${profile.displayName} · ${providerState(profile)}`}
              </option>
            ))}
          </select>
          {selectedProfile() === undefined ? null : (
            <p class="workspace-muted">
              Model: {selectedProfile()?.modelId} · effort:{" "}
              {selectedProfile()?.effort ?? "provider default"}
            </p>
          )}
          <button
            class="button primary"
            disabled={busy.value || directoryPath.value.trim() === "" || profileId.value === ""}
            onClick$={async () => {
              const product = props.product;
              if (product === undefined) return;
              busy.value = true;
              message.value = "Validating and registering the project…";
              const result = await product.request({
                operation: "product.project.register.v1",
                clientRequestId: workspaceRequestId("project-register"),
                directoryPath: directoryPath.value.trim(),
                displayName: displayName.value.trim() === "" ? null : displayName.value.trim(),
                profileId: profileId.value,
              });
              busy.value = false;
              if (!result.ok || result.value.operation !== "product.project.register.v1") {
                message.value = result.ok
                  ? "Registration returned an unexpected response."
                  : result.error.message;
                return;
              }
              registered.value = result.value.project;
              message.value = "Project added. No agent has been started.";
              await props.onRegistered$(result.value.project);
            }}
          >
            {busy.value ? "Adding…" : "Add project"}
          </button>
        </>
      )}
      {message.value === null ? null : (
        <p class="workspace-notice" role="status">
          {message.value}
        </p>
      )}
      {registered.value === null ? null : (
        <div class="product-start-card">
          <strong>{registered.value.displayName} is ready</strong>
          <span>Starting development is a separate action and may launch the selected agent.</span>
          <div class="product-start-actions">
            <button
              class="button secondary"
              onClick$={() => {
                const project = registered.value;
                if (project !== null) void props.onOpen$(project);
              }}
            >
              Open without starting
            </button>
            <button
              class="button primary"
              disabled={busy.value}
              onClick$={async () => {
                const workspace = props.workspace;
                const project = registered.value;
                const profile = selectedProfile();
                if (workspace === undefined || project === null || profile === undefined) return;
                busy.value = true;
                const result = await workspace.command({
                  operation: "session.start.v1",
                  clientRequestId: workspaceRequestId("start-development"),
                  projectId: project.projectId,
                  contextId: project.contextId,
                  profileId: profile.profileId,
                  interactionKind: profile.interactionKind,
                });
                busy.value = false;
                message.value = result.ok ? "Agent start was accepted." : result.error.message;
                if (result.ok) await props.onOpen$(project);
              }}
            >
              {busy.value ? "Starting…" : "Start development"}
            </button>
          </div>
        </div>
      )}
    </section>
  );
});

function availableProfiles(profiles: readonly ProductProviderProfile[]): ProductProviderProfile[] {
  return profiles.filter(usable);
}

function usable(profile: ProductProviderProfile): boolean {
  return profile.installed && profile.configured && profile.launchable;
}

function providerState(profile: ProductProviderProfile): string {
  if (!profile.installed) return "not installed";
  if (!profile.configured) return "setup required";
  if (!profile.launchable) return "unavailable";
  return profile.authenticated === "observed" ? "ready" : "authentication not observed";
}
