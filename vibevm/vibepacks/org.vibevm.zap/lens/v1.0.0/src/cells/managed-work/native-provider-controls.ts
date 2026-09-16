/** Codex and OpenCode managed sidecar composition. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import type { ManagedProviderControlAdapter } from "./control.ts";
import { createCodexManagedControlAdapter } from "./codex-control.ts";
import { createOpenCodeManagedControlAdapter } from "./opencode-control.ts";

export function createNativeManagedProviderControlAdapters(input: {
  readonly directory: string;
}): readonly ManagedProviderControlAdapter[] {
  return [
    createCodexManagedControlAdapter({ directory: input.directory }),
    createOpenCodeManagedControlAdapter(),
  ];
}

export { createCodexManagedControlAdapter } from "./codex-control.ts";
export { createOpenCodeManagedControlAdapter } from "./opencode-control.ts";
export type {
  ManagedControlProcess,
  ManagedControlProcessFactory,
} from "./native-control-process.ts";
