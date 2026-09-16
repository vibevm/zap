/** Runtime configuration failure construction.
 * @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership
 */
import type { WayfinderResult } from "./types.ts";

export function invalidConfig(message: string): WayfinderResult<never> {
  return {
    ok: false,
    error: {
      code: "invalid_config",
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: ${message}`,
    },
  };
}
