/** Stable local agent identifier digest. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { createHash } from "node:crypto";

export function agentDigest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
