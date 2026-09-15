import { fileURLToPath, pathToFileURL } from "node:url";
import { existsSync } from "node:fs";

const host = process.argv[2];
let input = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) {
  input += chunk;
  if (input.length > 65_536) process.exit(2);
}
const entry =
  process.env.CODLENS_CLI_PATH ??
  fileURLToPath(new URL("../../../../dist/cli.js", import.meta.url));
if (!["codex", "claude_code", "qwen_code"].includes(host)) process.exit(0);
if (!existsSync(entry)) {
  process.stderr.write(
    "codlens: CODLENS_CLI_PATH must name the installed dist/cli.js when the plugin is copied outside its package\n",
  );
  process.exit(2);
}
try {
  const cli = await import(pathToFileURL(entry).href);
  if (typeof cli.runCli !== "function") throw new Error("missing runCli");
  process.exitCode = await cli.runCli(["host-hook", host, input], process.env, (line) =>
    process.stdout.write(line),
  );
} catch {
  process.exitCode = 1;
}
