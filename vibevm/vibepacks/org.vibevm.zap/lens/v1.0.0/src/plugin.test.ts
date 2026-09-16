import assert from "node:assert/strict";
import { copyFileSync, cpSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const packageRoot = dirname(fileURLToPath(new URL("../package.json", import.meta.url)));

test("copied plugin launches a real JS entry through node without cmd shell lookup", () => {
  const root = mkdtempSync(join(tmpdir(), "codlens-plugin-"));
  const plugin = join(root, "codlens");
  cpSync(join(packageRoot, "integrations", "plugins", "codlens"), plugin, { recursive: true });
  const marker = join(root, "mcp-marker.txt");
  const entry = join(root, "installed-codlens.mjs");
  writeFileSync(
    entry,
    `import { writeFileSync } from "node:fs";
export async function runCli(args, _environment, write) {
  if (args[0] === "mcp" && args[1] === "serve") writeFileSync(${JSON.stringify(marker)}, "ok");
  if (args[0] === "host-hook") write(JSON.stringify({hookSpecificOutput:{additionalContext:"bounded"}}));
  return 0;
}
`,
    "utf8",
  );
  const environment = { ...process.env, CODLENS_CLI_PATH: entry };
  const mcp = spawnSync(process.execPath, [join(plugin, "scripts", "codlens-mcp.mjs")], {
    env: environment,
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(mcp.status, 0);
  assert.equal(readFileSync(marker, "utf8"), "ok");

  const hookCopy = join(root, "codlens-hook-copy.mjs");
  copyFileSync(join(plugin, "scripts", "codlens-hook.mjs"), hookCopy);
  const hook = spawnSync(process.execPath, [hookCopy, "codex"], {
    env: environment,
    input: "{}",
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(hook.status, 0);
  assert.match(hook.stdout, /additionalContext/);

  const missing = { ...process.env };
  delete missing["CODLENS_CLI_PATH"];
  const refused = spawnSync(process.execPath, [join(plugin, "scripts", "codlens-mcp.mjs")], {
    env: missing,
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(refused.status, 2);
  assert.match(refused.stderr, /CODLENS_CLI_PATH/);
});
