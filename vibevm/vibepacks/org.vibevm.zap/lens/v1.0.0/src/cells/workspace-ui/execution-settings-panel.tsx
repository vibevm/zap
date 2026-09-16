/** Project-scoped model policy and execution catalog settings. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import {
  $,
  component$,
  noSerialize,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
} from "@qwik.dev/core";
import type {
  ExecutionCatalogResult,
  ExecutionCatalogSnapshot,
  ExecutionSelection,
  ExecutionSelectionRequest,
} from "../execution-catalog/index.ts";
import {
  createWorkspaceExecutionConfiguration,
  createWorkspaceExecutionConnection,
  previewWorkspaceExecutionCatalog,
  refreshWorkspaceExecutionUsage,
  readWorkspaceExecutionCatalog,
  readWorkspaceModelPolicy,
  readWorkspaceModelPolicyHistory,
  saveWorkspaceExecutionCatalog,
  type ExecutionCatalogWorkspaceView,
  type ModelPolicyWorkspaceView,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";
import { ExecutionCatalogPanel } from "./execution-catalog-panel.tsx";
import { ModelPolicyPanel } from "./model-policy-panel.tsx";
import { ScopedRequestFence } from "./request-fence.ts";

type PreviewRequest = Omit<ExecutionSelectionRequest, "requestedAt">;

export const ExecutionSettingsPanel = component$<{
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
}>((props) => {
  const modelPolicy = useSignal<ModelPolicyWorkspaceView | null>(null);
  const modelPolicyError = useSignal<string | null>(null);
  const catalog = useSignal<ExecutionCatalogWorkspaceView | null>(null);
  const catalogError = useSignal<string | null>(null);
  const fence = useSignal<NoSerialize<ScopedRequestFence>>(noSerialize(new ScopedRequestFence()));

  const load = $(async () => {
    const port = props.port;
    const activeFence = fence.value;
    if (port === undefined || activeFence === undefined) return;
    const token = activeFence.begin(props.projectId + "\u0000" + props.contextId);
    const [policy, history, catalogRead] = await Promise.all([
      readWorkspaceModelPolicy(port, props.projectId, props.contextId),
      readWorkspaceModelPolicyHistory(port, props.projectId, props.contextId),
      readWorkspaceExecutionCatalog(port, props.projectId, props.contextId),
    ]);
    if (!activeFence.isCurrent(token)) return;
    if (!policy.ok) {
      modelPolicy.value = null;
      modelPolicyError.value = policy.error.message;
    } else if (!history.ok) {
      modelPolicy.value = { policy: policy.value, versions: [], changes: [] };
      modelPolicyError.value = history.error.message;
    } else {
      modelPolicy.value = { policy: policy.value, ...history.value };
      modelPolicyError.value = null;
    }
    if (catalogRead.ok) {
      catalog.value = catalogRead.value;
      catalogError.value = null;
    } else {
      catalog.value = null;
      catalogError.value = catalogRead.error.message;
    }
  });

  useVisibleTask$(({ track, cleanup }) => {
    track(() => props.projectId + ":" + props.contextId);
    void load();
    cleanup(() => fence.value?.cancel());
  });

  const saveCatalog = $(async (draft: ExecutionCatalogSnapshot) => {
    const port = props.port;
    const current = catalog.value;
    if (port === undefined || current === null) return unavailable("catalog is not loaded");
    const saved = await saveWorkspaceExecutionCatalog(
      port,
      props.projectId,
      props.contextId,
      current.snapshot,
      draft,
    );
    if (!saved.ok) return unavailable(saved.error.message);
    catalog.value = { ...current, snapshot: saved.value };
    return { ok: true as const, value: saved.value };
  });
  const previewCatalog = $(async (request: PreviewRequest) => {
    const port = props.port;
    return port === undefined
      ? unavailableSelection("catalog client is unavailable")
      : previewWorkspaceExecutionCatalog(port, props.projectId, props.contextId, request);
  });
  const createConnection = $(async (bindingId: string) => {
    const port = props.port;
    const current = catalog.value;
    if (port === undefined || current === null) return unavailable("catalog is not loaded");
    const created = await createWorkspaceExecutionConnection(
      port,
      props.projectId,
      props.contextId,
      current.snapshot,
      bindingId,
    );
    if (!created.ok) return unavailable(created.error.message);
    catalog.value = { ...current, snapshot: created.value };
    return { ok: true as const, value: created.value };
  });
  const createConfiguration = $(async (connectionId: string, referenceId: string) => {
    const port = props.port;
    const current = catalog.value;
    if (port === undefined || current === null) return unavailable("catalog is not loaded");
    const created = await createWorkspaceExecutionConfiguration(
      port,
      props.projectId,
      props.contextId,
      current.snapshot,
      connectionId,
      referenceId,
    );
    if (!created.ok) return unavailable(created.error.message);
    catalog.value = { ...current, snapshot: created.value };
    return { ok: true as const, value: created.value };
  });
  const refreshUsage = $(async (connectionId: string) => {
    const port = props.port;
    const current = catalog.value;
    if (port === undefined || current === null) return unavailable("catalog is not loaded");
    const refreshed = await refreshWorkspaceExecutionUsage(
      port,
      props.projectId,
      props.contextId,
      current.snapshot,
      connectionId,
    );
    if (!refreshed.ok) return unavailable(refreshed.error.message);
    catalog.value = { ...current, snapshot: refreshed.value };
    return { ok: true as const, value: refreshed.value };
  });

  return (
    <>
      <ExecutionCatalogPanel
        snapshot={catalog.value?.snapshot ?? null}
        administrator={catalog.value?.administrator ?? false}
        unavailableReason={catalogError.value}
        availableBindings={catalog.value?.availableBindings ?? []}
        modelReferences={catalog.value?.modelReferences ?? []}
        onRefresh$={load}
        onSave$={saveCatalog}
        onCreateConnection$={createConnection}
        onCreateConfiguration$={createConfiguration}
        onRefreshUsage$={refreshUsage}
        onPreview$={previewCatalog}
      />
      <ModelPolicyPanel
        view={modelPolicy.value}
        unavailableReason={modelPolicyError.value}
        port={props.port}
        projectId={props.projectId}
        contextId={props.contextId}
        onSaved$={load}
      />
    </>
  );
});

function unavailable(message: string): ExecutionCatalogResult<ExecutionCatalogSnapshot> {
  return { ok: false, error: { code: "storage_failure", message } };
}
function unavailableSelection(message: string): ExecutionCatalogResult<ExecutionSelection> {
  return { ok: false, error: { code: "storage_failure", message } };
}
