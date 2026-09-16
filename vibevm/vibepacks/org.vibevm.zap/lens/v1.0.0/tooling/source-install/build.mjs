#!/usr/bin/env node
/** Vibe lifecycle entry for source generation. @scope spec://org.vibevm.zap/lens/PROP-016#lifecycle */
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { buildSourceInstallation } from "./build-core.mjs";
import { loadBuildInput } from "./contract.mjs";
import { forward } from "./support.mjs";

export async function runSourceInstallBuild(environment = process.env, ports = {}) {
  const lensRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  const input = await loadBuildInput(environment, lensRoot);
  const result = await buildSourceInstallation({ ...input, lensRoot }, ports);
  await writeReply(input.replyPath, sourceInstallReply(input.projectRoot, result));
  return result;
}

export function sourceInstallReply(projectRoot, result) {
  return {
    artifacts: [
      {
        id: "zap-source-install-prepared",
        kind: "file",
        path: forward(resolve(projectRoot, "target/zap-source-install/prepared.json")),
      },
      {
        id: "zap-source-install-launchers",
        kind: "directory",
        path: forward(resolve(projectRoot, "target/zap-source-install/launchers")),
      },
    ],
    envelope: 1,
    message: result.reused
      ? `reused Zap source generation ${result.generation.generationId}`
      : `prepared Zap source generation ${result.generation.generationId}`,
    status: "ok",
    tasks: [],
  };
}

async function main() {
  try {
    await runSourceInstallBuild();
  } catch (error) {
    const message = safeMessage(error);
    const reply = process.env.VIBE_REPLY;
    if (typeof reply === "string" && reply.length > 0) {
      try {
        await writeReply(resolve(reply), {
          artifacts: [],
          envelope: 1,
          message,
          status: "fail",
          tasks: [],
        });
      } catch {
        // The original bounded diagnostic remains the authoritative failure.
      }
    }
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  }
}

async function writeReply(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify(value), { encoding: "utf8", flag: "w" });
}

function safeMessage(error) {
  const raw = error instanceof Error ? error.message : "source-install build failed";
  return raw
    .replace(/[\r\n\t]+/gu, " ")
    .replace(/[\u0000-\u001f]/gu, "")
    .slice(0, 4_000);
}

if (
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
)
  await main();
