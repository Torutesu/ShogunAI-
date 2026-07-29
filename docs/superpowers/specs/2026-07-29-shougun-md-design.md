# Shougun.md — ユーザー個別最適化 設計書

- Issue: [#41](https://github.com/Torutesu/ShogunAI-/issues/41) — Shougun.md 的なユーザーの MD ファイルで制御する。個別最適化。
- 日付: 2026-07-29
- ステータス: 設計確定（実装は①コアのみ着手、②は設計のみ）
- Base ブランチ: `main`

---

## 1. 目的とスコープ

ユーザーごとの 1 枚の Markdown ファイル（`Shougun.md`）を宣言的な設定として持ち、ShogunAI の挙動を個別最適化する。加えて、日々の会話・行動ログから「魅力（Charm）候補」を発見し、週次でユーザーに提案する。

本設計は 2 つのサブシステムを含む：

- **① `Shougun.md` パース → Context 注入基盤**（本 Issue で実装）
- **② 無意識インプットからの Charm 発見・週次提案**（本設計で仕様確定、実装は後続 Issue）

### Non-Goal（Issue 準拠）

- 全設定 UI の廃止（スコープ外）。
- すべての設定を Markdown だけで完結させること。
- エンドユーザーによる高度なスクリプティング提供。
- モデル選択・インフラ等システムレベル設定の対象化。
- Charm の自動推定を無断で `Shougun.md` に書き込むこと（提案＋ユーザー承認が前提）。

### 確定した設計判断（ブレスト結果）

1. **Context 注入は system prompt 注入のみ**。Fusion の relevance 重み付けや Workflow の trigger→steps ルーティングは v1 では行わない（後続検討）。
2. **パースはハイブリッド・fail-soft**。既知見出しを型付き構造体へ、未知見出しは保持のみ、欠損は許容、セクション単位でエラー隔離。
3. **パース結果は DB に持たず daemon の RAM に常駐**。ファイルが単一の source of truth。Charm 候補（②）のみ永続化が必要。
4. **`# Charm` の書き戻しは採用系操作のみ**。AI は無断で書き換えない。

---

## 2. アーキテクチャ全体像

```
~/Shougun.md  ──(notify/FSEvents)──▶  daemon watcher
   │                                      │ 再パース (fail-soft, debounce)
   │                                      ▼
   │                            ShougunConfig (RwLock<Arc<..>>, RAM常駐)
   │                                      │ + ParseReport(成否/エラー行)
   │                                      ├─▶ bus event: UserConfigUpdated
   │                                      ▼
   │                        render_directives(&config) -> String
   │                                      ▼
   └── 設定UI / CLI で編集 ──▶  全"ユーザー向け生成"の system prompt 先頭へ注入
                                (chat / draft / morning brief / intro生成)
```

**単一 source of truth はファイル。** パース結果は DB に持たず、変更のたび RAM 上で再パースする（設定は再計算が安く、常にファイルが裏にある → マイグレーション不要）。CLAUDE.md の「データの重心は Rust コア」「Hot=RAM」方針に合致。

### 統合点（コード調査で確定）

| 領域 | 既存の入口 | 変更/追加 |
|---|---|---|
| Context 組み立て | `crates/shogun-fusion/src/assemble.rs::assemble()` / `crates/shogun-core/src/daemon.rs::assemble_context()` | v1 では **触らない**（注入は prompt 側で行う） |
| system prompt 生成 | `crates/shogun-core/src/inline.rs::compose_inline` ほかユーザー向け生成呼び出し | `render_directives()` の出力を先頭へ注入 |
| 設定保存 | `apps/desktop/src-tauri/src/inline_source.rs`（`llm.json` パターン） | 本機能はファイルが SoT。Tauri command でパス/状態を返す |
| ファイル監視 | 既存はポーリングのみ（`notify` 未導入） | `notify` crate を追加し daemon に watcher を新設 |
| 設定 UI | `apps/desktop/src/App.tsx::Settings`（`<section className="set">` の並び） | `PersonalizationSection` を追加 |
| Markdown パース | 未導入 | `pulldown-cmark` を追加 |

> 実装時の注意: ユーザー向け生成の呼び出し site が複数（chat / draft / Morning Brief / intro）存在しうる。注入は必ず単一の `render_directives()` を経由させ、各 site はそれを先頭連結するだけにする（DRY・対称性の担保）。実装計画で全 site を洗い出すこと。

---

## 3. ①-1 パーサ / データモデル（実装対象）

- 配置: 新モジュール `crates/shogun-core/src/user_config/`（DB 非依存の純粋関数群）。
- 依存追加: `pulldown-cmark`（Markdown）。YAML frontmatter は**使わない**（Issue 仕様は見出しベース）。

### データモデル

```
ShougunConfig {
  profile:    Profile { role, industry, tools[], topics[] }
  style:      Style   { tone, length, format_hints[] }
  principles: Vec<String>
  do_not:     Vec<String>
  workflows:  Vec<Workflow { name, trigger, steps[] }>   // v1はテキスト化のみ（routingしない）
  charm:      Charm   { core_strengths[], persona_for_others[],
                        preferred_intro_contexts[], ng_charm_patterns[] }
  unknown_sections: Vec<RawSection>                        // 未知見出しは保持のみ
}

ParseReport { ok: bool, section_errors: Vec<SectionError { section, line, message }> }
```

### パース方針（ハイブリッド・fail-soft）

1. 見出し（`# Profile` / `# Style` / `# Principles` / `# DoNot` / `# Workflows` / `# Charm`）でセクション分割。
2. 既知見出しは対応する型へパース。サブキー（`Role:` `Tone:` `CoreStrengths:` など）と箇条書きを解釈。
3. **セクション単位でエラーを隔離**: 1 セクションが壊れても他は生かす。`# Charm` が不正なら Charm 機能のみ無効化しコアは継続。
4. 欠損セクションは空で許容。未知見出しは `unknown_sections` に保持（破棄しない）。
5. `ParseReport` にセクション名・行番号・メッセージを蓄積し、UI/CLI へ返す。

### テスト

- 正常系: Issue の設定例をそのままパースし全フィールドが埋まる。
- fail-soft: `# Charm` だけ壊れたケースで、Charm が空・他セクションは正常・`section_errors` に Charm のみ。
- 欠損・未知見出し・空ファイルの各ケース。

---

## 4. ①-2 ファイル監視 / サンプル生成 / RAM 常駐（実装対象）

- 依存追加: `notify` crate（macOS は FSEvents）。ポーリングより低 CPU で SLO（アイドル 5%）に有利。
- 監視対象: デフォルト `~/Shougun.md`（ホーム直下）。
- フロー: 変更検知 → debounce（〜500ms）→ 再パース → `RwLock<Arc<ShougunConfig>>` を更新 → bus に `UserConfigUpdated` を emit。
- 起動時:
  - ファイル無し → **サンプル `Shougun.md`（`# Charm` 雛形＋各項目の一言コメント込み）を自動生成**し、パスを UI で案内。
  - ファイル有り → 即パースして適用。
- サンプルテンプレートには「生のパスワードや API キーは書かない」旨の注意書きを含める（Issue のセキュリティ要件）。

---

## 5. ①-3 Context 注入（実装対象）

- **単一の注入関数** `render_directives(&ShougunConfig) -> String` が「User Directives」ブロックを生成。
  - 含めるもの: Style（tone/length/format）、Principles、DoNot、Charm、Workflows（テキストヒントとして）。
  - Profile は必要に応じて短く前置き。
- ユーザー向け生成呼び出し（chat / draft・reply / Morning Brief / 自己紹介・他者紹介文生成）の **system prompt 先頭**に差し込む。
- **背景処理（indexing / Dream 分類）には注入しない**（構造抽出でありペルソナ不要。キー分離＝不変条件5とも整合）。
- **confidence gate（`shogun-fusion::confidence`）は一切触らない**。ユーザー指示はランキングではなく宣言的前置きなので、低 confidence 状態が事実として混入するリスクを増やさない。
- Charm が無効（パース失敗）時は Charm ブロックを省くフェイルセーフ。

---

## 6. ①-4 設定 UI / Tauri commands（実装対象）

`App.tsx` の `Settings` に `PersonalizationSection`（`<section className="set">`）を追加：

- ファイルパス表示 ／ **Open in Editor**（Finder/エディタで開く）／ **Regenerate Sample**
- ステータス: 最終更新日時＋直近パース結果（Success / Error＋エラー行）。
- 初回起動 or メジャーアップデート時: 「1 枚の Markdown であなた専用の ShogunAI を育てましょう」紹介モーダル。`# Charm` にも触れ「強み・魅力も一緒に言語化できます」と案内。

Tauri commands（`lib.rs` に登録）:

- `get_user_config_status() -> { path, last_updated, parse: ParseReport }`
- `open_shougun_md()` — エディタ/Finder で開く
- `regenerate_shougun_md()` — サンプル再生成

UI 文言は英語（v1）、i18n-ready にコードから分離。ブランドルール準拠（競合名・技術名を出さない、絵文字は ⚔ のみ）。

---

## 7. ①-5 CLI / MCP 対称性（実装対象・不変条件6）

- `shogun config path | show | validate`（`crates/shogun-cli`）。
  - `path`: 解決されたファイルパス。
  - `show`: 現在の `ShougunConfig`（パース結果）を表示。
  - `validate`: パースを走らせ `ParseReport` を返す（CI/エディタ連携用）。
- Memory API（MCP/REST）経由の read を提供。ファイル文化ユーザー向けという Issue 動機に直結。
- 人間 UI と AI API を対称に保つ（新機能は UI と API 両方から呼べる）。

---

## 8. ② Charm 発見・週次提案（設計のみ・実装は後続 Issue）

### 8.1 フロー

```
event_log / chat turns (すべてローカル既存キャプチャ)
        │  週1回、Dream Cycle バッチに相乗り (Select KK Batch key)
        ▼
 CharmSignalExtractor ── シグナル抽出 ──▶ 候補生成 (provenance + confidence)
        ▼
 charm_candidates テーブル (shogun-memory, 新規・要永続化)
        │  status: pending / adopted / edited / discarded
        ▼
 週次提案UI (banner/modal, before/after 差分 = Charm セクションのみ)
        │  「採用 / 編集して採用 / 破棄」
        ▼  採用系のみ
 Shougun.md の #Charm を外科的に追記 ──▶ watcher が再パース
```

### 8.2 シグナル抽出（Issue の 3 種）

1. 繰り返し依頼されるタスク／思考パターン（同種 intent の頻度）。
2. 他者から褒められた文脈（発話・メモ中の「褒められた/評価された」等マーカー周辺）。
3. 意思決定で一貫している優先順位・観点（decision 系イベントの軸の一貫性）。

各候補は `evidence_refs`（根拠 event_id）＋ `confidence` を必ず持つ。低 confidence 候補は提案に出さない（provenance＋confidence の不変条件）。

### 8.3 実行基盤（Dream Cycle 相乗り）

- Charm 候補生成は背景バッチ推論 → **Select KK Batch キー**が正（Dream Cycle / Morning Brief と同じ。不変条件5）。BYOK は使わない。
- 既存 Dream Cycle スケジューラ・バッチ基盤を再利用。新規スケジューラ不要。
- 生データはデバイス外に出さず処理チャンクのみ（不変条件2・3）。送信箇所にトレーサビリティログ。

### 8.4 永続化（①と異なり DB が必要）

- 新テーブル `charm_candidates(id, text, section_key, evidence_json, confidence, status, created_at, decided_at)`（additive migration、ロールバック手順添付）。
- 候補は 1 週間かけて蓄積し、採用/破棄の判断履歴も残す → 再提案抑制・承認率 KPI 計測に利用。

### 8.5 提案 UI / 書き戻し

- インラインバナー or モーダルで **before/after 差分**（変更は `# Charm` のみハイライト、モバイル幅でも読める文量）。
- 3 アクション: **採用 / 編集して採用（差分編集）/ 破棄**。
- **書き戻しは採用系のみ**。`# Charm` セクションを**外科的に追記**（他セクション・コメント・整形を保持）→ watcher が再パースして即反映。無断書き換えはしない。

### 8.6 Morning Brief 連携（Charm の主用途）

- Morning Brief 生成時に `Charm.CoreStrengths` × 当日カレンダー/タスクで「今日の“魅力の使いどころ”」を 1〜2 行生成。
- 日中の曖昧質問（「今日の武器なに？」）にも `Charm` ＋当日文脈で短く返す。
- これは①の注入基盤（§5）が入れば、Charm を system prompt に載せるだけで大部分が実現できる。

---

## 9. 権限・セキュリティ

- `Shougun.md` はローカル保持、クラウド送信しない（同期は別 Issue）。
- サンプルに「生のパスワード/API キーを書かない」注意書き。
- AI による `Shougun.md` 自動更新は**ユーザーが明示承認した差分のみ**。
- テレメトリ・ログにファイル内容やキャプチャ内容を含めない。

---

## 10. 実装順（本 Issue で①のみ実装）

| 範囲 | 内容 | 本 Issue |
|---|---|---|
| ①-1 | パーサ＋データモデル（fail-soft、単体テスト） | ✅ 実装 |
| ①-2 | `notify` 監視＋サンプル生成＋RAM 常駐＋bus | ✅ 実装 |
| ①-3 | `render_directives` 注入（単一点） | ✅ 実装 |
| ①-4 | 設定 UI＋Tauri commands | ✅ 実装 |
| ①-5 | CLI/MCP 対称 (`shogun config …`) | ✅ 実装 |
| ② | Charm 抽出・週次提案・書き戻し・Morning Brief 連携 | 📐 設計のみ（後続 Issue） |

---

## 11. 成果指標（Issue 準拠）

- `Shougun.md` 有効化ユーザーの 1 週間後継続利用率 +20% 以上。
- 「期待通りの答え」割合 +30pt 以上。
- 「カスタマイズ方法」問い合わせ −30%。
- サンプルテンプレート GitHub リポジトリの Star/Fork。
- （②）Morning Brief 有効ユーザーで「強み・魅力が言語化されている」と感じる割合 70% 以上。
- （②）週次「魅力アップデート提案」承認率 30〜50%。
