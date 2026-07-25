# ワイヤーフレーム仕様 — 全画面インベントリ

デザインツール（Paper / Figma / HTML）に依存しない、**描くための1枚の正本**。
既存実装（`apps/desktop/src/App.tsx` / `strings.ts` / `styles.css`）から起こした「今あるもの」と、
設計済みの「これから作るもの」を同じ粒度で並べる。

- 作成日: 2026-07-25
- 設計の根拠: `docs/meeting-context-and-dashboard-design.md` / `docs/meeting-notes-ui-design.md`
- 文言は**実装の `strings.ts` に存在するものはそのまま**。新規画面の文言は英語（v1方針）で本書が初出

---

## 0. トークン（`apps/desktop/src/styles.css` の実値）

ダーク基準。α値は `--glass` の上に合成した平坦値を併記（ワイヤー用）。

| トークン | dark | light | 用途 |
|---|---|---|---|
| `--glass` | `rgba(21,24,31,.85)` → `#15181F` | `rgba(249,250,252,.92)` | パネル地 |
| `--ink` | `#EEF1F5` | `#1B1D22` | 本文 |
| `--muted` | `#9AA3AF` | `#5C616A` | 副次テキスト |
| `--faint` | `#8B95A3` | `#6B7280` | ヒント・メタ |
| `--accent` | `#6EA8FE` | `#2F6FED` | 主アクション |
| `--accent-ink` | `#06101C` | `#FFFFFF` | accent上の文字 |
| `--live` | `#30D158` | `#1F9E3C` | ライブドット |
| `--warn` | `#FF9F0A` | `#A4560A` | 注意・amber |
| `--line` / `--line-strong` | `rgba(255,255,255,.09)` / `.16` → `#2A2E36` / `#383C45` | 黒 `.09` / `.16` | 境界 |
| `--fill` / `--fill-strong` / `--card` | `.06` / `.12` / `.045` → `#22262E` / `#2E323B` / `#1D2028` | 黒同値 | チップ・行 |

- 文字サイズ: `10 / 11 / 12 / 13 / 14`（`--fs-xs`〜`--fs-xl`）。**14px より大きい文字はパネル内に存在しない**
- 角丸: `7 / 10 / 13 / 16 / 20 / 999`
- パネル: `W=560` / `H_OPEN=300` / `H_SETTINGS=460` / `H_HANDLE=44` / 最小 `460×240`
- モーション: `160ms cubic-bezier(.32,.72,0,1)`

---

## A. Notch — 実装済み（`App.tsx`）

### A1. Idle — collapsed handle
ノッチから下がるピル。幅は内容ハグ（フォールバック 260）、高さ 44、角丸 999。

```
[ ● reading Mail ] [ 3 due ] [ 2 waiting ]
```
- `●` = `--live` 7px / "reading" = faint 11px / アプリ名 = ink 11px medium
- 右のチップ = `--fill` 背景・muted 11px
- クリックで Expanded（SLO-01: 100ms）

### A2. Hover — peek
ホバー滞留で出る 300px 幅のプレビュー。**自動展開はしない**。
```
● reading Mail
3 due · 2 waiting
click to open — Open SHOGUN (⌃⌥N)
```

### A3. Expanded — welcome（空スレッド）560×300
```
┌ head ─────────────────────────────────────────────┐
│ (● reading Mail) (3 due · 2 waiting)  ⚙ – ✕       │
├───────────────────────────────────────────────────┤
│              What can I take off your plate?      │
│   Ask about your work, or tap ⌥ (Option) in any   │
│        app to draft where you're typing.          │
│   [ No key yet — add one in settings … ] (warn)   │
├───────────────────────────────────────────────────┤
│ [ Ask SHOGUN…                                   ] │
│ (Anthropic ⌄)        (Draft … (⌥))  [ Send ]      │
└───────────────────────────────────────────────────┘
```

### A4. Expanded — 回答＋出典
- 自分の発話は右寄せ `--fill-strong` の吹き出し
- 回答は左寄せ地なし。**下に出典チップ列**（`Sources` faint 10px ＋ チップ）
- 思考中は 3 ドットのプレースホルダ（`msg--think`）

### A5. Expanded — 追跡中の state
- counts チップを押すと state リストが開く（`showState`）
- 行: `✓` + 本文 + 右端メタ（`due today` / `waiting · 3d`）。行クリックで解決
- 低確度は `possibly:` 接頭・muted。**Low はアクションを提案しない**
- 空: `Nothing tracked yet.`

---

## B. Settings — 実装済み（560×460、パネル内・別窓ではない）

縦 1 カラム、`set` セクションの積み上げ。順序は実装どおり:

| # | セクション | 中身 |
|---|---|---|
| 1 | Appearance | Dark / Light / Auto のセグメント |
| 2 | Shortcuts | Draft = ⌥ 固定（変更不可の明示）/ Show-hide / Quit は再割当可 |
| 3 | Model | provider セグメント（Anthropic / OpenRouter / OpenAI / Gemini）＋ model id 入力 |
| 4 | Your key | 状態行（未設定 / 接続済み / 拒否された）＋ 入力 ＋ Save / Remove。**プロバイダごとに保持** |
| 5 | Memory | 抽出済み state の削除。`CLEAR` タイプ確認つき |
| 6 | Connections | サービスカード（connect / disconnect / coming soon） |
| 7 | AI sessions | Importing / Off のセグメント |
| 8 | Nightly review | 最終実行・events / updates / sent・Run now |
| 9 | Approvals | L3 待ち行列。本文プレビュー＋ Confirm & send / Reject。`third-party (Composio)` バッジ |

