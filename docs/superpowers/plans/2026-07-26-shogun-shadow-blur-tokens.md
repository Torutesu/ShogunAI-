# SHOGUN shadow/blur トークン化 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** desktop が直書きしている構造ガラスの shadow/blur（4値）を `@shogun-ai/tokens` の `static` に抽出し、desktop の6箇所の直書き（重複2ペア含む）を `var(--…)` 参照へ置換する（見た目不変）。

**Architecture:** `tokens.json` の `static` に `blur`/`blur-sm`/`shadow`/`shadow-sm`（desktop 実値）を加算。`build.mjs` は無変更（`staticBlock` が base `:root` に出力）。desktop `styles.css` の直書きを `var()` に置換。値は同一のため描画不変。

**Tech Stack:** Node ESM + `node --test`（tokens）、Vite（desktop）、pnpm workspaces + turbo。

---

## 前提と正本の値（desktop styles.css より確定）

| token | 値（desktop 実値そのまま） | 現行使用箇所 |
|---|---|---|
| `blur` | `blur(38px) saturate(1.8)` | 101-102（展開パネル） |
| `blur-sm` | `blur(30px) saturate(1.7)` | 51-52, 927-928（handle/mpill、重複） |
| `shadow` | `0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)` | 106（パネル影） |
| `shadow-sm` | `0 10px 26px rgba(0, 0, 0, 0.4)` | 56, 932（小サーフェス影、重複） |

据え置き（対象外）: 127（live-dot グロー `color-mix … var(--live)`）、410（focus ring `0 0 0 3px var(--accent-soft)`）、510（button `0 4px 13px var(--accent-soft)`）、516（`none`）。

`tokens.json` の `static` は現在 `"accent-soft"` で終わる。その後ろに4トークンを追加する。

---

## File Structure

- 変更: `packages/tokens/src/tokens.json` — `static` に4トークン追加
- 変更: `packages/tokens/scripts/build.test.mjs` — 4トークンの生成確認テスト追加
- 変更: `apps/desktop/src/styles.css` — 直書き6箇所を `var()` 置換
- 変更: `packages/tokens/README.md` — Shadow/Blur の記述へ更新
- 無変更: `packages/tokens/scripts/build.mjs`（`staticBlock` が汎用処理）

---

## Task 1: tokens.json に shadow/blur static を追加（TDD）

**Files:**
- Modify: `packages/tokens/src/tokens.json`
- Test: `packages/tokens/scripts/build.test.mjs`

- [ ] **Step 1: 失敗するテストを追加**

`packages/tokens/scripts/build.test.mjs` の末尾に追記（`generateCss`, `_read`, `_resolve`, `PKG_ROOT` は既存の import/定義を再利用）:
```js
test("generateCss emits the shadow/blur static tokens from real tokens.json", () => {
  const t = JSON.parse(_read(_resolve(PKG_ROOT, "src/tokens.json"), "utf8"));
  const css = generateCss(t);
  const base = css.slice(0, css.indexOf("}")); // base :root block
  assert.match(base, /--blur:\s*blur\(38px\) saturate\(1\.8\)/);
  assert.match(base, /--blur-sm:\s*blur\(30px\) saturate\(1\.7\)/);
  assert.match(base, /--shadow-sm:\s*0 10px 26px rgba\(0, 0, 0, 0\.4\)/);
  assert.match(base, /--shadow:\s*0 28px 66px rgba\(0, 0, 0, 0\.5\), inset 0 1px 0 rgba\(255, 255, 255, 0\.06\)/);
});
```

- [ ] **Step 2: テスト失敗を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: 新規テストが FAIL（tokens.json にまだ4トークンが無い）。

- [ ] **Step 3: tokens.json の static に4トークンを追加**

