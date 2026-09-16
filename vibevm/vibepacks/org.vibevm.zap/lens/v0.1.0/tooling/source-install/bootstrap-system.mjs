/** Node-only bootstrap ports. @scope spec://org.vibevm.zap/lens/PROP-016#ownership */
import { spawn } from "node:child_process";
import {
  access,
  copyFile,
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { homedir } from "node:os";

export function createNodeBootstrapPorts() {
  return {
    fs: {
      access,
      copyFile,
      lstat,
      mkdir,
      readFile,
      readdir,
      realpath,
      rename,
      rm,
      stat,
      writeFile,
    },
    process: { run: runProcess },
    now: () => new Date(),
    platform: process.platform,
    homeDir: homedir(),
    modulePath: import.meta.filename,
    env: { ...process.env },
    nodeExecutable: process.execPath,
    nodeVersion: process.versions.node,
    processId: process.pid,
  };
}

function runProcess(file, args, options) {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    let settled = false;
    const child = spawn(file, args, {
      cwd: options.cwd,
      env: options.env,
      shell: false,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout?.on("data", (chunk) => {
      const text = String(chunk);
      process.stdout.write(text);
      stdout = bounded(stdout, text);
    });
    child.stderr?.on("data", (chunk) => {
      const text = String(chunk);
      process.stderr.write(text);
      stderr = bounded(stderr, text);
    });
    child.once("error", (error) => {
      if (settled) return;
      settled = true;
      resolve({ code: null, stdout, stderr: bounded(stderr, error.message) });
    });
    child.once("exit", (code) => {
      if (settled) return;
      settled = true;
      resolve({ code, stdout, stderr });
    });
  });
}

function bounded(current, added) {
  return `${current}${added}`.slice(-65_536);
}