> **B は Full UI（D）へ移設する**。パネルに 9 セクションは無理がある（現状は暫定）。

---

## C. Meeting notes — 新規（`docs/meeting-notes-ui-design.md`）

### C1. Offered — 検知直後の 10 秒
```
┌───────────────────────────────────────────┐
│ Weekly sync starting — taking notes in 8s │
│                    [ Not now ]  [ Start ] │
└───────────────────────────────────────────┘
```
- カウントダウンは実数で減る。無視すれば開始
- `Not now` = **今回だけ**。長押しで「このアプリでは今後録らない」

### C2. Recording — 常時可視ピル
```
[ ● Notes · 12:04   Weekly sync            ■ Stop ]
```
- ドットは `--live`（**赤の録画ランプは使わない**＝録画ではない）
- 経過時間は必須。`Stop` は確認なしで即停止
- フルスクリーン中も隠さない唯一の要素

### C3. 会議中の展開パネル 560×340
```
┌ ● Notes · 12:04  Weekly sync        ■ Stop ┐
├────────────────────────────────────────────┤
│ Type your notes…                           │  ← 唯一の主役
│ - pricing decision pending                 │
│ - Alice takes the vendor thread            │
├────────────────────────────────────────────┤
│ Listening · 3 participants      Not now ⌄  │  ← 静かな1行
└────────────────────────────────────────────┘
```
**ライブ文字起こしは表示しない**（集中を削がない／誤りを人に直させない）。

### C4. Recap — 終了直後
```
Weekly sync · 32 min · 4 people
─ Summary ────────────────────────────────
  Vendor renewal is settled at 12k for the year.
  Launch date moved to the 14th.
─ Decisions ──────────────────────────────
  · Renewal at 12k — agreed
─ Picked up ──────────────────────────────
  ✓ You: send the revised deck        [Track]
  ✓ Alice: owns the vendor thread     [Track]
  possibly: launch review on the 14th [Track]
─ Your notes                        [ Open ]
─ From this meeting's transcript · 41 segments   Why? ⌄
```
- `[Track]` を押した瞬間だけ state に確定（confidence 1.0・provenance=ユーザー編集）
- `Why?` で根拠セグメントに降りる

### C5. Off — 三段（設定セクション）
```
Meeting notes            ( Off | Ask me | On )   ← 既定は Off
Audio is processed on this Mac. No recording is saved.
Never in these apps      [ + Add app ]
Never for these events   [ + Add event ]
```

---

## D. Full UI — 新規（別窓。要件 §6.1 の状態機械が前提）

左サイドバー固定（幅 200）＋ 右ペイン。最小 1040×720。

### D1. Today
- Morning Brief（Today / Commitments due / Open loops / What happened / Suggested actions）
- 今日の予定リスト（時刻順）＋ 各行に `Prep` 導線
- 生成できなかった朝は**縮退 Brief**（カレンダー＋overdue のみ、LLM文なし）

### D2. Context Health ★本命
カード 8 枚。**すべてに「直す導線」を必ず1つ**。
| カード | 値の例 | 直す導線 |
|---|---|---|
| Coverage | `18h / 24h captured` | 除外設定を開く |
| Blind spots | `Figma: 2h focused, 3 events` | 除外解除 / 未対応として報告 |
| Freshness | `Mail 3m · Calendar 12m · Slack —` | 再認証 |
| Yield | `1,204 events → 38 candidates → 9 tracked` | 言語設定 |
| Confidence mix | `High 0% / Medium 62% / Low 38%` | 夜間分類の設定 |
| Grounding | `71% of answers cited a source` | 検索窓を広げる |
| Egress | `12 chunks · 84 KB · 0 third-party` | Traceability へ |
| SLO | 6項目の p50/p95 | — |

### D3. Memory
検索窓（FTS＋ベクトル）／ threads ／ state 4テーブル（confidence 帯つき）／ provenance ／ 名寄せの分割修正。

### D4. Sources
サービスカード（状態ドット・鮮度・スコープ・第三者バッジ）＋ キャプチャ除外設定 ＋ AI sessions。

### D5. Activity
実行履歴（時刻・エージェント・レベル・承認方法・結果・外部送信の有無）／ 夜間サイクルの結果 ／ L3 承認キュー。

### D6. Traceability
時系列一覧。route / purpose / destination で絞り込み。**本文は無く digest と bytes のみ**。
`third-party` 行を強調。

---

## E. 描く順序（ツールを問わず）

1. **A1–A3**（済: Figma `FJQojT0lzJovdk9ElAs5vc`）
2. A4 / A5 → B
3. **C1–C4**（新規の中で最も判断が要る。ここを先に見たい）
4. D2 → D1 → D4 → D5 → D6 → D3

> C と D2 が「このプロダクトが記録ツールではない」ことを一番よく示す2枚。
> レビューを1枚に絞るならこの2枚。
