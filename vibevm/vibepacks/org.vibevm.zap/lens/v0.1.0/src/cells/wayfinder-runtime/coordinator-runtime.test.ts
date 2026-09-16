/** @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import assert from "node:assert/strict";
import test from "node:test";
import type { CodexProcessFactory } from "../codex-coordinator/index.ts";
import { createCodexHost } from "./coordinator-runtime.ts";

test("one protected profile opens distinct coordinator adapters for concurrent projects", async () => {
  const processFactory: CodexProcessFactory = {
    start: () =>
      Promise.resolve({
        ok: false,
        error: {
          kind: "spawn_failed",
          message: "no process is started by this host identity proof",
        },
      }),
  };
  const host = createCodexHost(
    {
      profileId: "profile.concurrent.projects",
      executablePath: "C:/fixture/codex.exe",
      requestTimeoutMs: 30_000,
      model: "no-model",
      effort: "low",
      approvalPolicy: "on-request",
      sandbox: "workspace-write",
      personality: "pragmatic",
      serviceName: "concurrent-project-test",
    },
    processFactory,
  );
  const first = await host.openCoordinator("profile.concurrent.projects");
  const second = await host.openCoordinator("profile.concurrent.projects");
  assert.equal(first.ok, true);
  assert.equal(second.ok, true);
  if (!first.ok || !second.ok) return;
  assert.notEqual(first.value, second.value);
  host.closeOwned();
});
