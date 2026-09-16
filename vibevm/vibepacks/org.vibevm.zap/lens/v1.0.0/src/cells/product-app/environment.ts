/** Protected provider environment references. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { readFile } from "node:fs/promises";
import { z } from "zod";
import type { ProtectedEnvironmentPort } from "../managed-work/index.ts";

export function createProtectedEnvironmentResolver(
  files: Readonly<Record<string, string>>,
): ProtectedEnvironmentPort {
  return {
    async resolve(reference) {
      if (reference === null) return { ok: true, value: {} };
      const path = files[reference];
      if (path === undefined)
        return { ok: false, message: "protected environment reference is not configured" };
      try {
        const raw: unknown = JSON.parse(await readFile(path, "utf8"));
        const parsed = z.record(z.string(), z.string()).safeParse(raw);
        return parsed.success
          ? { ok: true, value: parsed.data }
          : { ok: false, message: "protected environment file must contain string values" };
      } catch {
        return { ok: false, message: "protected environment file is unavailable" };
      }
    },
  };
}
