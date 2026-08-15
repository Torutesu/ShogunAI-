// Desktop lint gate (#119). Scope: correctness classes typecheck cannot see — above all the React
// Hooks rules, because the overlay bugs this repo has actually shipped (#120/#122: timers reset by
// changing deps, effects leaking on unmount) are exactly what react-hooks flags. Style is left to
// rustfmt-culture: no formatting rules here, so the gate stays cheap to keep green.
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  { ignores: ["dist/", "node_modules/", "src-tauri/"] },
  {
    files: ["src/**/*.{ts,tsx}", "test/**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // Hooks v6's new analyses flag ~40 pre-existing sites (refs-in-render, setState-in-effect,
      // immutability). Those are precisely the #120/#122 work — real smells, wrong day to block
      // on them. Kept VISIBLE as warnings so the count can only shrink; the classic correctness
      // rules below stay hard errors from day one.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      "react-hooks/set-state-in-effect": "warn",
      "react-hooks/refs": "warn",
      "react-hooks/immutability": "warn",
      "react-hooks/preserve-manual-memoization": "warn",
      // The IPC boundary speaks loosely-typed payloads; `any` at the edge is explicit and local.
      "@typescript-eslint/no-explicit-any": "off",
      // fire-and-forget `void invoke(...)` is the house idiom for IPC casts
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
);
