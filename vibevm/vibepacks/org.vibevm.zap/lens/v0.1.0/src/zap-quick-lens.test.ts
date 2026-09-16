/** Normal launcher ownership proof. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import test from "node:test";
import { z } from "zod";
import { createWorkspaceHttpConnection } from "./cells/workspace-client/index.ts";

const ReceiptSchema = z
  .object({
    protocol: z.literal("zap-quick-lens/1"),
    url: z.url(),
    reusedOwner: z.boolean(),
    databasePath: z.string().min(1),
    presentation: z.enum(["browser", "electron"]),
  })
  .passthrough();

test("ordinary launcher starts empty then a second invocation reuses the owner", async () => {
  const state = await mkdtemp(join(tmpdir(), "zap-quick-lens-cli-"));
  await writeFile(
    join(state, "settings.json"),
    JSON.stringify({
      version: 1,
      uiPort: 4174,
      proxy: { mode: "inherit" },
      coordinatorDefaults: { modelId: "gpt-5.6-luna", effort: "low" },
    }),
  );
  const owner = launch(state);
  try {
    const first = await receipt(owner);
    assert.equal(first.reusedOwner, false);
    assert.match(first.url, /^http:\/\/127\.0\.0\.1:4174\//);
    const attach = new URL(first.url);
    const gateway = attach.searchParams.get("workspace-gateway");
    const pairing = new URLSearchParams(attach.hash.slice(1)).get("workspace-pair");
    assert.notEqual(gateway, null);
    assert.notEqual(pairing, null);
    if (gateway === null || pairing === null) return;
    const connection = createWorkspaceHttpConnection({
      baseUrl: gateway,
      origin: attach.origin,
      pairingToken: pairing,
    });
    assert.notEqual(connection, null);
    if (connection === null) return;
    const setup = await connection.product.request({ operation: "product.setup.get.v1" });
    assert.equal(
      setup.ok &&
        setup.value.operation === "product.setup.get.v1" &&
        setup.value.snapshot.providers.some(
          (profile) =>
            profile.provider === "codex" &&
            profile.installed &&
            profile.configured &&
            profile.authenticated === "not_observed",
        ),
      true,
    );
    const secondProcess = launch(state);
    const second = await receipt(secondProcess);
    assert.equal(await exitCode(secondProcess), 0);
    assert.equal(second.reusedOwner, true);
    assert.equal(second.databasePath, first.databasePath);
    assert.equal(owner.exitCode, null);
  } finally {
    owner.kill("SIGTERM");
    await exitCode(owner);
    await rm(state, { recursive: true, force: true });
  }
});

function launch(state: string): ChildProcess {
  return spawn(
    process.execPath,
    ["--experimental-strip-types", "src/zap-quick-lens.ts", "--state-dir", state, "--no-open"],
    { cwd: process.cwd(), stdio: ["ignore", "pipe", "pipe"] },
  );
}

function receipt(child: ChildProcess): Promise<z.infer<typeof ReceiptSchema>> {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stderr?.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.stdout?.on("data", (chunk: string) => {
      stdout += chunk;
      const boundary = stdout.indexOf("\n");
      if (boundary < 0) return;
      try {
        const raw: unknown = JSON.parse(stdout.slice(0, boundary));
        resolve(ReceiptSchema.parse(raw));
      } catch (error) {
        reject(
          error instanceof Error
            ? error
            : new Error(
                "violates REQ spec://org.vibevm.zap/lens/PROP-010#start-and-projects: launcher receipt could not be parsed; fix surface: emit one JSON receipt line",
              ),
        );
      }
    });
    child.once("exit", (code) => {
      if (!stdout.includes("\n")) reject(new Error(`launcher exited ${String(code)}: ${stderr}`));
    });
  });
}

function exitCode(child: ChildProcess): Promise<number | null> {
  return child.exitCode === null
    ? new Promise((resolve) => child.once("exit", resolve))
    : Promise.resolve(child.exitCode);
}
