/** Short headless product command proof. @scope spec://org.vibevm.zap/lens/PROP-017#verification */
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import test from "node:test";
import { z } from "zod";
import { createWorkspaceHttpConnection } from "./cells/workspace-client/index.ts";

const ServerReceiptSchema = z
  .object({
    protocol: z.literal("zap-server/1"),
    url: z.url(),
    reusedOwner: z.boolean(),
    databasePath: z.string().min(1),
    headless: z.literal(true),
    viewerOpened: z.literal(false),
  })
  .passthrough();

test("zap-server help and incompatible presentation exit before state creation", async () => {
  const parent = await mkdtemp(join(tmpdir(), "zap-server-help-"));
  const state = join(parent, "must-not-exist");
  try {
    const help = await output(
      spawn(
        process.execPath,
        ["--experimental-strip-types", "src/zap-server.ts", "--help", "--state-dir", state],
        { cwd: process.cwd(), stdio: ["ignore", "pipe", "pipe"] },
      ),
    );
    assert.equal(help.code, 0);
    assert.match(help.stdout, /^Zap Server\r?\n/);
    assert.match(help.stdout, /Usage: zap-server \[options\]/);
    assert.match(help.stdout, /does not start a coordinator, worker, child agent, or model turn/i);
    assert.equal(existsSync(state), false);
    const incompatible = await output(
      spawn(
        process.execPath,
        ["--experimental-strip-types", "src/zap-server.ts", "--electron", "--state-dir", state],
        { cwd: process.cwd(), stdio: ["ignore", "pipe", "pipe"] },
      ),
    );
    assert.equal(incompatible.code, 2);
    assert.match(incompatible.stderr, /does not open a presentation/);
    assert.equal(existsSync(state), false);
  } finally {
    await rm(parent, { recursive: true, force: true });
  }
});

test("zap-server starts the normal HTTP stack headlessly without agents", async () => {
  const state = await mkdtemp(join(tmpdir(), "zap-server-product-"));
  const uiPort = 24_000 + (process.pid % 1_000);
  await writeFile(
    join(state, "settings.json"),
    JSON.stringify({
      version: 1,
      uiPort,
      proxy: { mode: "inherit" },
      coordinatorDefaults: { modelId: "gpt-5.6-luna", effort: "low" },
    }),
  );
  const server = spawn(
    process.execPath,
    ["--experimental-strip-types", "src/zap-server.ts", "--state-dir", state],
    { cwd: process.cwd(), stdio: ["ignore", "pipe", "pipe"] },
  );
  try {
    const started = await receipt(server);
    assert.equal(started.reusedOwner, false);
    assert.equal(started.headless, true);
    assert.equal(started.viewerOpened, false);
    assert.equal("presentation" in started, false);
    const attach = new URL(started.url);
    assert.equal(attach.origin, `http://127.0.0.1:${String(uiPort)}`);
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
        setup.value.snapshot.projects.length === 0,
      true,
    );
    assert.equal(server.exitCode, null);
  } finally {
    server.kill("SIGTERM");
    await exitCode(server);
    await rm(state, { recursive: true, force: true });
  }
});

function receipt(child: ChildProcess): Promise<z.infer<typeof ServerReceiptSchema>> {
  return new Promise((resolveReceipt, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stderr?.on("data", (chunk: string) => (stderr += chunk));
    child.stdout?.on("data", (chunk: string) => {
      stdout += chunk;
      const boundary = stdout.indexOf("\n");
      if (boundary < 0) return;
      try {
        const raw: unknown = JSON.parse(stdout.slice(0, boundary));
        resolveReceipt(ServerReceiptSchema.parse(raw));
      } catch (error) {
        reject(
          error instanceof Error
            ? error
            : new Error(
                reqMessage(
                  "zap-server receipt could not be parsed",
                  "emit one typed JSON receipt line",
                ),
              ),
        );
      }
    });
    child.once("exit", (code) => {
      if (!stdout.includes("\n")) reject(new Error(`zap-server exited ${String(code)}: ${stderr}`));
    });
  });
}

function exitCode(child: ChildProcess): Promise<number | null> {
  return child.exitCode === null
    ? new Promise((resolveExit) => child.once("exit", resolveExit))
    : Promise.resolve(child.exitCode);
}

function output(
  child: ChildProcess,
): Promise<{ readonly code: number | null; readonly stdout: string; readonly stderr: string }> {
  return new Promise((resolveOutput, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stdout?.on("data", (chunk: string) => (stdout += chunk));
    child.stderr?.on("data", (chunk: string) => (stderr += chunk));
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      reject(
        new Error(
          reqMessage(
            "zap-server help or presentation refusal did not exit before product startup",
            "handle read-only/refused arguments before settings and runtime composition",
          ),
        ),
      );
    }, 5_000);
    child.once("exit", (code) => {
      clearTimeout(timer);
      resolveOutput({ code, stdout, stderr });
    });
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });
}

function reqMessage(why: string, fix: string): string {
  return `violates REQ spec://org.vibevm.zap/lens/PROP-017#verification: ${why}; fix surface: ${fix}`;
}
