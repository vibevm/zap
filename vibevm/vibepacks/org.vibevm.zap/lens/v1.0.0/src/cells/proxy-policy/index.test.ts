import assert from "node:assert/strict";
import test from "node:test";
import { resolveProxyEnvironment } from "./index.ts";

const ambient = {
  HTTP_PROXY: "http://ambient:8080",
  http_proxy: "http://ambient:8080",
  HTTPS_PROXY: "http://ambient:8080",
  https_proxy: "http://ambient:8080",
  ALL_PROXY: "http://ambient:8080",
  all_proxy: "http://ambient:8080",
  NO_PROXY: "example.test",
  no_proxy: "example.test",
};

test("explicit profile proxy overrides ambient casing and preserves local bypass", () => {
  const resolved = resolveProxyEnvironment({
    ambient,
    global: { mode: "explicit", allProxy: "http://global:9000" },
    profile: {
      mode: "explicit",
      httpsProxy: "http://192.168.1.141:10808",
      noProxy: "service.test",
    },
  });
  assert.equal(resolved.source, "explicit");
  assert.equal(resolved.environment["HTTPS_PROXY"], "http://192.168.1.141:10808");
  assert.equal(resolved.environment["https_proxy"], "http://192.168.1.141:10808");
  assert.equal(resolved.environment["ALL_PROXY"], undefined);
  assert.match(resolved.environment["NO_PROXY"] ?? "", /localhost/);
  assert.match(resolved.environment["no_proxy"] ?? "", /127\.0\.0\.1/);
  assert.match(resolved.environment["NO_PROXY"] ?? "", /service\.test/);
});

test("direct mode removes ambient proxies while retaining local bypass", () => {
  const resolved = resolveProxyEnvironment({ ambient, global: { mode: "direct" } });
  assert.equal(resolved.source, "direct");
  assert.equal(resolved.environment["HTTP_PROXY"], undefined);
  assert.equal(resolved.environment["https_proxy"], undefined);
  assert.equal(resolved.environment["NO_PROXY"], "localhost,127.0.0.1,::1,[::1]");
});

test("inherit mode keeps ambient proxies and adds local bypass", () => {
  const resolved = resolveProxyEnvironment({ ambient, global: { mode: "inherit" } });
  assert.equal(resolved.source, "ambient");
  assert.equal(resolved.environment["HTTPS_PROXY"], ambient["HTTPS_PROXY"]);
  assert.match(resolved.environment["no_proxy"] ?? "", /::1/);
});

test("profile inherit preserves an explicit runtime global policy", () => {
  const resolved = resolveProxyEnvironment({
    ambient,
    global: { mode: "explicit", httpsProxy: "http://global:9000" },
    profile: { mode: "inherit" },
  });
  assert.equal(resolved.source, "explicit");
  assert.equal(resolved.environment["HTTPS_PROXY"], "http://global:9000");
});

test("ALL_PROXY supplies HTTP and HTTPS fallbacks", () => {
  const resolved = resolveProxyEnvironment({
    ambient: { ALL_PROXY: "http://all:9000" },
    global: { mode: "inherit" },
  });
  assert.equal(resolved.environment["HTTP_PROXY"], "http://all:9000");
  assert.equal(resolved.environment["HTTPS_PROXY"], "http://all:9000");
});
