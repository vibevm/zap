/** CLI proof that a new client attaches to the live Wayfinder state owner. */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import test from "node:test";
import { z } from "zod";

test("ticket-only CLI reuses the running state owner without starting another runtime", async () => {
  const directory = mkdtempSync(join(process.env["TEMP"] ?? process.cwd(), "wayfinder-owner-cli-"));
  const configPath = join(directory, "wayfinder.json");
  writeFileSync(configPath, JSON.stringify(config(directory)), "utf8");
  const owner = launch(configPath, []);
  try {
    const initial = await line(owner);
    assert.equal(initial.reusedOwner, false);
    const client = launch(configPath, ["--issue-pairing-ticket"]);
    const attached = await line(client);
    const exitCode = await exited(client);
    assert.equal(exitCode, 0);
    assert.equal(attached.reusedOwner, true);
    assert.equal(attached.ticketOnly, true);
    assert.match(attached.attachUrl, /workspace-pair=/);
    assert.equal(owner.exitCode, null);
  } finally {
    owner.kill("SIGTERM");
    await exited(owner);
    rmSync(directory, { recursive: true, force: true });
  }
});

interface CliReceipt {
  readonly reusedOwner: boolean;
  readonly ticketOnly: boolean;
  readonly attachUrl: string;
}

function launch(configPath: string, args: readonly string[]): ChildProcess {
  return spawn(
    process.execPath,
    ["--experimental-strip-types", "src/wayfinder.ts", configPath, ...args],
    {
      cwd: process.cwd(),
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
}

function line(child: ChildProcess): Promise<CliReceipt> {
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
        resolve(receipt(parseJson(stdout.slice(0, boundary))));
      } catch (error) {
        reject(
          error instanceof Error
            ? error
            : new Error(
                "violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: CLI receipt parsing failed; fix surface: return one valid JSON receipt line",
              ),
        );
      }
    });
    child.once("exit", (code) => {
      if (stdout.indexOf("\n") < 0)
        reject(new Error(`Wayfinder exited ${String(code)}: ${stderr}`));
    });
  });
}

function receipt(value: unknown): CliReceipt {
  return z
    .object({ reusedOwner: z.boolean(), ticketOnly: z.boolean(), attachUrl: z.string().min(1) })
    .passthrough()
    .parse(value);
}

function parseJson(text: string): unknown {
  return JSON.parse(text);
}

function exited(child: ChildProcess): Promise<number | null> {
  if (child.exitCode !== null) return Promise.resolve(child.exitCode);
  return new Promise((resolve) => child.once("exit", resolve));
}

function config(directory: string) {
  return {
    version: 1,
    state: { databasePath: join(directory, "workspace.sqlite") },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "owner-cli",
      pairingToken: "synthetic-owner-cli-pairing-token-0001",
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: ["http://quicklens.test"],
    },
    profiles: [
      {
        profileId: "profile.codex.native",
        executablePath: "C:/placeholder/codex.exe",
        requestTimeoutMs: 30_000,
        model: "gpt-5.6-luna",
        effort: "low",
        approvalPolicy: "on-request",
        sandbox: "workspace-write",
        personality: "pragmatic",
        serviceName: "owner-cli-test",
      },
    ],
    projects: [
      {
        registrationId: "registration.owner.cli",
        projectId: "project.owner.cli",
        displayName: "Owner CLI",
        repositoryRootRefs: ["repository.owner.cli"],
        actions: { startCoordinator: { state: "available" } },
        context: {
          contextId: "context.owner.cli",
          displayName: "Owner CLI context",
          workspaceRef: "workspace.owner.cli",
          branchLabel: "main",
          revisionBinding: "revision.owner.cli",
          planning: { state: "unavailable", reason: "Synthetic owner fixture" },
          coordinatorConversationId: "conversation.owner.cli",
        },
        coordinatorLaunchOptions: [
          {
            profileId: "profile.codex.native",
            label: "Native",
            interactionKind: "structured",
            availability: { state: "available" },
          },
        ],
        protected: { cwd: "C:/placeholder/owner", launchProfileRef: "profile.codex.native" },
      },
    ],
    modelPolicies: [],
  };
}
