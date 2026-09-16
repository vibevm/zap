/** MCP protected credential envelope compatibility. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { CredentialSchema } from "./cells/protocol/index.ts";
import { readAgentCredential } from "./mcp.ts";

test("MCP reads both dynamic Codex scope and generated provider credential envelopes", () => {
  const root = mkdtempSync(join(tmpdir(), "zap-mcp-credential-"));
  const token = CredentialSchema.parse("credential.fixture.123456789012345678901234567890");
  try {
    const direct = join(root, "direct.json");
    const nested = join(root, "nested.json");
    writeFileSync(direct, JSON.stringify({ protocol: "lens/1", principalToken: token }), "utf8");
    writeFileSync(
      nested,
      JSON.stringify({ protocol: "lens/1", agent: { principalToken: token } }),
      "utf8",
    );
    assert.equal(readAgentCredential({ CODLENS_CREDENTIAL_FILE: direct }).success, true);
    assert.equal(readAgentCredential({ CODLENS_CREDENTIAL_FILE: nested }).success, true);
    const ambiguous = join(root, "ambiguous.json");
    writeFileSync(
      ambiguous,
      JSON.stringify({
        protocol: "lens/1",
        principalToken: token,
        agent: { principalToken: token },
      }),
      "utf8",
    );
    assert.equal(readAgentCredential({ CODLENS_CREDENTIAL_FILE: ambiguous }).success, false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