`packages/tokens/src/tokens.json` の `static` セクションで、`"accent-soft": "color-mix(in srgb, var(--accent) 26%, transparent)"` の行の**末尾にカンマを付け**、その直後に以下4行を追加する（`static` の閉じ `}` の前）:
```json
    "blur": "blur(38px) saturate(1.8)",
    "blur-sm": "blur(30px) saturate(1.7)",
    "shadow": "0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)",
    "shadow-sm": "0 10px 26px rgba(0, 0, 0, 0.4)"
```
JSON が妥当であること（`accent-soft` 行末のカンマ追加を忘れない）。`themed` / `web` セクションは触らない。

- [ ] **Step 4: テスト通過を確認**

Run: `cd packages/tokens && node --test scripts/build.test.mjs`
Expected: 全テスト PASS。

- [ ] **Step 5: 実ビルドで dist に出力されることを確認**

Run: `cd packages/tokens && rm -rf dist && pnpm build && node -e "const c=require('fs').readFileSync('dist/tokens.css','utf8'); for(const t of ['--blur: blur(38px) saturate(1.8)','--blur-sm: blur(30px) saturate(1.7)','--shadow-sm: 0 10px 26px rgba(0, 0, 0, 0.4)','--shadow: 0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)']) if(!c.includes(t)){console.error('MISSING',t);process.exit(1)} console.log('shadow/blur all present')"`
Expected: `shadow/blur all present`

- [ ] **Step 6: Commit**

```bash
git add packages/tokens/src/tokens.json packages/tokens/scripts/build.test.mjs
git commit -m "feat(tokens): shadow/blur を static トークンに追加(desktop実値)"
```

---

## Task 2: desktop styles.css の直書きを var() へ置換

**Files:**
- Modify: `apps/desktop/src/styles.css`

- [ ] **Step 1: blur-sm（重複2箇所）を置換**

`apps/desktop/src/styles.css` で以下2種の文字列を**全て**（各2回出現、計4行）置換する:
- `-webkit-backdrop-filter: blur(30px) saturate(1.7);` → `-webkit-backdrop-filter: var(--blur-sm);`
- `backdrop-filter: blur(30px) saturate(1.7);` → `backdrop-filter: var(--blur-sm);`

（Edit ツールなら `replace_all: true` を使う。両方のベンダー行を必ず置換すること。）

- [ ] **Step 2: blur（1箇所）を置換**

- `-webkit-backdrop-filter: blur(38px) saturate(1.8);` → `-webkit-backdrop-filter: var(--blur);`
- `backdrop-filter: blur(38px) saturate(1.8);` → `backdrop-filter: var(--blur);`

- [ ] **Step 3: shadow-sm（重複2箇所）を置換**

- `box-shadow: 0 10px 26px rgba(0, 0, 0, 0.4);` → `box-shadow: var(--shadow-sm);`（2回出現、`replace_all: true`）

- [ ] **Step 4: shadow（1箇所）を置換**

- `box-shadow: 0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06);` → `box-shadow: var(--shadow);`

- [ ] **Step 5: 構造ガラスの直書きが残っていないことを確認**

Run: `grep -nE "blur\(30px\)|blur\(38px\)|0 10px 26px|0 28px 66px" apps/desktop/src/styles.css`
Expected: 出力なし（全て `var()` に置換済み）。

- [ ] **Step 6: 置換した var 参照が入っていることを確認 + 据え置き対象が残っていることを確認**

Run: `grep -nE "var\(--blur\)|var\(--blur-sm\)|var\(--shadow\)|var\(--shadow-sm\)" apps/desktop/src/styles.css`
Expected: `var(--blur-sm)` ×4、`var(--blur)` ×2、`var(--shadow-sm)` ×2、`var(--shadow)` ×1 が並ぶ。
Run: `grep -nE "var\(--accent-soft\)|color-mix\(in srgb, var\(--live\)" apps/desktop/src/styles.css`
Expected: 410/510 の accent-soft 影と 127 の live グローが**残っている**（据え置き対象）。

- [ ] **Step 7: desktop がビルドできることを確認（var 解決）**

