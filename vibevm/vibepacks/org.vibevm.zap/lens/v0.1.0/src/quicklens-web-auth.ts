#!/usr/bin/env node
/**
 * Password setup/rotation entry; password input is hidden and never accepted as an argument.
 * @scope spec://org.vibevm.zap/lens/PROP-003#password-authentication
 */
import { pathToFileURL } from "node:url";

import { defaultPasswordVerifierPath, writePasswordVerifier } from "./cells/web-auth/index.ts";

export async function runQuicklensWebAuth(
  args: readonly string[],
  readPassword: () => Promise<readonly [string, string]>,
  write: (line: string) => void,
): Promise<number> {
  const parsed = parseArgs(args);
  if (parsed === null) return 2;
  const [password, confirmation] = await readPassword();
  if (password !== confirmation) return 2;
  const updated = await writePasswordVerifier({
    path: parsed.path,
    password,
    mode: parsed.mode,
  });
  if (!updated.ok) return 1;
  write(
    `Quicklens web password verifier ${parsed.mode === "init" ? "initialized" : "rotated"} at ${updated.value.path}`,
  );
  return 0;
}

function parseArgs(args: readonly string[]): { mode: "init" | "rotate"; path: string } | null {
  const mode = args[0];
  if (mode !== "init" && mode !== "rotate") return null;
  if (args.length === 1) return { mode, path: defaultPasswordVerifierPath() };
  return args.length === 3 && args[1] === "--path" && (args[2]?.length ?? 0) > 0
    ? { mode, path: args[2] ?? "" }
    : null;
}

async function hiddenPasswords(): Promise<readonly [string, string]> {
  if (!process.stdin.isTTY || typeof process.stdin.setRawMode !== "function") {
    return pipedPasswords();
  }
  return [await hiddenLine("Password: "), await hiddenLine("Confirm password: ")];
}

function hiddenLine(prompt: string): Promise<string> {
  process.stderr.write(prompt);
  process.stdin.setEncoding("utf8");
  process.stdin.setRawMode(true);
  process.stdin.resume();
  return new Promise((resolve, reject) => {
    let value = "";
    const receive = (chunk: string): void => {
      for (const character of chunk) {
        if (character === "\u0003") {
          finish();
          reject(
            new Error(
              "violates REQ spec://org.vibevm.zap/lens/PROP-003#password-authentication: password entry was cancelled; fix surface: rerun setup from a trusted terminal",
            ),
          );
          return;
        }
        if (character === "\r" || character === "\n") {
          finish();
          resolve(value);
          return;
        }
        if (character === "\b" || character === "\u007f") value = value.slice(0, -1);
        else if (Buffer.byteLength(value + character, "utf8") <= 1_024) value += character;
      }
    };
    const finish = (): void => {
      process.stdin.off("data", receive);
      process.stdin.setRawMode(false);
      process.stdin.pause();
      process.stderr.write("\n");
    };
    process.stdin.on("data", receive);
  });
}

async function pipedPasswords(): Promise<readonly [string, string]> {
  let value = "";
  for await (const chunk of process.stdin) {
    value += String(chunk);
    if (Buffer.byteLength(value, "utf8") > 2_100) return ["", ""];
  }
  const lines = value.split(/\r?\n/u);
  return [lines[0] ?? "", lines[1] ?? ""];
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = await runQuicklensWebAuth(
      process.argv.slice(2),
      hiddenPasswords,
      console.log,
    );
  } catch {
    process.exitCode = 2;
  }
}
