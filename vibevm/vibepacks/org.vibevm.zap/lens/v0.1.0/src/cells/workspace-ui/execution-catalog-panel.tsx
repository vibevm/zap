/** Shared execution catalog administration UI. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import { $, component$, useSignal, useTask$, type QRL } from "@qwik.dev/core";
import {
  ExecutionCatalogSnapshotSchema,
  TaskSpecializationSchema,
  type ExecutionCatalogResult,
  type ExecutionCatalogSnapshot,
  type ExecutionBindingChoice,
  type ExecutionConnectionRecord,
  type ExecutionConfigurationRecord,
  type ExecutionModelReferenceView,
  type ExecutionSelection,
  type ExecutionSelectionRequest,
} from "../execution-catalog/index.ts";
import { TaskClassSchema, TaskPurposeSchema } from "../model-policy/index.ts";
import { ConfigurationEditor, ConnectionEditor } from "./execution-catalog-fields.tsx";
import {
  ExecutionReferenceTable,
  preferredExecutionReference,
} from "./execution-catalog-reference.tsx";
import "./execution-catalog.css";

type PreviewRequest = Omit<ExecutionSelectionRequest, "requestedAt">;

export const ExecutionCatalogPanel = component$<{
  readonly snapshot: ExecutionCatalogSnapshot | null;
  readonly administrator: boolean;
  readonly unavailableReason: string | null;
  readonly availableBindings: readonly ExecutionBindingChoice[];
  readonly modelReferences: readonly ExecutionModelReferenceView[];
  readonly onRefresh$: QRL<() => Promise<void>>;
  readonly onSave$: QRL<
    (
      snapshot: ExecutionCatalogSnapshot,
    ) => Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>>
  >;
  readonly onCreateConnection$?: QRL<
    (bindingId: string) => Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>>
  >;
  readonly onCreateConfiguration$?: QRL<
    (
      connectionId: string,
      referenceId: string,
    ) => Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>>
  >;
  readonly onRefreshUsage$?: QRL<
    (connectionId: string) => Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>>
  >;
  readonly onPreview$?: QRL<
    (request: PreviewRequest) => Promise<ExecutionCatalogResult<ExecutionSelection>>
  >;
}>((props) => {
  const draft = useSignal<ExecutionCatalogSnapshot | null>(null);
  const preview = useSignal<ExecutionSelection | null>(null);
  const selectedConnectionId = useSignal("");
  const selectedBindingId = useSignal("");
  const selectedReferenceId = useSignal("");
  const specialization = useSignal<(typeof TaskSpecializationSchema.options)[number]>("general");
  const purpose = useSignal<(typeof TaskPurposeSchema.options)[number]>(
    "development_implementation",
  );
  const taskClass = useSignal<(typeof TaskClassSchema.options)[number]>("change");
  const status = useSignal<string | null>(null);
  const busy = useSignal(false);
  const sampledAt = useSignal(new Date().toISOString());

  useTask$(({ track }) => {
    track(() => props.snapshot?.catalogRevision);
    track(() => props.snapshot?.preferencesRevision);
    draft.value =
      props.snapshot === null
        ? null
        : mergeDraft(ExecutionCatalogSnapshotSchema.parse(props.snapshot), draft.value);
    selectedConnectionId.value =
      props.snapshot?.connections.find((connection) => connection.enabled)?.connectionId ?? "";
    selectedBindingId.value =
      props.availableBindings.find((binding) => binding.enabled)?.bindingId ?? "";
    selectedReferenceId.value = preferredExecutionReference(
      props.snapshot?.connections.find((connection) => connection.enabled)?.agentProduct,
      props.modelReferences,
    );
    sampledAt.value = new Date().toISOString();
  });

  const changeConnection = $((value: ExecutionConnectionRecord) => {
    const current = draft.value;
    if (current === null) return;
    draft.value = {
      ...current,
      connections: current.connections.map((candidate) =>
        candidate.connectionId === value.connectionId ? value : candidate,
      ),
    };
  });
  const changeConfiguration = $((value: ExecutionConfigurationRecord) => {
    const current = draft.value;
    if (current === null) return;
    draft.value = {
      ...current,
      configurations: current.configurations.map((candidate) =>
        candidate.configurationId === value.configurationId ? value : candidate,
      ),
    };
  });
  const setEconomyQuality = $((value: number) => {
    const current = draft.value;
    if (current === null) return;
    draft.value = {
      ...current,
      preferences: { ...current.preferences, economyQuality: value },
    };
  });
  const setQuotaEnabled = $((enabled: boolean) => {
    const current = draft.value;
    if (current === null) return;
    draft.value = {
      ...current,
      preferences: {
        ...current.preferences,
        quota: {
          ...current.preferences.quota,
          deprioritizeLowRemaining: enabled,
          thresholdPercent: current.preferences.quota.thresholdPercent,
        },
      },
    };
  });
  const setQuotaThreshold = $((value: number) => {
    const current = draft.value;
    if (current === null) return;
    draft.value = {
      ...current,
      preferences: {
        ...current.preferences,
        quota: { ...current.preferences.quota, thresholdPercent: value },
      },
    };
  });

  const run = $(async (action: () => Promise<ExecutionCatalogResult<ExecutionCatalogSnapshot>>) => {
    busy.value = true;
    try {
      const result = await action();
      if (!result.ok) {
        await props.onRefresh$();
        status.value =
          result.error.message +
          " The current server state was refreshed; any unsaved fields remain in this form.";
        return;
      }
      draft.value = result.value;
      status.value = "Execution catalog saved.";
    } finally {
      busy.value = false;
    }
  });
  const refreshUsage = $(async (connectionId: string) => {
    const action = props.onRefreshUsage$;
    if (action === undefined) return;
    busy.value = true;
    try {
      const result = await action(connectionId);
      if (!result.ok) {
        status.value = result.error.message;
        return;
      }
      draft.value = mergeDraft(result.value, draft.value);
      status.value = "Usage observations refreshed; unsaved catalog fields were retained.";
    } finally {
      busy.value = false;
    }
  });

  if (draft.value === null) {
    return (
      <section class="workspace-panel workspace-empty">
        <strong>Execution catalog unavailable</strong>
        <span>{props.unavailableReason ?? "The shared execution catalog is not configured."}</span>
        <button class="button secondary" onClick$={props.onRefresh$}>
          Refresh
        </button>
      </section>
    );
  }
  const catalog = draft.value;
  return (
    <details class="workspace-panel execution-catalog-panel" open>
      <summary>Accounts, agents and task routing</summary>
      <div class="workspace-section-heading">
        <div>
          <p class="eyebrow">Execution catalog</p>
          <h2>Allowed execution choices</h2>
          <p>Name first, then agent, account, model, effort and context.</p>
        </div>
        <div class="execution-actions">
          <span class="count-chip">rev {catalog.catalogRevision}</span>
          <button class="button secondary" onClick$={props.onRefresh$}>
            Refresh
          </button>
          <button
            class="button primary"
            disabled={!props.administrator || busy.value}
            onClick$={() => run(() => props.onSave$(catalog))}
          >
            Save catalog
          </button>
        </div>
      </div>
      {!props.administrator ? (
        <p class="workspace-notice">
          Read-only view. Catalog changes require local owner administration authority.
        </p>
      ) : null}

      <section class="execution-preferences">
        <h3>Routing preference</h3>
        <label class="field-label">
          Economy ↔ Quality · {catalog.preferences.economyQuality}
          <input
            type="range"
            min={0}
            max={100}
            value={catalog.preferences.economyQuality}
            disabled={!props.administrator}
            onInput$={(_, element) => {
              void setEconomyQuality(Number(element.value));
            }}
          />
        </label>
        <label class="execution-toggle">
          <input
            type="checkbox"
            checked={catalog.preferences.quota.deprioritizeLowRemaining}
            disabled={!props.administrator}
            onChange$={(_, element) => {
              void setQuotaEnabled(element.checked);
            }}
          />
          Deprioritize accounts below the remaining-subscription threshold
        </label>
        <label class="field-label">
          Threshold · {catalog.preferences.quota.thresholdPercent}%
          <input
            type="number"
            min={0}
            max={100}
            value={catalog.preferences.quota.thresholdPercent}
            disabled={!props.administrator || !catalog.preferences.quota.deprioritizeLowRemaining}
            onInput$={(_, element) => {
              void setQuotaThreshold(Number(element.value));
            }}
          />
        </label>
        <p class="workspace-muted">
          Fresh low subscription is deprioritized, not forbidden. Stale, unknown or unsupported
          usage remains unknown and is never treated as 0%.
        </p>
      </section>

      <section>
        <div class="workspace-section-heading">
          <div>
            <h3>Account connections</h3>
            <p>Connections reference protected host bindings; credentials never enter this view.</p>
          </div>
          {props.onCreateConnection$ === undefined ? null : (
            <div class="execution-actions">
              <select
                aria-label="Protected binding for new account"
                value={selectedBindingId.value}
                disabled={!props.administrator}
                onChange$={(_, element) => (selectedBindingId.value = element.value)}
              >
                {props.availableBindings.map((binding) => (
                  <option
                    key={binding.bindingId}
                    value={binding.bindingId}
                    disabled={!binding.enabled}
                    selected={binding.bindingId === selectedBindingId.value}
                  >
                    {binding.displayName + " · " + binding.agentProduct.replaceAll("_", " ")}
                  </option>
                ))}
              </select>
              <button
                class="button secondary"
                disabled={!props.administrator || busy.value || selectedBindingId.value === ""}
                onClick$={() => {
                  const create = props.onCreateConnection$;
                  if (create !== undefined) void run(() => create(selectedBindingId.value));
                }}
              >
                Add account connection
              </button>
            </div>
          )}
        </div>
        {catalog.connections.length === 0 ? (
          <div class="workspace-empty compact">
            <strong>No authorized account connections</strong>
            <span>Add a protected binding to create the first account connection.</span>
          </div>
        ) : (
          <div class="execution-catalog-grid">
            {catalog.connections.map((connection) => (
              <ConnectionEditor
                key={connection.connectionId}
                connection={connection}
                usage={catalog.usage.filter(
                  (observation) => observation.connectionId === connection.connectionId,
                )}
                sampledAt={sampledAt.value}
                freshnessSeconds={catalog.preferences.quota.freshnessSeconds}
                canAdmin={props.administrator}
                refreshing={busy.value}
                onChange$={changeConnection}
                {...(props.onRefreshUsage$ === undefined
                  ? {}
                  : { onRefreshUsage$: $(() => refreshUsage(connection.connectionId)) })}
              />
            ))}
          </div>
        )}
      </section>

      <section>
        <div class="workspace-section-heading">
          <div>
            <h3>Named configurations</h3>
            <p>Each configuration authorizes one exact account, agent and model combination.</p>
          </div>
          <div class="execution-actions">
            <select
              aria-label="Connection for new configuration"
              value={selectedConnectionId.value}
              disabled={!props.administrator}
              onChange$={(_, element) => {
                selectedConnectionId.value = element.value;
                selectedReferenceId.value = preferredExecutionReference(
                  catalog.connections.find(
                    (connection) => connection.connectionId === element.value,
                  )?.agentProduct,
                  props.modelReferences,
                );
              }}
            >
              {catalog.connections.map((connection) => (
                <option
                  key={connection.connectionId}
                  value={connection.connectionId}
                  selected={connection.connectionId === selectedConnectionId.value}
                >
                  {connection.displayName}
                </option>
              ))}
            </select>
            <select
              aria-label="Model for new configuration"
              value={selectedReferenceId.value}
              disabled={!props.administrator}
              onChange$={(_, element) => (selectedReferenceId.value = element.value)}
            >
              {props.modelReferences.map((reference) => (
                <option
                  key={reference.referenceId}
                  value={reference.referenceId}
                  disabled={!reference.conversationModel}
                  selected={reference.referenceId === selectedReferenceId.value}
                >
                  {reference.modelId + " · " + reference.availability.replaceAll("_", " ")}
                </option>
              ))}
            </select>
            {props.onCreateConfiguration$ === undefined ? null : (
              <button
                class="button secondary"
                disabled={
                  !props.administrator ||
                  busy.value ||
                  selectedConnectionId.value === "" ||
                  selectedReferenceId.value === ""
                }
                onClick$={() => {
                  const create = props.onCreateConfiguration$;
                  if (create !== undefined)
                    void run(() => create(selectedConnectionId.value, selectedReferenceId.value));
                }}
              >
                Add configuration
              </button>
            )}
          </div>
        </div>
        {catalog.configurations.length === 0 ? (
          <div class="workspace-empty compact">
            <strong>No execution configurations</strong>
            <span>Select an authorized connection and add its first model configuration.</span>
          </div>
        ) : (
          <div class="execution-catalog-grid">
            {catalog.configurations.map((configuration) => {
              const connection = catalog.connections.find(
                (candidate) => candidate.connectionId === configuration.connectionId,
              );
              return (
                <ConfigurationEditor
                  key={configuration.configurationId}
                  configuration={configuration}
                  connectionName={connection?.displayName ?? "Missing connection"}
                  usage={catalog.usage.filter(
                    (observation) => observation.connectionId === configuration.connectionId,
                  )}
                  canAdmin={props.administrator}
                  onChange$={changeConfiguration}
                />
              );
            })}
          </div>
        )}
      </section>

      <ExecutionReferenceTable references={props.modelReferences} />

      <section class="execution-preview">
        <h3>Task routing preview</h3>
        <div class="execution-preview-fields">
          <SelectField
            label="Specialization"
            value={specialization.value}
            options={TaskSpecializationSchema.options}
            onChange$={$((value) => (specialization.value = TaskSpecializationSchema.parse(value)))}
          />
          <SelectField
            label="Purpose"
            value={purpose.value}
            options={TaskPurposeSchema.options}
            onChange$={$((value) => (purpose.value = TaskPurposeSchema.parse(value)))}
          />
          <SelectField
            label="Task class"
            value={taskClass.value}
            options={TaskClassSchema.options}
            onChange$={$((value) => (taskClass.value = TaskClassSchema.parse(value)))}
          />
        </div>
        <p class="workspace-muted">
          Task ref: preview.{specialization.value} · managed worker · catalog revisions{" "}
          {catalog.catalogRevision}/{catalog.preferencesRevision}
        </p>
        {props.onPreview$ === undefined ? (
          <p class="workspace-muted">Register or open a project to preview routed dispatch.</p>
        ) : (
          <button
            class="button secondary"
            disabled={busy.value || catalog.configurations.length === 0}
            onClick$={async () => {
              const productId = catalog.configurations[0]?.productId;
              const previewRequest = props.onPreview$;
              if (productId === undefined || previewRequest === undefined) return;
              const result = await previewRequest({
                selectionRef: "selection.catalog.preview",
                specialization: specialization.value,
                purpose: purpose.value,
                taskClass: taskClass.value,
                role: "worker",
                executionMode: "managed",
                invocationScope: "managed_agent",
                productId,
                productVersion: "catalog",
                requiredModalities:
                  specialization.value === "image_generation" ? ["text", "image_output"] : ["text"],
                effort: { mode: "unspecified" },
                context: { mode: "default" },
                override: null,
              });
              preview.value = result.ok ? result.value : null;
              status.value = result.ok ? "Preview resolved." : result.error.message;
            }}
          >
            Preview shared resolver
          </button>
        )}
        {preview.value === null ? null : <SelectionPreview selection={preview.value} />}
      </section>
      {status.value === null ? null : <p class="operation-message">{status.value}</p>}
    </details>
  );
});

const SelectionPreview = component$<{ readonly selection: ExecutionSelection }>((props) => (
  <div class="execution-preview-result">
    <strong>{props.selection.configurationName}</strong>
    <span>
      {props.selection.agentProduct} · {props.selection.providerId} · {props.selection.modelId}
    </span>
    <span>
      Effort {props.selection.appliedEffort.state} · Context {props.selection.appliedContext.state}
    </span>
    <details>
      <summary>Candidate explanations</summary>
      <ul>
        {props.selection.explanations.map((explanation) => (
          <li key={explanation.configurationId}>
            <strong>{explanation.configurationId}</strong> · {explanation.state} ·{" "}
            {explanation.score ?? "no score"} · {explanation.reasons.join("; ")}
          </li>
        ))}
      </ul>
    </details>
  </div>
));

function mergeDraft(
  latest: ExecutionCatalogSnapshot,
  current: ExecutionCatalogSnapshot | null,
): ExecutionCatalogSnapshot {
  if (current === null) return latest;
  const connections = new Map(current.connections.map((value) => [value.connectionId, value]));
  const configurations = new Map(
    current.configurations.map((value) => [value.configurationId, value]),
  );
  return {
    ...latest,
    connections: latest.connections.map((value) => {
      const draft = connections.get(value.connectionId);
      return draft === undefined
        ? value
        : { ...value, displayName: draft.displayName, enabled: draft.enabled };
    }),
    configurations: latest.configurations.map((value) => {
      const draft = configurations.get(value.configurationId);
      return draft === undefined
        ? value
        : {
            ...value,
            displayName: draft.displayName,
            enabled: draft.enabled,
            effort: draft.effort,
            context: draft.context,
            usageBucketIds: draft.usageBucketIds,
            scores: draft.scores,
          };
    }),
    preferences: current.preferences,
  };
}

const SelectField = component$<{
  readonly label: string;
  readonly value: string;
  readonly options: readonly string[];
  readonly onChange$: QRL<(value: string) => void>;
}>((props) => (
  <label class="field-label">
    {props.label}
    <select value={props.value} onChange$={(_, element) => props.onChange$(element.value)}>
      {props.options.map((option) => (
        <option key={option} value={option} selected={option === props.value}>
          {option.replaceAll("_", " ")}
        </option>
      ))}
    </select>
  </label>
));
