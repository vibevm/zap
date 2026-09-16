/** @scope spec://org.vibevm.zap/lens/PROP-003#web-sessions */
import { createHash, timingSafeEqual } from "node:crypto";
import type { IncomingMessage, ServerResponse } from "node:http";

export interface WebSecurityConfig {
  readonly publicOrigin: string;
  readonly publicHost: string;
  readonly proxyProofToken: string;
  readonly clock: () => number;
}

export function trustedProxyRequest(
  request: IncomingMessage,
  config: WebSecurityConfig,
  port: number | undefined,
): boolean {
  const proof = singleHeader(request, "x-quicklens-proxy-proof");
  const forwardedProto = singleHeader(request, "x-forwarded-proto");
  const forwardedHost = singleHeader(request, "x-forwarded-host");
  const host = singleHeader(request, "host");
  return (
    proof !== undefined &&
    safeEqual(proof, config.proxyProofToken) &&
    forwardedProto === "https" &&
    forwardedHost === config.publicHost &&
    port !== undefined &&
    (host === `127.0.0.1:${String(port)}` ||
      host === `localhost:${String(port)}` ||
      host === `[::1]:${String(port)}`)
  );
}

export function singleHeader(request: IncomingMessage, name: string): string | undefined {
  const values: string[] = [];
  for (let index = 0; index < request.rawHeaders.length; index += 2) {
    if (request.rawHeaders[index]?.toLowerCase() === name) {
      const value = request.rawHeaders[index + 1];
      if (value !== undefined) values.push(value);
    }
  }
  return values.length === 1 ? values[0] : undefined;
}

export function cookieValue(request: IncomingMessage): string | undefined {
  return singleHeader(request, "cookie")
    ?.split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith("__Host-qls="))
    ?.slice("__Host-qls=".length);
}

export function purge<T extends { readonly expiresAt: number; readonly authVersion: string }>(
  sessions: Map<string, T>,
  now: number,
  version: string,
): void {
  for (const [id, record] of sessions)
    if (record.expiresAt <= now || record.authVersion !== version) sessions.delete(id);
}

export function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
export function safeEqual(left: string, right: string): boolean {
  const a = Buffer.from(left);
  const b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}
export function loopback(host: string): boolean {
  return host === "127.0.0.1" || host === "localhost" || host === "::1";
}
export function secureHeaders(response: ServerResponse): void {
  response.setHeader(
    "Content-Security-Policy",
    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
  );
  response.setHeader("Strict-Transport-Security", "max-age=31536000; includeSubDomains");
  response.setHeader("X-Content-Type-Options", "nosniff");
  response.setHeader("X-Frame-Options", "DENY");
  response.setHeader("Referrer-Policy", "no-referrer");
  response.setHeader("Cache-Control", "no-store");
}
export function sendJson(response: ServerResponse, status: number, value: unknown): void {
  response.writeHead(status, { "Content-Type": "application/json; charset=utf-8" });
  response.end(JSON.stringify(value));
}
