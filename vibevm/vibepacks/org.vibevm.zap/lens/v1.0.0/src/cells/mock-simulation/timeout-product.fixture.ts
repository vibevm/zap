/** Intentional hanging process tree used only by product-process.test. */
import { spawn } from "node:child_process";

const descendant = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
  stdio: "ignore",
  windowsHide: true,
});
process.stdout.write(`OWNED_DESCENDANT_PID ${String(descendant.pid)}\n`);
setInterval(() => {}, 1_000);
