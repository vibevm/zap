/** @scope spec://org.vibevm.zap/lens/PROP-007#primary-experience */
/** Public WorkspaceApp inputs shared by local, Electron and protected web shells. */
import type { NoSerialize, QRL } from "@qwik.dev/core";
import type { WorkspaceClientPort } from "../workspace-client/index.ts";
import type { ProductSetupPort } from "../workspace-model/index.ts";
import type { ProductDirectoryPicker } from "./product-setup.tsx";

export interface WorkspaceAppProps {
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly product?: NoSerialize<ProductSetupPort> | undefined;
  readonly directoryPicker?: NoSerialize<ProductDirectoryPicker> | undefined;
  readonly demoLabel?: string | undefined;
  readonly headerActionLabel?: string | undefined;
  readonly onHeaderAction$?: QRL<() => void | Promise<void>> | undefined;
}
