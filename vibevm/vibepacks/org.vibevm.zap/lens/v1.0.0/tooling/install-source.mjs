#!/usr/bin/env node
/** Zap user-local Vibe source installer. @scope spec://org.vibevm.zap/lens/PROP-016#commands */
import { executeBootstrap, helpText, parseBootstrapArgs } from "./source-install/bootstrap.mjs";
import { createNodeBootstrapPorts } from "./source-install/bootstrap-system.mjs";

const parsed = parseBootstrapArgs(process.argv.slice(2), process.env);
if (!parsed.ok) {
  console.error(parsed.error.message);
  console.error(helpText());
  process.exitCode = 2;
} else if (parsed.value.help) {
  console.log(helpText());
} else {
  const result = await executeBootstrap(parsed.value, createNodeBootstrapPorts());
  const output = {
    protocol: "zap-source-install-result/1",
    ok: result.ok,
    operation: result.operation,
    state: result.state,
    installerRoot: result.installerRoot,
    message: result.message,
    commands: result.commands.map((command) => ({
      file: command.file,
      args: command.args,
      operation: command.operation,
    })),
    ...(result.detail === undefined ? {} : { detail: result.detail }),
  };
  console.log(JSON.stringify(output));
  process.exitCode = result.code;
}