Run: `pnpm --filter @shogun-ai/tokens build && pnpm --filter @shogun-ai/desktop build:vite`
Expected: tokens が dist を生成後、desktop の tsc + vite build が成功（`var(--blur)` 等が解決、`@shogun-ai/tokens/css` から供給）。エラーなし。
> desktop の `dev`/`build` は Tauri（ネイティブ）を起動し得るため、フロント検証には `build:vite` を用いる。PRE-EXISTING で無関係な失敗があれば DONE_WITH_CONCERNS で報告。

- [ ] **Step 8: Commit**

```bash
git add apps/desktop/src/styles.css
git commit -m "refactor(desktop): 構造ガラスの shadow/blur を var(--…) 参照へ(見た目不変)"
```

---

## Task 3: README 更新 + 最終検証

**Files:**
- Modify: `packages/tokens/README.md`

- [ ] **Step 1: README の Shadow/Spacing 節を更新**

`packages/tokens/README.md` の以下の節（Foundation ブランチで書いた「未定義」注記）:
```markdown
### Shadow / Spacing
本 Foundation では未定義（desktop 正本に無いため）。将来のブランチで wireframe（`docs/wireframes/shogun-ui.css`）の shadow/blur を統合する。
```
を、次に置き換える:
```markdown
### Shadow / Blur（static）
`--shadow: 0 28px 66px rgba(0, 0, 0, 0.5), inset 0 1px 0 rgba(255, 255, 255, 0.06)`（展開パネル）/ `--shadow-sm: 0 10px 26px rgba(0, 0, 0, 0.4)`（小サーフェス）
`--blur: blur(38px) saturate(1.8)`（展開パネル）/ `--blur-sm: blur(30px) saturate(1.7)`（小サーフェス）
値は desktop 実値。light/dark 非依存のため static（base `:root` のみ）。`--blur*` は `backdrop-filter` 用。

### Spacing
本 Foundation では未定義（desktop 正本に汎用 spacing スケールが無いため）。必要になれば後続ブランチで追加する。
```

- [ ] **Step 2: README に古い注記が残っていないことを確認**

Run: `grep -n "未定義（desktop 正本に無いため）" packages/tokens/README.md`
Expected: 出力なし（Shadow の旧注記が消えている。Spacing 側の新しい注記は文言が異なる）。
Run: `grep -n "Shadow / Blur" packages/tokens/README.md`
Expected: 新節がヒット。

- [ ] **Step 3: パッケージ最終検証**

Run: `cd /Users/torutano/ShogunAI- && pnpm --filter @shogun-ai/tokens test && pnpm --filter @shogun-ai/tokens build && ls packages/tokens/dist`
Expected: 全テスト PASS、`dist/{tokens.css,tokens.ts,tokens.web.css}` 生成。

- [ ] **Step 4: Commit**

```bash
git add packages/tokens/README.md
git commit -m "docs(tokens): README に shadow/blur トークンの記述を追加"
```

---

## 完了条件（Definition of Done）

1. `tokens.json` の `static` に `blur`/`blur-sm`/`shadow`/`shadow-sm` が desktop 実値で追加されている。
2. `dist/tokens.css` の base `:root` に4トークンが出力される。
3. desktop `styles.css` の構造ガラス直書き6箇所が `var()` に置換され、`blur(30px)`/`blur(38px)`/`0 10px 26px`/`0 28px 66px` が残っていない。据え置き対象（127/410/510）は残る。
4. `pnpm --filter @shogun-ai/tokens test` 全 pass、`pnpm --filter @shogun-ai/desktop build:vite` 成功、見た目不変。
5. `build.mjs` ロジック・website・product themed は無変更。

## 申し送り

- 本ブランチは website-tokens（#15）にスタック。マージ順は #14 → #15 → 本ブランチ。
- 最終的に desktop（Notch/展開パネル/mpill/設定）の目視スポットチェック推奨（値は同一）。
- 後続: Components（`packages/ui`）。component 級の影が要れば semantic 化を検討。
