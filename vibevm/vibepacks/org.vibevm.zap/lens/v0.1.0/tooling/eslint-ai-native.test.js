import assert from "node:assert/strict";
import { test } from "node:test";

import parser from "@typescript-eslint/parser";
import { Linter } from "eslint";

import { diagnosticCitesReq, matchesReqGrammar } from "./eslint-ai-native.js";

function lint(code) {
  return new Linter().verify(code, {
    plugins: { "ai-native": { rules: { "diagnostic-cites-req": diagnosticCitesReq } } },
    languageOptions: { parser },
    rules: { "ai-native/diagnostic-cites-req": "error" },
  });
}

test("vendored rule preserves constructor and thrown-object coverage", () => {
  for (const code of [
    `throw new Error("boom");`,
    `const e = new PlanError("plain failure");`,
    `throw { message: "nope" };`,
    `throw new TypeError("bad type");`,
  ]) {
    const messages = lint(code);
    assert.equal(messages.length, 1, `expected one finding for ${code}`);
    assert.equal(matchesReqGrammar(messages[0]?.message ?? ""), true);
  }
});

test("vendored rule accepts each canonical grammar scheme", () => {
  for (const scheme of ["spec://a/b#c", "discipline://a/b#c", "misra://a/b#c"]) {
    assert.deepEqual(
      lint(`throw { message: "violates REQ ${scheme}: why; fix surface: here" };`),
      [],
    );
  }
});
