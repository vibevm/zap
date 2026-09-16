/** Fresh-install execution catalog administration. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import {
  $,
  component$,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import type {
  ExecutionBindingChoice,
  ExecutionCatalogSnapshot,
  ExecutionModelReferenceView,
} from "../execution-catalog/index.ts";
import {
  createProductExecutionConfiguration,
  createProductExecutionConnection,
  readProductExecutionCatalog,
  refreshProductExecutionUsage,
  saveProductExecutionCatalog,
} from "../workspace-client/index.ts";
import type { ProductSetupPort } from "../workspace-model/index.ts";
import { ExecutionCatalogPanel } from "./execution-catalog-panel.tsx";

export const ProductExecutionCatalogAdmin = component$<{
  readonly product: NoSerialize<ProductSetupPort>;
  readonly onChanged$?: QRL<() => void>;
}>((props) => {
  const snapshot = useSignal<ExecutionCatalogSnapshot | null>(null);
  const administrator = useSignal(false);
  const availableBindings = useSignal<readonly ExecutionBindingChoice[]>([]);
  const modelReferences = useSignal<readonly ExecutionModelReferenceView[]>([]);
  const unavailableReason = useSignal<string | null>(null);

  const load = $(async () => {
    const product = props.product;
    if (product === undefined) return;
    const result = await readProductExecutionCatalog(product);
    if (!result.ok) {
      unavailableReason.value = result.error.message;
      return;
    }
    administrator.value = result.value.administrator;
    snapshot.value = result.value.snapshot;
    availableBindings.value = result.value.availableBindings;
    modelReferences.value = result.value.modelReferences;
    unavailableReason.value = null;
  });

  useVisibleTask$(() => {
    void load();
  });

  return (
    <ExecutionCatalogPanel
      snapshot={snapshot.value}
      administrator={administrator.value}
      unavailableReason={unavailableReason.value}
      availableBindings={availableBindings.value}
      modelReferences={modelReferences.value}
      onRefresh$={load}
      onSave$={$(async (draft) => {
        const product = props.product;
        const current = snapshot.value;
        if (product === undefined || current === null)
          return unavailable("The global execution catalog is not loaded.");
        const result = await saveProductExecutionCatalog(product, current, draft);
        if (result.ok) {
          snapshot.value = result.value;
          await props.onChanged$?.();
        }
        return result;
      })}
      onCreateConnection$={$(async (bindingId) => {
        const product = props.product;
        const current = snapshot.value;
        if (product === undefined || current === null)
          return unavailable("The global execution catalog is not loaded.");
        const result = await createProductExecutionConnection(product, current, bindingId);
        if (result.ok) {
          snapshot.value = result.value;
          await props.onChanged$?.();
        }
        return result;
      })}
      onCreateConfiguration$={$(async (connectionId, referenceId) => {
        const product = props.product;
        const current = snapshot.value;
        if (product === undefined || current === null)
          return unavailable("The global execution catalog is not loaded.");
        const result = await createProductExecutionConfiguration(
          product,
          current,
          connectionId,
          referenceId,
        );
        if (result.ok) {
          snapshot.value = result.value;
          await props.onChanged$?.();
        }
        return result;
      })}
      onRefreshUsage$={$(async (connectionId) => {
        const product = props.product;
        const current = snapshot.value;
        if (product === undefined || current === null)
          return unavailable("The global execution catalog is not loaded.");
        const result = await refreshProductExecutionUsage(product, current, connectionId);
        if (result.ok) {
          snapshot.value = result.value;
          await props.onChanged$?.();
        }
        return result;
      })}
    />
  );
});

function unavailable(message: string) {
  return {
    ok: false as const,
    error: { code: "closed" as const, message },
  };
}
