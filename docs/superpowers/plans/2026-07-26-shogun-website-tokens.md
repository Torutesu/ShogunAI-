# SHOGUN website トークン統合 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** マーケサイト `apps/website` の生トークン定義を `@shogun-ai/tokens` の `web` セットへ引っ越し、`dist/tokens.web.css` を生成して website がそれを参照する（見た目不変・値そのまま）。

**Architecture:** `tokens.json` に `web.themed`（18トークン, light/dark）を加算し、`build.mjs` に web 専用ジェネレータ `generateWebCss`（light 基準・`data-theme` 切替・system-dark の3ブロック）を追加。`validate` は web をモード存在のみ検証（非色トークンを含むため color 検証はしない）。product 経路・desktop・`tokens.ts` は無変更。website の `globals.css` から旧3ブロックを削除し `@import` に置換。

**Tech Stack:** Node ESM（`.mjs`）+ `node --test`、Tailwind v4 + Next 16（website）、pnpm workspaces + turbo。

---

## 前提と正本の値（globals.css 7〜71行より確定）

`web.themed` の各キーは `{ "light": <:root値>, "dark": <dark値> }`。dark と system-`@media` は同値。

| token | light | dark |
|---|---|---|
| bg | `#ffffff` | `#090b0d` |
| surface | `#ffffff` | `#14181b` |
| cloud | `#f7fdff` | `#10151a` |
| ink | `#090b0c` | `#eef2f4` |
| on-ink | `#fafafa` | `#090b0d` |
| muted | `#5f6b73` | `#97a3ac` |
| faint | `#9aa3a9` | `#6b7780` |
| border | `#e5e7eb` | `#262d33` |
| sky | `#97e5ff` | `#2a7ba3` |
| sky-soft | `#d8f6ff` | `#103245` |
| accent | `#00a6f4` | `#38bdf8` |
| accent-strong | `#0089cf` | `#7dd3fc` |
| danger | `#ef4444` | `#f87171` |
| band | `#090b0c` | `#05070a` |
| band-ink | `#ffffff` | `#ffffff` |
| orb-blend | `multiply` | `screen` |
| orb-opacity | `0.55` | `0.4` |
| hairline | `rgba(9, 11, 12, 0.06)` | `rgba(255, 255, 255, 0.06)` |

生成する web CSS のブロック構造（現行を忠実再現）:
```
:root { <web light> }
:root[data-theme='dark'] { <web dark> }
@media (prefers-color-scheme: dark) {
  :root:not([data-theme='light']) { <web dark> }
}
```

> product 側（`static`/`themed` → `dist/tokens.css`）と `dist/tokens.ts` は本計画で一切変更しない。

---

## File Structure

- 変更: `packages/tokens/src/tokens.json` — `web.themed` を追加、`$website_mapping` を削除
- 変更: `packages/tokens/scripts/build.mjs` — `validate` に web 検証追加、`generateWebCss` 追加、`main` が `dist/tokens.web.css` も出力
- 変更: `packages/tokens/scripts/build.test.mjs` — web の validate / generateWebCss / main 出力のテスト追加
- 変更: `packages/tokens/package.json` — `exports` に `"./web.css"` 追加
- 変更: `packages/tokens/README.md` — web セットの記述を追加
- 変更: `apps/website/package.json` — `@shogun-ai/tokens` 依存追加
- 変更: `apps/website/src/app/globals.css` — 旧3トークンブロック（7〜71行）を `@import` に置換

---

## Task 1: `tokens.json` に web セットを追加、$website_mapping を削除

**Files:**
- Modify: `packages/tokens/src/tokens.json`

- [ ] **Step 1: `$website_mapping` を削除し `web` を追加**

