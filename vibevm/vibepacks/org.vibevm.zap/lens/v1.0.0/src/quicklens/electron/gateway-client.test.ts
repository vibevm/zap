import assert from "node:assert/strict";
import test from "node:test";

import { createElectronGatewayClient } from "./gateway-client.ts";

test("Electron main pairs once and keeps the session cookie behind fixed routes", async () => {
  const calls: Array<{ readonly url: string; readonly headers: Headers }> = [];
  let paired = false;
  const client = createElectronGatewayClient({
    baseUrl: "http://127.0.0.1:43100/quicklens/electron",
    pairingToken: "pairing-token-with-at-least-24-characters",
    origin: "quicklens://app",
    fetcher: async (url, init) => {
      const headers = new Headers(init.headers);
      calls.push({ url, headers });
      if (url.endsWith("/v1/pair")) {
        paired = true;
        return Response.json(
          { ok: true, value: { paired: true } },
          {
            headers: {
              "Set-Cookie":
                "quicklens_session_electron=abcdefghijklmnopqrstuvwx12345678; HttpOnly; SameSite=Strict; Path=/quicklens/electron",
            },
          },
        );
      }
      if (url.endsWith("/v1/invalidations")) {
        return Response.json({
          ok: true,
          value: { events: [{ sequence: 1, reason: "plan" }], next: 1 },
        });
      }
      return paired && headers.has("Cookie")
        ? Response.json({ ok: true, value: { sourceMode: "live" } })
        : Response.json(
            { ok: false, error: { code: "forbidden", message: "pair", recovery: "pair" } },
            { status: 401 },
          );
    },
  });
  assert.ok(client !== null);
  assert.deepEqual(await client.read(), { ok: true, value: { sourceMode: "live" } });
  assert.deepEqual(
    calls.map((call) => new URL(call.url).pathname),
    ["/quicklens/electron/v1/pair", "/quicklens/electron/v1/read"],
  );
  assert.equal(
    calls[0]?.headers.get("Authorization"),
    "Bearer pairing-token-with-at-least-24-characters",
  );
  assert.equal(
    calls[1]?.headers.get("Cookie"),
    "quicklens_session_electron=abcdefghijklmnopqrstuvwx12345678",
  );
  assert.equal(calls[1]?.headers.get("Origin"), "quicklens://app");
  let unsubscribe = (): void => undefined;
  const invalidation = await new Promise<string>((resolve) => {
    unsubscribe = client.subscribe(resolve);
  });
  unsubscribe();
  assert.equal(invalidation, "plan");
  assert.equal(
    new URL(calls.at(-1)?.url ?? "http://invalid").pathname,
    "/quicklens/electron/v1/invalidations",
  );
});

test("Electron gateway refuses non-loopback endpoints", () => {
  assert.equal(
    createElectronGatewayClient({
      baseUrl: "https://example.com/quicklens/electron",
      pairingToken: "pairing-token-with-at-least-24-characters",
      origin: "quicklens://app",
      fetcher: () =>
        Promise.reject(
          new Error(
            "violates REQ spec://org.vibevm.zap/lens/PROP-002#shells: foreign gateway reached fetch; fix surface: reject the endpoint before transport",
          ),
        ),
    }),
    null,
  );
});
