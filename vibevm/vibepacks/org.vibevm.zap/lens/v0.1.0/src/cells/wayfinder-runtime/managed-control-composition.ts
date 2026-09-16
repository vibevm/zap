/** Managed provider control composition over the exact terminal lease.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import {
  createManagedControlRuntime,
  createManagedProviderControlAdapters,
  type ManagedProviderControlAdapter,
} from "../managed-work/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import { managedControlIoResult } from "./managed-control-io.ts";

export function createManagedControlComposition(input: {
  readonly terminals: ManagedTerminalServicePort;
  readonly directory: string;
  readonly adapters: readonly ManagedProviderControlAdapter[];
}) {
  return createManagedControlRuntime({
    adapters: [
      ...createManagedProviderControlAdapters({ directory: input.directory }),
      ...input.adapters,
    ],
    io: ({ access, terminalId, leaseId, controlEpoch }) => ({
      input: (data) =>
        managedControlIoResult(
          input.terminals.input(access, terminalId, leaseId, controlEpoch, data),
        ),
      interrupt: () =>
        managedControlIoResult(
          input.terminals.interrupt(access, terminalId, leaseId, controlEpoch),
        ),
      stop: () =>
        managedControlIoResult(input.terminals.stop(access, terminalId, leaseId, controlEpoch)),
    }),
  });
}
