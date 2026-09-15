/** @scope spec://org.vibevm.zap/lens/PROP-007#primary-experience */
/** Public WorkspaceApp inputs shared by local, Electron and protected web shells. */
import type { NoSerialize, QRL } from "@qwik.dev/core";
import type { WorkspaceClientPort } from "../workspace-client/index.ts";

export interface WorkspaceAppProps {
  readonly port: NoSerialize<WorkspaceClientPort>;
  readonly demoLabel?: string | undefined;
  readonly headerActionLabel?: string | undefined;
  readonly onHeaderAction$?: QRL<() => void | Promise<void>> | undefined;
}
