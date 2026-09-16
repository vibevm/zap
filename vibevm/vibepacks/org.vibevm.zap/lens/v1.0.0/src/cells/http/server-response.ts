/** HTTP response framing helpers. @scope spec://org.vibevm.zap/lens/PROP-001#transport */
import { once } from "node:events";
import type { IncomingMessage, ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import type { Result } from "../protocol/index.ts";

export function singleHeader(request: IncomingMessage, name: string): string | undefined {
  const values: string[] = [];
  for (let index = 0; index < request.rawHeaders.length; index += 2) {
    const headerName = request.rawHeaders[index];
    const headerValue = request.rawHeaders[index + 1];
    if (headerName?.toLowerCase() === name && headerValue !== undefined) values.push(headerValue);
  }
  return values.length === 1 ? values[0] : undefined;
}

export function sendResult<T>(
  response: ServerResponse,
  result: Result<T>,
  status: number,
  request: IncomingMessage,
): void {
  sendJson(response, status, { protocol: "lens/1", ...result }, request);
}

export function sendJson(
  response: ServerResponse,
  status: number,
  value: unknown,
  request: IncomingMessage,
): void {
  writeCors(response, request);
  response.writeHead(status, {
    "Content-Type": "application/json",
    "Cache-Control": "no-store",
  });
  response.end(JSON.stringify(value));
}

export function writeCors(response: ServerResponse, request: IncomingMessage): void {
  const origin = singleHeader(request, "origin");
  if (origin !== undefined) {
    response.setHeader("Access-Control-Allow-Origin", origin);
    response.setHeader("Vary", "Origin");
  }
}

export function addressInfo(value: AddressInfo): { readonly host: string; readonly port: number } {
  return { host: value.address, port: value.port };
}

export async function writeSse(
  response: ServerResponse,
  value: string,
  signal: AbortSignal,
): Promise<boolean> {
  if (signal.aborted || response.destroyed) return false;
  if (response.write(value)) return true;
  await Promise.race([once(response, "drain"), once(signal, "abort")]);
  return !signal.aborted;
}
