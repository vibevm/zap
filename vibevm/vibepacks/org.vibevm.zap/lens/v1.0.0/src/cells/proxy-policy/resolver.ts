/** Deterministic proxy environment resolution. @scope spec://org.vibevm.zap/lens/PROP-010#agent-network */
import { ProxyPolicySchema, type ProxyResolution, type ProxyResolutionInput } from "./types.ts";

const PROXY_KEYS = [
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "ALL_PROXY",
  "NO_PROXY",
  "http_proxy",
  "https_proxy",
  "all_proxy",
  "no_proxy",
] as const;
const LOCAL_NO_PROXY = ["localhost", "127.0.0.1", "::1", "[::1]"];
const PROXY_KEY_SET = new Set<string>(PROXY_KEYS);

export function resolveProxyEnvironment(input: ProxyResolutionInput): ProxyResolution {
  const global = input.global === undefined ? undefined : ProxyPolicySchema.parse(input.global);
  const profile = input.profile === undefined ? undefined : ProxyPolicySchema.parse(input.profile);
  const selected = profile?.mode === "inherit" ? (global ?? profile) : (profile ?? global);
  const ambient = { ...input.ambient };
  if (selected === undefined || selected.mode === "inherit") {
    return {
      mode: "inherit",
      source: "ambient",
      environment: withLocalNoProxy(ambient, selected?.noProxy),
    };
  }
  if (selected.mode === "direct") {
    return {
      mode: "direct",
      source: "direct",
      environment: withLocalNoProxy(removeProxyKeys(ambient), selected.noProxy),
    };
  }
  const clean = removeProxyKeys(ambient);
  const values: Record<string, string | undefined> = {
    ...clean,
    HTTP_PROXY: selected.httpProxy ?? selected.allProxy,
    HTTPS_PROXY: selected.httpsProxy ?? selected.allProxy,
    ALL_PROXY: selected.allProxy,
    http_proxy: selected.httpProxy,
    https_proxy: selected.httpsProxy,
    all_proxy: selected.allProxy,
  };
  return {
    mode: "explicit",
    source: "explicit",
    environment: withLocalNoProxy(values, selected.noProxy),
  };
}

function removeProxyKeys(
  environment: Readonly<Record<string, string | undefined>>,
): Record<string, string | undefined> {
  return Object.fromEntries(
    Object.entries(environment).filter(([key]) => !PROXY_KEY_SET.has(key.toUpperCase())),
  );
}

function withLocalNoProxy(
  environment: Readonly<Record<string, string | undefined>>,
  configured?: string,
): Readonly<Record<string, string | undefined>> {
  const http = firstProxyValue(environment, "HTTP_PROXY");
  const https = firstProxyValue(environment, "HTTPS_PROXY");
  const all = firstProxyValue(environment, "ALL_PROXY");
  const existing = [
    ...Object.entries(environment)
      .filter(([key]) => key.toUpperCase() === "NO_PROXY")
      .map(([, value]) => value),
    configured,
  ]
    .filter((value): value is string => value !== undefined && value.trim().length > 0)
    .join(",");
  const noProxy = [
    ...new Set([
      ...LOCAL_NO_PROXY,
      ...existing
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean),
    ]),
  ].join(",");
  return {
    ...removeProxyKeys(environment),
    HTTP_PROXY: http ?? all,
    http_proxy: http ?? all,
    HTTPS_PROXY: https ?? all,
    https_proxy: https ?? all,
    ALL_PROXY: all,
    all_proxy: all,
    NO_PROXY: noProxy,
    no_proxy: noProxy,
  };
}

function firstProxyValue(
  environment: Readonly<Record<string, string | undefined>>,
  family: string,
): string | undefined {
  const upper = family.toUpperCase();
  return (
    environment[family] ??
    environment[family.toLowerCase()] ??
    Object.entries(environment).find(
      ([key, value]) => key.toUpperCase() === upper && value !== undefined,
    )?.[1]
  );
}
