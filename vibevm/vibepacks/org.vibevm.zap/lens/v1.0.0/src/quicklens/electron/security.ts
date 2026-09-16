/**
 * Pure custom-protocol containment check used by the Electron main process.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shells
 */
import { existsSync } from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";

export function resolveRendererAsset(
  browserRoot: string,
  rawUrl: string,
): { readonly ok: true; readonly file: string } | { readonly ok: false } {
  const url = new URL(rawUrl);
  if (url.protocol !== "quicklens:" || url.host !== "app") return { ok: false };
  const requested = decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname);
  const file = resolve(browserRoot, `.${requested}`);
  const fromRoot = relative(browserRoot, file);
  return fromRoot.startsWith("..") || isAbsolute(fromRoot) || !existsSync(file)
    ? { ok: false }
    : { ok: true, file };
}
