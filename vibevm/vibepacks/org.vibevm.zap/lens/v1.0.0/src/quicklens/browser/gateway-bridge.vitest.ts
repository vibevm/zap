import { expect, test } from "vitest";

import { createQuicklensDemoDataSource } from "../../cells/quicklens-demo/index.ts";
import { createBrowserGatewayBridge } from "./gateway-bridge.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-002#acceptance */
test("browser gateway pairs once, scrubs its fragment and uses fixed credentialed routes", async () => {
  const demo = await createQuicklensDemoDataSource().read({
    signal: new AbortController().signal,
  });
  if (!demo.ok) throw new Error(demo.error.message);
  const calls: Array<{ readonly url: string; readonly init: RequestInit }> = [];
  let paired = false;
  const fetcher = async (url: string, init: RequestInit): Promise<Response> => {
    calls.push({ url, init });
    if (url.endsWith("/v1/pair")) {
      paired = true;
      return Response.json({ ok: true, value: { paired: true } });
    }
    if (url.endsWith("/v1/invalidations")) {
      return Response.json({
        ok: true,
        value: { events: [{ sequence: 1, reason: "questions" }], next: 1 },
      });
    }
    return paired
      ? Response.json({ ok: true, value: demo.value })
      : Response.json(
          { ok: false, error: { code: "forbidden", message: "pair", recovery: "pair" } },
          { status: 401 },
        );
  };
  const replacements: string[] = [];
  const bridge = createBrowserGatewayBridge(
    {
      href: "http://localhost:4174/?gateway=http%3A%2F%2Flocalhost%3A43100%2Fquicklens%2Fbrowser#pair=pairing-secret",
    },
    { replaceState: (_data, _unused, url) => replacements.push(String(url)) },
    fetcher,
  );
  expect(bridge).not.toBeNull();
  await expect(bridge?.read()).resolves.toEqual({ ok: true, value: demo.value });
  expect(calls.map((call) => new URL(call.url).pathname)).toEqual([
    "/quicklens/browser/v1/pair",
    "/quicklens/browser/v1/read",
  ]);
  expect(new Headers(calls[0]?.init.headers).get("Authorization")).toBe("Bearer pairing-secret");
  expect(calls[1]?.init.credentials).toBe("include");
  expect(replacements).toEqual(["/?gateway=http%3A%2F%2Flocalhost%3A43100%2Fquicklens%2Fbrowser"]);
  let unsubscribe = (): void => undefined;
  const invalidation = await new Promise<string>((resolve) => {
    unsubscribe = bridge?.subscribe?.(resolve) ?? (() => undefined);
  });
  unsubscribe();
  expect(invalidation).toBe("questions");
  expect(new URL(calls.at(-1)?.url ?? "http://invalid").pathname).toBe(
    "/quicklens/browser/v1/invalidations",
  );
});

test("browser gateway refuses foreign and cross-site endpoints", () => {
  const history = { replaceState: () => undefined };
  expect(
    createBrowserGatewayBridge(
      { href: "http://localhost:4174/?gateway=https%3A%2F%2Fexample.com%2F" },
      history,
    ),
  ).toBeNull();
  expect(
    createBrowserGatewayBridge(
      { href: "http://localhost:4174/?gateway=http%3A%2F%2F127.0.0.1%3A43100%2F" },
      history,
    ),
  ).toBeNull();
});
