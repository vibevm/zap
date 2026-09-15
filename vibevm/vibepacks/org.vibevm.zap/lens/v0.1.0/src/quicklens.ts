#!/usr/bin/env node
/** Trusted Quicklens entry. @scope spec://org.vibevm.zap/lens/PROP-002#shells */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { startQuicklensRuntime } from "./cells/quicklens-service/runtime.ts";

export async function runQuicklens(
  environment: NodeJS.ProcessEnv,
  write: (line: string) => void,
): Promise<number> {
  const path = environment["QUICKLENS_CONFIG_FILE"];
  if (path === undefined) return 2;
  try {
    const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
    const started = await startQuicklensRuntime(raw, { configDirectory: dirname(resolve(path)) });
    if (!started.ok) return 1;
    write(
      JSON.stringify({
        protocol: "quicklens/1",
        ok: true,
        address: started.value.address,
      }),
    );
    const close = (): void => {
      void started.value.close();
    };
    process.once("SIGINT", close);
    process.once("SIGTERM", close);
    return 0;
  } catch {
    return 2;
  }
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await runQuicklens(process.env, console.log);
}
