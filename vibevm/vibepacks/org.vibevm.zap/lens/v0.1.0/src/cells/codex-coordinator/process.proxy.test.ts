import assert from "node:assert/strict";
import test from "node:test";
import { codexProcessEnvironment, CodexCoordinatorProfileSchema } from "./index.ts";

test("Codex process environment applies profile proxy over global policy and ambient values", () => {
  const profile = CodexCoordinatorProfileSchema.parse({
    profileId: "codex.proxy.fixture",
    executablePath: "C:/fixture/codex.exe",
    requestTimeoutMs: 30_000,
    model: "gpt-5.6-luna",
    approvalPolicy: "never",
    sandbox: "read-only",
    proxy: { mode: "explicit", httpsProxy: "http://192.168.1.141:10808" },
  });
  const resolved = codexProcessEnvironment(profile, {
    proxyPolicy: { mode: "direct" },
    environment: {
      HTTPS_PROXY: "http://ambient:8080",
      https_proxy: "http://ambient:8080",
    },
  });
  assert.equal(resolved.environment["HTTPS_PROXY"], "http://192.168.1.141:10808");
  assert.equal(resolved.environment["https_proxy"], "http://192.168.1.141:10808");
  assert.match(resolved.environment["NO_PROXY"] ?? "", /localhost/);
  assert.match(resolved.environment["NO_PROXY"] ?? "", /127\.0\.0\.1/);
});