`packages/tokens/src/tokens.json` の末尾の `"$website_mapping": { ... }` ブロック全体を削除し、代わりに以下の `"web"` セクションを（`"themed"` セクションの後ろ、トップレベルに）追加する。JSON 全体が妥当であること（末尾カンマに注意）:
```json
  "web": {
    "themed": {
      "bg":            { "light": "#ffffff", "dark": "#090b0d" },
      "surface":       { "light": "#ffffff", "dark": "#14181b" },
      "cloud":         { "light": "#f7fdff", "dark": "#10151a" },
      "ink":           { "light": "#090b0c", "dark": "#eef2f4" },
      "on-ink":        { "light": "#fafafa", "dark": "#090b0d" },
      "muted":         { "light": "#5f6b73", "dark": "#97a3ac" },
      "faint":         { "light": "#9aa3a9", "dark": "#6b7780" },
      "border":        { "light": "#e5e7eb", "dark": "#262d33" },
      "sky":           { "light": "#97e5ff", "dark": "#2a7ba3" },
      "sky-soft":      { "light": "#d8f6ff", "dark": "#103245" },
      "accent":        { "light": "#00a6f4", "dark": "#38bdf8" },
      "accent-strong": { "light": "#0089cf", "dark": "#7dd3fc" },
      "danger":        { "light": "#ef4444", "dark": "#f87171" },
      "band":          { "light": "#090b0c", "dark": "#05070a" },
      "band-ink":      { "light": "#ffffff", "dark": "#ffffff" },
      "orb-blend":     { "light": "multiply", "dark": "screen" },
      "orb-opacity":   { "light": "0.55", "dark": "0.4" },
      "hairline":      { "light": "rgba(9, 11, 12, 0.06)", "dark": "rgba(255, 255, 255, 0.06)" }
    }
  }
```

- [ ] **Step 2: JSON 妥当性と値の照合**

Run: `node -e "const t=JSON.parse(require('fs').readFileSync('packages/tokens/src/tokens.json','utf8')); if(t['\$website_mapping'])throw new Error('mapping残存'); const w=t.web.themed; const need={bg:['#ffffff','#090b0d'],accent:['#00a6f4','#38bdf8'],'orb-blend':['multiply','screen'],'orb-opacity':['0.55','0.4'],hairline:['rgba(9, 11, 12, 0.06)','rgba(255, 255, 255, 0.06)']}; for(const k in need){if(w[k].light!==need[k][0]||w[k].dark!==need[k][1])throw new Error('mismatch '+k)} console.log('web ok, count', Object.keys(w).length)"`
Expected: `web ok, count 18`

さらに `apps/website/src/app/globals.css` の 7〜71 行と値を突き合わせ、全18トークンの light/dark が一致することを目視確認する。差異があれば STOP して報告（値は勝手に変えない）。

- [ ] **Step 3: Commit**

```bash
git add packages/tokens/src/tokens.json
git commit -m "feat(tokens): web トークンセットを追加し誤った\$website_mappingを削除"
```

---

## Task 2: `validate` を web 対応に拡張（TDD）

**Files:**
- Modify: `packages/tokens/scripts/build.mjs`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを追加**

`packages/tokens/scripts/build.test.mjs` の末尾に追記:
```js
test("validate passes for well-formed web tokens (incl. non-color values)", () => {
  const good = {
    static: {}, themed: {},
    web: { themed: {
      bg: { light: "#ffffff", dark: "#090b0d" },
      "orb-blend": { light: "multiply", dark: "screen" },
      "orb-opacity": { light: "0.55", dark: "0.4" },
    } },
  };
  assert.deepEqual(validate(good), []);
});

test("validate flags a web token missing a mode", () => {
  const bad = { static: {}, themed: {}, web: { themed: { bg: { light: "#ffffff" } } } };
  const errors = validate(bad);
  assert.ok(errors.some((e) => e.includes("bg") && e.includes("dark") && e.includes("web")));
});
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: 新規2テストが FAIL（web はまだ検証されず、`orb-blend: multiply` などが未処理／欠損検出なし）。

- [ ] **Step 3: `validate` に web 検証を追加**

`packages/tokens/scripts/build.mjs` の `validate` 関数（`return errors;` の直前）に、web のモード存在チェックを追加する（**color 形式検証はしない**）:
```js
  const web = tokens.web?.themed ?? {};
  for (const [name, byMode] of Object.entries(web)) {
    for (const mode of MODES) {
      if (byMode?.[mode] == null) {
        errors.push(`web token "${name}" is missing mode "${mode}"`);
      }
    }
  }
