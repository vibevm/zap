#!/usr/bin/env node
/** Ordinary headless Zap product stack. @scope spec://org.vibevm.zap/lens/PROP-017#launch */
import { runZapProductLauncher } from "./zap-quick-lens.ts";

await runZapProductLauncher({
  args: process.argv.slice(2),
  commandName: "zap-server",
  forceNoOpen: true,
  serverMode: true,
});
