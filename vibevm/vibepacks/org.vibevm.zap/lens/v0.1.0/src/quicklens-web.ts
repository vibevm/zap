#!/usr/bin/env node
/**
 * Explicit remote Quicklens profile; tunnel provisioning remains external.
 * @scope spec://org.vibevm.zap/lens/PROP-003#deployment-profile
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { homedir } from "node:os";
import { pathToFileURL } from "node:url";
import { z } from "zod";

import {
  QuicklensSourceRuntimeConfigSchema,
  openQuicklensSourceRuntime,
} from "./cells/quicklens-service/index.ts";
import { defaultPasswordVerifierPath, loadPasswordAuthenticator } from "./cells/web-auth/index.ts";
import { WebOperationSchema, createQuicklensWebGateway } from "./cells/web-gateway/index.ts";

export const QuicklensWebRuntimeConfigSchema = z
  .object({
    protocol: z.literal("quicklens-web/1"),
    source: QuicklensSourceRuntimeConfigSchema,
    auth: z
      .object({
        verifierPath: z.string().min(1).optional(),
        maximumConcurrentKdf: z.number().int().min(1).max(8).default(2),
      })
      .strict(),
    gateway: z
      .object({
        rendererRoot: z.string().min(1),
        publicOrigin: z.url(),
        proxyProofToken: z.string().min(24).max(512),
        host: z.enum(["127.0.0.1", "localhost", "::1"]),
        port: z.number().int().min(0).max(65_535),
        role: z.enum(["viewer", "operator", "owner"]),
        allowedOperations: z.array(WebOperationSchema).min(1).max(8),
        sessionTtlMilliseconds: z.number().int().min(60_000).max(86_400_000).optional(),
        maximumSessions: z.number().int().min(1).max(1_000).optional(),
        maximumLoginAttempts: z.number().int().min(1).max(100).optional(),
        loginWindowMilliseconds: z.number().int().min(1_000).max(3_600_000).optional(),
      })
      .strict(),
  })
  .strict();
export type QuicklensWebRuntimeConfig = z.infer<typeof QuicklensWebRuntimeConfigSchema>;

export async function runQuicklensWeb(
  environment: NodeJS.ProcessEnv,
  write: (line: string) => void,
): Promise<number> {
  const path = environment["QUICKLENS_WEB_CONFIG_FILE"];
  if (path === undefined) return 2;
  try {
    const absoluteConfig = resolve(path);
    const raw: unknown = JSON.parse(readFileSync(absoluteConfig, "utf8"));
    const config = QuicklensWebRuntimeConfigSchema.safeParse(raw);
    if (!config.success) return 2;
    const directory = dirname(absoluteConfig);
    const source = await openQuicklensSourceRuntime(config.data.source, {
      configDirectory: directory,
    });
    if (!source.ok) return 1;
    const verifierPath =
      config.data.auth.verifierPath === undefined
        ? defaultPasswordVerifierPath(homedir())
        : resolve(directory, config.data.auth.verifierPath);
    const authenticator = await loadPasswordAuthenticator(verifierPath, {
      maximumConcurrent: config.data.auth.maximumConcurrentKdf,
    });
    if (!authenticator.ok) {
      source.value.close();
      return 1;
    }
    const gateway = createQuicklensWebGateway({
      source: source.value.source,
      authenticator: authenticator.value,
      rendererRoot: resolve(directory, config.data.gateway.rendererRoot),
      publicOrigin: config.data.gateway.publicOrigin,
      proxyProofToken: config.data.gateway.proxyProofToken,
      role: config.data.gateway.role,
      allowedOperations: config.data.gateway.allowedOperations,
      ...(config.data.gateway.sessionTtlMilliseconds === undefined
        ? {}
        : { sessionTtlMilliseconds: config.data.gateway.sessionTtlMilliseconds }),
      ...(config.data.gateway.maximumSessions === undefined
        ? {}
        : { maximumSessions: config.data.gateway.maximumSessions }),
      ...(config.data.gateway.maximumLoginAttempts === undefined
        ? {}
        : { maximumLoginAttempts: config.data.gateway.maximumLoginAttempts }),
      ...(config.data.gateway.loginWindowMilliseconds === undefined
        ? {}
        : { loginWindowMilliseconds: config.data.gateway.loginWindowMilliseconds }),
    });
    if (!gateway.ok) {
      source.value.close();
      return 1;
    }
    const started = await gateway.value.start({
      host: config.data.gateway.host,
      port: config.data.gateway.port,
    });
    if (!started.ok) {
      await gateway.value.close();
      source.value.close();
      return 1;
    }
    write(
      JSON.stringify({
        protocol: "quicklens-web/1",
        ok: true,
        listener: { host: started.value.address, port: started.value.port },
        publicOrigin: config.data.gateway.publicOrigin,
        webUrl: `${config.data.gateway.publicOrigin}/?web=1`,
      }),
    );
    const close = (): void => {
      void gateway.value.close().finally(() => {
        source.value.close();
      });
    };
    process.once("SIGINT", close);
    process.once("SIGTERM", close);
    return 0;
  } catch {
    return 2;
  }
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await runQuicklensWeb(process.env, console.log);
}
