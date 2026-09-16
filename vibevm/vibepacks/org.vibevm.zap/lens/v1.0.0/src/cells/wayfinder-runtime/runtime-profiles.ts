/** Coordinator profile identity validation for runtime composition. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import type { WayfinderRuntimeConfig } from "./runtime-config.ts";
import type { WayfinderRuntimeOptions } from "./types.ts";

export function validateRuntimeProfileIds(
  config: WayfinderRuntimeConfig,
  options: WayfinderRuntimeOptions,
): string | null {
  const configured = new Set(
    config.providerCoordinatorProfiles.map((profile) => profile.profileId),
  );
  const injected = options.hosts?.flatMap((host) => host.profileIds) ?? [];
  return new Set(injected).size !== injected.length ||
    injected.some((profileId) => configured.has(profileId))
    ? "injected and configured provider profile IDs must be unique"
    : null;
}

export function knownRuntimeProfileIds(
  config: WayfinderRuntimeConfig,
  profiles: readonly CodexCoordinatorProfile[],
): string[] {
  return [
    ...profiles.map((profile) => profile.profileId),
    ...config.providerCoordinatorProfiles.map((profile) => profile.profileId),
  ];
}
