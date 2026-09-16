import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import {
  createExecutionAccountIsolation,
  isolatedExecutionEnvironment,
  ProtectedExecutionBindingSchema,
} from "./index.ts";

test("protected homes isolate two Codex accounts and one Claude account", async () => {
  const root = await mkdtemp(join(tmpdir(), "zap-execution-accounts-"));
  try {
    const codexA = join(root, "codex-a");
    const codexB = join(root, "codex-b");
    const claude = join(root, "claude");
    await Promise.all([mkdir(codexA), mkdir(codexB), mkdir(claude)]);
    const hostId = ExecutionHostIdSchema.parse("host.local.test");
    const opened = createExecutionAccountIsolation([
      binding("binding.codex.a", hostId, "codex_home", "codex", codexA),
      binding("binding.codex.b", hostId, "codex_home", "codex", codexB),
      binding("binding.claude.a", hostId, "claude_config_dir", "claude_code", claude),
    ]);
    assert.equal(opened.ok, true);
    if (!opened.ok) return;
    assert.deepEqual(
      opened.value.list().map((binding) => Object.keys(binding).sort()),
      [
        ["agentProduct", "bindingId", "displayName", "enabled", "hostId", "setupGuidance"],
        ["agentProduct", "bindingId", "displayName", "enabled", "hostId", "setupGuidance"],
        ["agentProduct", "bindingId", "displayName", "enabled", "hostId", "setupGuidance"],
      ],
    );
    const a = await opened.value.resolve({
      bindingId: "binding.codex.a",
      hostId,
      agentProduct: "codex",
    });
    const b = await opened.value.resolve({
      bindingId: "binding.codex.b",
      hostId,
      agentProduct: "codex",
    });
    const c = await opened.value.resolve({
      bindingId: "binding.claude.a",
      hostId,
      agentProduct: "claude_code",
    });
    assert.equal(a.ok && a.value.environment["CODEX_HOME"], codexA);
    assert.equal(b.ok && b.value.environment["CODEX_HOME"], codexB);
    assert.notEqual(
      a.ok ? a.value.environment["CODEX_HOME"] : null,
      b.ok ? b.value.environment["CODEX_HOME"] : null,
    );
    assert.equal(c.ok && c.value.environment["CLAUDE_CONFIG_DIR"], claude);
    const crossed = await opened.value.resolve({
      bindingId: "binding.codex.a",
      hostId,
      agentProduct: "claude_code",
    });
    assert.equal(crossed.ok, false);
    assert.equal(crossed.ok ? "unexpected" : crossed.error.code, "product_mismatch");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("isolated account environment removes unrelated ambient credentials", () => {
  const environment = isolatedExecutionEnvironment(
    {
      PATH: "synthetic-path",
      HTTPS_PROXY: "http://proxy.invalid",
      ANTHROPIC_API_KEY: "synthetic-ambient-secret",
      OPENAI_API_KEY: "synthetic-ambient-secret",
    },
    { CLAUDE_CONFIG_DIR: "C:/synthetic/claude", CODLENS_URL: "http://127.0.0.1:1" },
  );
  assert.equal(environment["PATH"], "synthetic-path");
  assert.equal(environment["HTTPS_PROXY"], "http://proxy.invalid");
  assert.equal(environment["ANTHROPIC_API_KEY"], undefined);
  assert.equal(environment["OPENAI_API_KEY"], undefined);
  assert.equal(environment["CLAUDE_CONFIG_DIR"], "C:/synthetic/claude");
});

function binding(
  bindingId: string,
  hostId: ReturnType<typeof ExecutionHostIdSchema.parse>,
  kind: "codex_home" | "claude_config_dir",
  agentProduct: "codex" | "claude_code",
  homePath: string,
) {
  return ProtectedExecutionBindingSchema.parse({
    bindingId,
    hostId,
    displayName: bindingId,
    enabled: true,
    setupGuidance: `Create and sign in to the protected home for ${bindingId}.`,
    kind,
    agentProduct,
    homePath,
  });
}