```
（`MODES` = `["dark","light"]` を再利用。product 側の既存ループは変更しない。）

- [ ] **Step 4: テスト通過を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: 既存 + 新規すべて PASS。

- [ ] **Step 5: Commit**

```bash
git add packages/tokens/scripts/build.mjs packages/tokens/scripts/build.test.mjs
git commit -m "feat(tokens): validate を web トークン(モード存在のみ)に対応"
```

---

## Task 3: `generateWebCss` を実装（TDD）

**Files:**
- Modify: `packages/tokens/scripts/build.mjs`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを追加**

`packages/tokens/scripts/build.test.mjs` の末尾に追記:
```js
import { generateWebCss } from "./build.mjs";

const webSample = {
  web: { themed: {
    bg:     { light: "#ffffff", dark: "#090b0d" },
    accent: { light: "#00a6f4", dark: "#38bdf8" },
  } },
};

test("generateWebCss emits base :root with light values", () => {
  const css = generateWebCss(webSample);
  assert.match(css, /:root\s*\{[^}]*--bg:\s*#ffffff/);
  assert.match(css, /:root\s*\{[^}]*--accent:\s*#00a6f4/);
});

test("generateWebCss emits the data-theme=dark block with dark values", () => {
  const css = generateWebCss(webSample);
  assert.match(css, /:root\[data-theme='dark'\]\s*\{[^}]*--bg:\s*#090b0d/);
});

test("generateWebCss emits the system dark media with :root:not([data-theme='light'])", () => {
  const css = generateWebCss(webSample);
  assert.match(css, /@media \(prefers-color-scheme: dark\)\s*\{\s*:root:not\(\[data-theme='light'\]\)\s*\{[^}]*--accent:\s*#38bdf8/);
});

test("generateWebCss base :root does NOT contain dark bg value", () => {
  const css = generateWebCss(webSample);
  const base = css.slice(css.indexOf(":root {"), css.indexOf("}"));
  assert.doesNotMatch(base, /#090b0d/);
});
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: `generateWebCss is not a function` 系で新規テスト FAIL。

- [ ] **Step 3: `generateWebCss` を実装**

`packages/tokens/scripts/build.mjs` の `generateCss` 関数の直後に追加（既存の `themedBlock` を再利用）:
```js
export function generateWebCss(tokens) {
  const web = tokens.web?.themed ?? {};
  return (
    HEADER +
    `:root {\n${themedBlock(web, "light")}\n}\n` +
    `:root[data-theme='dark'] {\n${themedBlock(web, "dark")}\n}\n` +
    `@media (prefers-color-scheme: dark) {\n  :root:not([data-theme='light']) {\n${themedBlock(web, "dark")
      .split("\n")
      .map((l) => "  " + l)
      .join("\n")}\n  }\n}\n`
  );
}
```

- [ ] **Step 4: テスト通過を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add packages/tokens/scripts/build.mjs packages/tokens/scripts/build.test.mjs
git commit -m "feat(tokens): web CSS 生成(generateWebCss)"
```

---

## Task 4: `main` で web CSS を出力 + exports 追加（TDD）

**Files:**
- Modify: `packages/tokens/scripts/build.mjs`
- Modify: `packages/tokens/package.json`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを追加**

`packages/tokens/scripts/build.test.mjs` の末尾に追記（`PKG_ROOT` は既存定義を再利用）:
```js
test("main also writes dist/tokens.web.css with themed web blocks", () => {
  main();
  const css = _read(_resolve(PKG_ROOT, "dist/tokens.web.css"), "utf8");
  assert.match(css, /:root\s*\{[^}]*--bg:\s*#ffffff/);
  assert.match(css, /:root\[data-theme='dark'\]\s*\{[^}]*--accent:\s*#38bdf8/);
});
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: 新規テストが FAIL（`dist/tokens.web.css` が生成されない）。

- [ ] **Step 3: `main` に web 出力を追加**

`packages/tokens/scripts/build.mjs` の `main()` 内、`writeFileSync(resolve(PKG, "dist/tokens.ts"), generateTs(tokens));` の**直後**に追加し、ログ文字列も更新:
```js
  writeFileSync(resolve(PKG, "dist/tokens.web.css"), generateWebCss(tokens));
```
そして直後の
```js
  console.log("Wrote dist/tokens.css and dist/tokens.ts");
```
を
```js
  console.log("Wrote dist/tokens.css, dist/tokens.web.css and dist/tokens.ts");
```
に変更する。

- [ ] **Step 4: exports に web.css を追加**

`packages/tokens/package.json` の `exports` を次のようにする（`"./ts"` の後ろに1行追加）:
```json
  "exports": {
    "./css": "./dist/tokens.css",
    "./ts": "./dist/tokens.ts",
    "./web.css": "./dist/tokens.web.css",
    "./tokens.json": "./src/tokens.json"
  },
```

- [ ] **Step 5: テスト通過 + 実ビルド確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs && rm -rf dist && pnpm build && ls dist`
Expected: 全テスト PASS、ログに `dist/tokens.web.css` を含む、`ls dist` に `tokens.css  tokens.ts  tokens.web.css`。

- [ ] **Step 6: 生成 web CSS が globals.css の旧値と一致するか確認**

Run:
```bash
cd /Users/torutano/ShogunAI-/packages/tokens
node -e "const c=require('fs').readFileSync('dist/tokens.web.css','utf8'); for (const t of ['--bg: #ffffff','--bg: #090b0d','--accent: #00a6f4','--accent: #38bdf8','--orb-blend: multiply','--orb-blend: screen','--orb-opacity: 0.55','--hairline: rgba(9, 11, 12, 0.06)',\"--accent-strong: #0089cf\"]) if(!c.includes(t)){console.error('MISSING',t);process.exit(1)} console.log('web css all present')"
```
Expected: `web css all present`

- [ ] **Step 7: Commit**

```bash
git add packages/tokens/scripts/build.mjs packages/tokens/scripts/build.test.mjs packages/tokens/package.json
git commit -m "feat(tokens): main が dist/tokens.web.css を生成 + ./web.css エクスポート"
```

---

## Task 5: website を web トークン参照へ切替

**Files:**
- Modify: `apps/website/package.json`
- Modify: `apps/website/src/app/globals.css`

- [ ] **Step 1: website に依存を追加**

`apps/website/package.json` の `dependencies` に以下を追加（既存の並びに合わせる）:
```json
"@shogun-ai/tokens": "workspace:*"
```

- [ ] **Step 2: インストール**

Run: `pnpm install`
Expected: エラーなく完了。

- [ ] **Step 3: globals.css の旧トークンブロックを import に置換**

`apps/website/src/app/globals.css` を編集する:
1. 1行目 `@import 'tailwindcss';` の**直後の行**に `@import '@shogun-ai/tokens/web.css';` を追加。
2. **7〜71行目**の3ブロック（`:root { ... }`、`:root[data-theme='dark'] { ... }`、`@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) { ... } }`）を**削除**する。これらはトークン変数定義のみで構成される。
- KEEP: 3〜6行のコメントブロック、および 73行目以降の `@theme inline { ... }` 以下すべて（fonts / radius / shadow / ease / `@layer base` / utilities / animations）。

編集後、ファイル先頭は次のようになる:
```css
@import 'tailwindcss';
@import '@shogun-ai/tokens/web.css';

/* =====================================================================
   ShogunAI — Aside Skyglass, theme-aware (light + dark + system).
   Semantic tokens flip per theme; components style through the tokens.
   ===================================================================== */

@theme inline {
```
READ してから編集し、削除範囲がトークン定義のみであることを確認する（`@theme` / `@layer` / utilities を消さない）。判断に迷ったら STOP して報告。

- [ ] **Step 4: globals.css に生トークン定義が残っていないことを確認**

Run: `grep -nE "^\s*--(bg|ink|accent|sky|band|hairline|orb-):" apps/website/src/app/globals.css`
Expected: 出力なし（これらは web.css から供給される。`--color-*`（@theme 内）や `--radius-xl`/`--shadow-*`/`--ease-*` は残るのが正しい）。

- [ ] **Step 5: website ビルドで @import 解決とユーティリティ生成を確認（最重要）**

Run: `pnpm --filter @shogun-ai/tokens build && pnpm --filter @shogun-ai/website build`
Expected: tokens が `dist/tokens.web.css` を生成した後、`next build` が成功。`@shogun-ai/tokens/web.css` の解決エラーが出ない。ビルドは完了する。
> これが本計画の最重要検証（Tailwind v4 のパッケージ `@import` 解決）。もし `@import` が解決されずビルドが失敗する場合は、**STOP して失敗ログを添えて報告**すること（勝手に別方式へ切り替えない）。コーディネーターが import 方式を再検討する。

- [ ] **Step 6: 生成 CSS に web トークンが現行値で含まれるか確認**

Run: `grep -roE "\-\-accent:\s*#00a6f4|\-\-bg:\s*#ffffff" apps/website/.next 2>/dev/null | head` （`.next` が対象。存在しなければビルド出力ディレクトリを `ls apps/website` で確認して読み替える）
Expected: `--accent: #00a6f4` および `--bg: #ffffff` が少なくとも1件ヒット（現行値が最終CSSに乗っている）。ヒットしない場合でも Step 5 のビルド成功が主判定。結果を報告する。

- [ ] **Step 7: Commit**

```bash
git add apps/website/package.json apps/website/src/app/globals.css pnpm-lock.yaml
git commit -m "refactor(website): 生トークンを @shogun-ai/tokens/web.css から参照(見た目不変)"
```

---

## Task 6: 等価性確認 + README 追記

**Files:**
- Modify: `packages/tokens/README.md`

- [ ] **Step 1: 削除ブロック ⇔ 生成 web CSS の等価性を独立確認**

`git show <Task5 commit> -- apps/website/src/app/globals.css` の削除行から `--name: value` を抽出し、`packages/tokens/dist/tokens.web.css`（`pnpm --filter @shogun-ai/tokens build` で再生成）の対応ブロックと突き合わせる。3スコープ（light 基準 `:root`、`[data-theme='dark']`、system `@media` の `:root:not([data-theme='light'])`）で **`--name: value` 集合が完全一致**することを確認。差異があれば報告し tokens.json を修正。
> 補足: 現行 globals.css では dark ブロックと system `@media` ブロックが同一値。生成側も同一のはずで、両者一致を確認する。

- [ ] **Step 2: README に web セットの記述を追加**

`packages/tokens/README.md` の「website について」節を、実態に合わせて次の内容へ更新する（旧 `$website_mapping` への言及を削除）:
```markdown
## website セット（web）
`apps/website` は独自パレット（"Skyglass"、light 基準、`data-theme` 切替）を使う。その生トークンは `src/tokens.json` の `web.themed` を正本とし、`dist/tokens.web.css` を生成する。

- 使い方（CSS）: `@import '@shogun-ai/tokens/web.css';`（`@import 'tailwindcss';` の直後）。
- ブロック構造: `:root`（light 基準）/ `:root[data-theme='dark']` / `@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) }`。
- web トークンには非色値（`--orb-blend`, `--orb-opacity`）を含むため、`validate` は web についてはモード存在のみ検証する。
- product（desktop）セットとは別パレット。統一はしない（2テーマ共存）。
```

- [ ] **Step 3: パッケージのテスト・ビルドが通ることを最終確認**

Run: `cd /Users/torutano/ShogunAI- && pnpm --filter @shogun-ai/tokens test && pnpm --filter @shogun-ai/tokens build`
Expected: 全テスト PASS、`dist/{tokens.css,tokens.web.css,tokens.ts}` 生成。

- [ ] **Step 4: Commit**

```bash
git add packages/tokens/README.md
git commit -m "docs(tokens): README に web セットの記述を追加"
```

---

## 完了条件（Definition of Done）

1. `tokens.json` に `web.themed`（18トークン）が入り、`$website_mapping` が削除されている。
2. `pnpm --filter @shogun-ai/tokens build` が `dist/tokens.web.css` を生成し、内容が globals.css の旧3ブロックと完全一致。
3. `pnpm --filter @shogun-ai/tokens test` が全 PASS（product 既存 + web 新規）。
4. website が `pnpm --filter @shogun-ai/website build` で成功し、`@import '@shogun-ai/tokens/web.css'` が解決、Tailwind ユーティリティ生成が従来どおり。見た目不変。
5. product（`dist/tokens.css` / `tokens.ts`）/ desktop は無変更。

## 申し送り

- 本ブランチは `design-system/foundation-tokens`（PR #14）にスタック。PR は #14 マージ後にベースを feat へ張り替えるか、#14 → 本ブランチの順でマージ。
- Tailwind v4 のパッケージ `@import` 解決が万一不可なら、import 方式（配布形態・ビルド前コピー等）を再検討。
- 最終的に website の目視スポットチェックを推奨（等価性検証済みだが念のため）。
- 後続: shadow/blur 統合（product 側）、Components（`packages/ui`）。
