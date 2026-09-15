/** Modest same-machine Wayfinder ownership and attach-ticket control. @scope spec://org.vibevm.zap/lens/PROP-005#simultaneous-clients */
import { randomBytes, randomUUID } from "node:crypto";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { createServer, type Server } from "node:http";
import { dirname } from "node:path";
import { z } from "zod";

const OwnerRecordSchema = z
  .object({
    ownerId: z.uuid(),
    pid: z.number().int().positive(),
    secret: z.string().min(32).max(256),
    controlPort: z.number().int().min(1).max(65_535).nullable(),
    gateway: z
      .object({
        host: z.string().min(1),
        port: z.number().int().min(1).max(65_535),
        basePath: z.string().min(1),
      })
      .strict()
      .nullable(),
  })
  .strict();
type OwnerRecord = z.infer<typeof OwnerRecordSchema>;

export type OwnerResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "already_running" | "not_found" | "unavailable";
        readonly message: string;
      };
    };

export interface WayfinderOwnerLease {
  publish(
    gateway: { readonly host: string; readonly port: number; readonly basePath: string },
    issueTicket: () => OwnerResult<{ readonly ticket: string; readonly expiresAt: string }>,
  ): Promise<OwnerResult<void>>;
  close(): Promise<void>;
}

export function acquireWayfinderOwner(databasePath: string): OwnerResult<WayfinderOwnerLease> {
  const path = ownerPath(databasePath);
  mkdirSync(dirname(path), { recursive: true });
  const state = clearStale(path);
  if (state === "live")
    return failure("already_running", "another Wayfinder owns this state database");
  if (state === "invalid")
    return failure("unavailable", "Wayfinder owner record is incomplete or unreadable");
  const ownerId = randomUUID();
  const secret = randomBytes(32).toString("base64url");
  const initial: OwnerRecord = {
    ownerId,
    pid: process.pid,
    secret,
    controlPort: null,
    gateway: null,
  };
  try {
    const descriptor = openSync(path, "wx", 0o600);
    writeFileSync(descriptor, JSON.stringify(initial), "utf8");
    closeSync(descriptor);
  } catch {
    return failure("already_running", "another Wayfinder owns this state database");
  }
  let server: Server | undefined;
  return {
    ok: true,
    value: {
      async publish(gateway, issueTicket) {
        if (server !== undefined) return { ok: true, value: undefined };
        server = createServer((request, response) => {
          if (
            request.method !== "POST" ||
            request.url !== "/ticket" ||
            request.headers.authorization !== `Bearer ${secret}`
          ) {
            response.writeHead(404).end();
            return;
          }
          const ticket = issueTicket();
          response.writeHead(ticket.ok ? 200 : 503, { "content-type": "application/json" });
          response.end(JSON.stringify(ticket));
        });
        const listening = await listen(server);
        if (!listening.ok) return listening;
        const record: OwnerRecord = {
          ...initial,
          controlPort: listening.value,
          gateway,
        };
        writeFileSync(path, JSON.stringify(record), { encoding: "utf8", mode: 0o600 });
        return { ok: true, value: undefined };
      },
      async close() {
        if (server !== undefined) await closeServer(server);
        removeIfOwned(path, ownerId);
      },
    },
  };
}

export async function requestRunningOwnerTicket(databasePath: string): Promise<
  OwnerResult<{
    readonly ticket: string;
    readonly expiresAt: string;
    readonly gateway: { readonly host: string; readonly port: number; readonly basePath: string };
  }>
> {
  const path = ownerPath(databasePath);
  const state = readOwnerState(path);
  if (state.state === "absent") {
    return failure("not_found", "no running Wayfinder owns this state database");
  }
  if (state.state === "invalid")
    return failure("unavailable", "Wayfinder owner record is incomplete or unreadable");
  const record = state.record;
  if (!pidAlive(record.pid)) {
    clearStale(path);
    return failure("not_found", "no running Wayfinder owns this state database");
  }
  if (record.controlPort === null || record.gateway === null) {
    return failure("unavailable", "Wayfinder owner is still starting");
  }
  try {
    const response = await fetch(`http://127.0.0.1:${String(record.controlPort)}/ticket`, {
      method: "POST",
      headers: { authorization: `Bearer ${record.secret}` },
      signal: AbortSignal.timeout(5_000),
    });
    const raw: unknown = await response.json();
    const parsed = z
      .object({
        ok: z.literal(true),
        value: z.object({ ticket: z.string().min(1), expiresAt: z.string().min(1) }).strict(),
      })
      .strict()
      .safeParse(raw);
    return parsed.success
      ? { ok: true, value: { ...parsed.data.value, gateway: record.gateway } }
      : failure("unavailable", "running Wayfinder did not issue a ticket");
  } catch {
    return failure("unavailable", "running Wayfinder owner control is unavailable");
  }
}

function ownerPath(databasePath: string): string {
  return `${databasePath}.owner.json`;
}

function readOwnerState(
  path: string,
):
  | { readonly state: "absent" }
  | { readonly state: "invalid" }
  | { readonly state: "valid"; readonly record: OwnerRecord } {
  if (!existsSync(path)) return { state: "absent" };
  try {
    const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
    const parsed = OwnerRecordSchema.safeParse(raw);
    return parsed.success ? { state: "valid", record: parsed.data } : { state: "invalid" };
  } catch {
    return existsSync(path) ? { state: "invalid" } : { state: "absent" };
  }
}

function clearStale(path: string): "ready" | "live" | "invalid" {
  const state = readOwnerState(path);
  if (state.state === "absent") return "ready";
  if (state.state === "invalid") return "invalid";
  if (pidAlive(state.record.pid)) return "live";
  try {
    unlinkSync(path);
    return "ready";
  } catch {
    return "invalid";
  }
}

function removeIfOwned(path: string, ownerId: string): void {
  const state = readOwnerState(path);
  if (state.state !== "valid" || state.record.ownerId !== ownerId) return;
  try {
    unlinkSync(path);
  } catch {
    return;
  }
}

function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function listen(server: Server): Promise<OwnerResult<number>> {
  return new Promise((resolve) => {
    const failed = (): void => {
      resolve(failure("unavailable", "owner control could not bind"));
    };
    server.once("error", failed);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", failed);
      const address = server.address();
      resolve(
        typeof address === "object" && address !== null
          ? { ok: true, value: address.port }
          : failure("unavailable", "owner control address is unavailable"),
      );
    });
  });
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolve) => {
    server.close(() => {
      resolve();
    });
  });
}

function failure(
  code: "already_running" | "not_found" | "unavailable",
  message: string,
): OwnerResult<never> {
  return { ok: false, error: { code, message } };
}
