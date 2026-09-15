import js from "@eslint/js";
import aiNative from "./tooling/eslint-ai-native.js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["node_modules/**", "target/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  {
    files: ["src/**/*.ts"],
    languageOptions: {
      parserOptions: {
        project: ["./tsconfig.json", "./tsconfig.test.json"],
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      "ai-native": aiNative,
    },
    rules: {
      "ai-native/diagnostic-cites-req": "error",
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      // typescript-eslint 8.46 crashes on generic union aliases under TS 5.9.
      // The compiler still checks signatures; keep the rest of strictTypeChecked active.
      "@typescript-eslint/unified-signatures": "off",
    },
  },
  {
    files: ["src/**/*.test.ts", "src/**/*.vitest.ts"],
    rules: {
      // Node's test registrar and assertion control-flow types trigger false positives;
      // production source keeps every strict rule above.
      "@typescript-eslint/no-floating-promises": "off",
      "@typescript-eslint/no-unnecessary-condition": "off",
      "@typescript-eslint/require-await": "off",
      "@typescript-eslint/no-deprecated": "off",
      "@typescript-eslint/restrict-template-expressions": "off",
      "@typescript-eslint/no-confusing-void-expression": "off",
    },
  },
);
