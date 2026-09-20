/** Local Wayfinder owner/ticket proof. @scope spec://org.vibevm.zap/lens/PROP-005#simultaneous-clients */
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import {
  acquireWayfinderOwner,
  requestRunningOwnerStop,
  requestRunningOwnerTicket,
} from "./owner.ts";

test("one live owner issues fresh attach tickets without another state owner", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "wayfinder-owner-"));
  const databasePath = join(directory, "workspace.sqlite");
  const first = acquireWayfinderOwner(databasePath);
  assert.equal(first.ok, true);
  if (!first.ok) return;
  const second = acquireWayfinderOwner(databasePath);
  assert.equal(second.ok, false);
  if (!second.ok) assert.equal(second.error.code, "already_running");
  let issued = 0;
  const published = await first.value.publish(
    { host: "127.0.0.1", port: 43110, basePath: "/workspace" },
    () => {
      issued += 1;
      return {
        ok: true,
        value: { ticket: `ticket.${String(issued)}`, expiresAt: `expiry.${String(issued)}` },
      };
    },
  );
  assert.equal(published.ok, true);
  const ticketA = await requestRunningOwnerTicket(databasePath);
  const ticketB = await requestRunningOwnerTicket(databasePath);
  assert.equal(ticketA.ok, true);
  assert.equal(ticketB.ok, true);
  if (ticketA.ok && ticketB.ok) {
    assert.equal(ticketA.value.ticket, "ticket.1");
    assert.equal(ticketB.value.ticket, "ticket.2");
    assert.deepEqual(ticketA.value.gateway, {
      host: "127.0.0.1",
      port: 43110,
      basePath: "/workspace",
    });
  }
  await first.value.close();
  const replacement = acquireWayfinderOwner(databasePath);
  assert.equal(replacement.ok, true);
  if (replacement.ok) await replacement.value.close();
  rmSync(directory, { recursive: true, force: true });
});

test("owner creates the state parent and never removes an in-progress unreadable lock", async () => {
  const directory = mkdtempSync(
    join(process.env["TEMP"] ?? process.cwd(), "wayfinder-owner-race-"),
  );
  const databasePath = join(directory, "nested", "workspace.sqlite");
  const first = acquireWayfinderOwner(databasePath);
  assert.equal(first.ok, true);
  if (!first.ok) return;
  await first.value.close();
  const ownerPath = `${databasePath}.owner.json`;
  writeFileSync(ownerPath, "", "utf8");
  const refused = acquireWayfinderOwner(databasePath);
  assert.equal(refused.ok, false);
  if (!refused.ok) assert.equal(refused.error.code, "unavailable");
  assert.equal(existsSync(ownerPath), true);
  rmSync(directory, { recursive: true, force: true });
});

test("authenticated owner control stops only the exact live owner", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "wayfinder-stop-"));
  const databasePath = join(directory, "workspace.sqlite");
  const owner = acquireWayfinderOwner(databasePath);
  assert.equal(owner.ok, true);
  if (!owner.ok) return;
  const published = await owner.value.publish(
    { host: "127.0.0.1", port: 43111, basePath: "/workspace" },
    () => ({ ok: true, value: { ticket: "ticket.stop", expiresAt: "expiry.stop" } }),
    () => void owner.value.close(),
  );
  assert.equal(published.ok, true);
  const stopped = await requestRunningOwnerStop(databasePath);
  assert.equal(stopped.ok, true, stopped.ok ? undefined : stopped.error.message);
  assert.equal(existsSync(`${databasePath}.owner.json`), false);
  const absent = await requestRunningOwnerStop(databasePath);
  assert.equal(absent.ok, false);
  if (!absent.ok) assert.equal(absent.error.code, "not_found");
  rmSync(directory, { recursive: true, force: true });
});
