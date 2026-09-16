#!/usr/bin/env node
/** Zap Wayfinder local runtime entry point. */
import {
  acquireWayfinderOwner,
  createWayfinderRuntime,
  loadWayfinderConfig,
  requestRunningOwnerTicket,
  type OwnerResult,
  type WayfinderOwnerLease,
  type WayfinderRuntimeConfig,
} from "./cells/wayfinder-runtime/index.ts";

const configPath = process.argv[2] ?? process.env["WAYFINDER_CONFIG_FILE"];
const issueTicketOnly = process.argv.includes("--issue-pairing-ticket");
if (configPath === undefined) {
  fail(
    "violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: config path is required; fix: pass zap-wayfinder <config.json>",
    2,
  );
} else {
  const loaded = await loadWayfinderConfig(configPath);
  if (!loaded.ok) {
    fail(loaded.error.message, 2);
  } else {
    const browserOrigin = loaded.value.gateway.allowedOrigins[0] ?? "http://127.0.0.1:4174";
    const existing = issueTicketOnly
      ? await requestRunningOwnerTicket(loaded.value.state.databasePath)
      : null;
    if (existing?.ok === true) {
      emitTicket(
        existing.value.gateway,
        existing.value.ticket,
        existing.value.expiresAt,
        browserOrigin,
        true,
      );
    } else if (existing !== null && existing.error.code !== "not_found") {
      fail(existing.error.message, 1);
    } else {
      await startOwner(loaded.value, browserOrigin, issueTicketOnly);
    }
  }
}

async function startOwner(
  config: WayfinderRuntimeConfig,
  browserOrigin: string,
  ticketOnly: boolean,
): Promise<void> {
  const owner = acquireWayfinderOwner(config.state.databasePath);
  if (!owner.ok) {
    fail(owner.error.message, 1);
    return;
  }
  const created = createWayfinderRuntime(config);
  if (!created.ok) {
    await owner.value.close();
    fail(created.error.message, 2);
    return;
  }
  const started = await created.value.start();
  if (!started.ok) {
    await closeOwner(() => created.value.close(), owner.value);
    fail(started.error.message, 1);
    return;
  }
  const gateway = {
    host: started.value.host,
    port: started.value.port,
    basePath: started.value.basePath,
  };
  const published = await owner.value.publish(gateway, () =>
    ownerTicket(created.value.issuePairingTicket()),
  );
  if (!published.ok) {
    await closeOwner(() => created.value.close(), owner.value);
    fail(published.error.message, 1);
    return;
  }
  const ticket = created.value.issuePairingTicket();
  console.log(
    JSON.stringify({
      protocol: "zap-wayfinder/1",
      receipt: started.value,
      attachUrl: ticket.ok ? attachUrl(gateway, ticket.value.ticket, browserOrigin) : null,
      ticketOnly,
      reusedOwner: false,
    }),
  );
  const close = async (): Promise<void> => {
    await closeOwner(() => created.value.close(), owner.value);
    process.exit(0);
  };
  process.once("SIGINT", () => void close());
  process.once("SIGTERM", () => void close());
  setInterval(() => undefined, 60_000);
}

function ownerTicket(
  ticket:
    | {
        readonly ok: true;
        readonly value: { readonly ticket: string; readonly expiresAt: string };
      }
    | { readonly ok: false; readonly error: { readonly message: string } },
): OwnerResult<{ readonly ticket: string; readonly expiresAt: string }> {
  return ticket.ok
    ? ticket
    : { ok: false, error: { code: "unavailable", message: ticket.error.message } };
}

function emitTicket(
  gateway: { readonly host: string; readonly port: number; readonly basePath: string },
  ticket: string,
  expiresAt: string,
  browserOrigin: string,
  reusedOwner: boolean,
): void {
  console.log(
    JSON.stringify({
      protocol: "zap-wayfinder/1",
      receipt: gateway,
      attachUrl: attachUrl(gateway, ticket, browserOrigin),
      ticketExpiresAt: expiresAt,
      ticketOnly: true,
      reusedOwner,
    }),
  );
}

function attachUrl(
  gateway: { readonly host: string; readonly port: number; readonly basePath: string },
  ticket: string,
  browserOrigin: string,
): string {
  const gatewayUrl = `http://${gateway.host}:${String(gateway.port)}${gateway.basePath}`;
  return `${browserOrigin}/?workspace-gateway=${encodeURIComponent(gatewayUrl)}#workspace-pair=${encodeURIComponent(ticket)}`;
}

async function closeOwner(closeRuntime: () => Promise<void>, owner: WayfinderOwnerLease) {
  await closeRuntime();
  await owner.close();
}

function fail(message: string, exitCode: number): void {
  console.error(message);
  process.exitCode = exitCode;
}
