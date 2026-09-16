/** @verifies spec://org.vibevm.zap/lens/PROP-002#plan-control */
import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { Worker } from "node:worker_threads";
import { openSqlitePlanWorkflowStore } from "./workflow-store.ts";

test("a second workflow connection opens during a bounded short SQLite writer", async () => {
  const directory = await mkdtemp(join(tmpdir(), "quicklens-contention-"));
  const path = join(directory, "workflow.sqlite");
  const scope = { workspaceId: "workspace.contention", conversationId: "conversation.contention" };
  const initial = openSqlitePlanWorkflowStore(path, scope);
  assert.equal(initial.ok, true);
  if (!initial.ok) return;
  initial.value.close();
  const worker = new Worker(
    `const {parentPort}=require('node:worker_threads');const {DatabaseSync}=require('node:sqlite');const db=new DatabaseSync(${JSON.stringify(path)});db.exec('BEGIN IMMEDIATE');parentPort.postMessage('locked');setTimeout(()=>{db.exec('COMMIT');db.close()},150);`,
    { eval: true },
  );
  await once(worker, "message");
  const exited = once(worker, "exit");
  const second = openSqlitePlanWorkflowStore(path, scope);
  assert.equal(second.ok, true);
  if (second.ok) second.value.close();
  await exited;
});
