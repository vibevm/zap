import { fileURLToPath, pathToFileURL } from "node:url";
import { existsSync } from "node:fs";

const entry =
  process.env.CODLENS_CLI_PATH ??
  fileURLToPath(new URL("../../../../dist/cli.js", import.meta.url));
if (!existsSync(entry)) {
  process.stderr.write(
    "codlens: CODLENS_CLI_PATH must name the installed dist/cli.js when the plugin is copied outside its package\n",
  );
  process.exit(2);
}
try {
  const cli = await import(pathToFileURL(entry).href);
  if (typeof cli.runCli !== "function") throw new Error("missing runCli");
  const status = await cli.runCli(["mcp", "serve"], process.env, (line) =>
    process.stdout.write(`${line}\n`),
  );
  process.exitCode = status;
  if (status !== 0) {
    process.stderr.write(
      "codlens: configure CODLENS_URL and CODLENS_CREDENTIAL_FILE for the running local broker\n",
    );
  }
} catch {
  process.exitCode = 1;
}
