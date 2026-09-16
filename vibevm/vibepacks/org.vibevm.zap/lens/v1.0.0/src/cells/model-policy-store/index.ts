/** Durable per-project/context model policy and pinned selection store. @scope spec://org.vibevm.zap/lens/PROP-008#policy-lifecycle */
import { SqliteModelPolicyStore } from "./store.ts";
import type { ModelPolicyStore, OpenModelPolicyStoreOptions } from "./types.ts";
import type { ModelPolicyStoreResult } from "./types.ts";

export type * from "./types.ts";
export { ModelPolicyStoreAccessSchema } from "./types.ts";

export function openModelPolicyStore(
  options: OpenModelPolicyStoreOptions,
): ModelPolicyStoreResult<ModelPolicyStore> {
  try {
    return { ok: true, value: new SqliteModelPolicyStore(options) };
  } catch {
    return {
      ok: false,
      error: { code: "storage_failure", message: "model policy store could not open" },
    };
  }
}
