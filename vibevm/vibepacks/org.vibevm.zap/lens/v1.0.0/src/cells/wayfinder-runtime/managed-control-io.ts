/** Maps terminal control results onto managed work control results.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import type { ManagedTerminalResult } from "../managed-terminal/index.ts";
import type { ManagedWorkResult } from "../managed-work/index.ts";

export function managedControlIoResult(
  result: ManagedTerminalResult<void>,
): ManagedWorkResult<void> {
  return result.ok
    ? result
    : {
        ok: false,
        error: {
          code: result.error.code === "unsupported" ? "unavailable" : result.error.code,
          message: result.error.message,
        },
      };
}
