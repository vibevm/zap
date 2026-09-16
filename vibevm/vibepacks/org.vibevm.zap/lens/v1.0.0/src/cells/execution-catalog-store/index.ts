/** Durable execution catalog store. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import type { ExecutionCatalogResult } from "../execution-catalog/index.ts";
import { SqliteExecutionCatalogStore } from "./store.ts";
import type { ExecutionCatalogStore, OpenExecutionCatalogStoreOptions } from "./types.ts";

export type * from "./types.ts";
export { ExecutionCatalogStoreAccessSchema } from "./types.ts";

export function openExecutionCatalogStore(
  options: OpenExecutionCatalogStoreOptions,
): ExecutionCatalogResult<ExecutionCatalogStore> {
  try {
    return { ok: true, value: new SqliteExecutionCatalogStore(options) };
  } catch {
    return {
      ok: false,
      error: { code: "storage_failure", message: "execution catalog store could not open" },
    };
  }
}
