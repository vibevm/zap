/** Safe owned-provider process diagnostics. @scope spec://org.vibevm.zap/lens/PROP-006#provider-adapters */
import { createHash } from "node:crypto";

const MAX_DIAGNOSTIC_BYTES = 65_536;

export interface ProviderProcessDiagnostic {
  readonly channel: "stderr";
  readonly category: "auth" | "network" | "rate_limit" | "api" | "configuration" | "other";
  readonly digest: string;
  readonly bytes: number;
  readonly truncated: boolean;
}

export function providerStderrDiagnostic(chunk: Buffer | string): ProviderProcessDiagnostic {
  const source = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk, "utf8");
  const bounded = source.subarray(0, MAX_DIAGNOSTIC_BYTES);
  return {
    channel: "stderr",
    category: diagnosticCategory(bounded.toString("utf8")),
    digest: createHash("sha256").update(bounded).digest("hex"),
    bytes: bounded.byteLength,
    truncated: source.byteLength > bounded.byteLength,
  };
}

function diagnosticCategory(text: string): ProviderProcessDiagnostic["category"] {
  if (/\b(?:429|rate[ -]?limit|too many requests)\b/iu.test(text)) return "rate_limit";
  if (/\b(?:401|403|auth|oauth|credential|api[ _-]?key|unauthori[sz]ed|forbidden)\b/iu.test(text))
    return "auth";
  if (
    /\b(?:proxy|connect|connection|dns|socket|econn\w*|tls|certificate|network|timed? ?out)\b/iu.test(
      text,
    )
  )
    return "network";
  if (/\b(?:config|configuration|settings?|mcp)\b/iu.test(text)) return "configuration";
  if (/\b(?:api|5\d\d|overload|service unavailable)\b/iu.test(text)) return "api";
  return "other";
}
