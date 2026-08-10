# Skills — 概念・データモデル・UI 設計（Issue #57）

**対象**: ShogunAI (SHOGUN) macOS アプリ本体 / v1
**前提文書**: `CLAUDE.md`（絶対不変条件）、`docs/requirements-v1.0.md`（§6.5 Fusion / §6.6 エージェント / §6.11 Memory API / §6.13 オンボーディング / §6.15 設定）、`docs/meeting-notes-ui-design.md`、`docs/push-to-talk-voice-design.md`
**状態**: 設計提案（未着手）。§13 の未決事項はオーナー判断待ち。

---

## 0. 結論サマリ

この設計は Issue #57 の要求を 3 点で**再定義**した上で開発要件に落とす。

1. **Skill は「新しい実行基盤」ではなく、既に存在する能力に付ける“ユーザーに見える名前”である。** v1 の Skill は Rust の宣言的レジストリ（静的マニフェスト表）であり、プリセットエージェント（FR-AG-10..16）、Morning Brief、会議ノート、Visual recall、Push-to-Talk、メモリ検索を 1 つの語彙に束ねる。新しいプロセス・新しいサンドボックス・新しいプラグイン読み込みは作らない。
2. **Skill は権限の単位ではない。** 権限は今までどおり operation（`OpKind`）に付く。Skill を ON にしても L1/L2/L3 の判定、Composio の 3 開示同意、TCC 権限、プラン境界は**一切変わらない**。「Skill を ON にしたら送れるようになる」という経路を作らないことが、不変条件 4（L1 に外部送信を含めない）と 6（UI と API の対称）を守るための必須条件である。
3. **カタログは「探して有効化させる場所」ではなく「理解して止める場所」である。** SHOGUN の製品命題は「ボタンを押して仕事が終わる」であり、能力の発見をユーザーの仕事にしない。したがって Skill 一覧の既定ソートは人気順ではなく **あなたの実使用順**、既定状態は**主要 Skill が最初から ON**、一覧の主機能は **OFF にする・何が要るか知る**である。

この 3 点により、Issue が挙げたゴール「新しい Skill をメタデータ定義だけで UI に載せられる」「Skill 単位で計測できる」は達成しつつ、マーケットプレイス的な設計（人気順・ストア・サードパーティ配布）は v1 から外す。

---

## 1. Issue の前提と製品命題の衝突点

Issue #57 は良い問題設定だが、そのまま実装すると SHOGUN の設計原則と 4 箇所で衝突する。衝突点と本設計の解を先に置く。

| # | Issue の記述 | 衝突する原則 | 本設計の解 |
|---|---|---|---|
| C1 | 「Skill 一覧からブラウズし、有効化・設定して利用する」 | 「状態の推定と実行」— 何を押すかは Context Fusion が決める。ユーザーに能力探索を課すと、押すべきボタンを自分で組み立てる作業が戻る | カタログは**理解と制御**の面。価値の初回到達はカタログ経由にせず、Fusion が出したアクションに Skill 名を帰属表示し、そこから詳細へ入れる（**逆流導線**） |
| C2 | 「MCP 連携を内部的には Skill の一種として扱う（カレンダー読み取り Skill 等）」 | Full UI には既に **Sources ペイン**（接続・同期状態・第三者バッジ）がある。連携を Skill にすると同じ対象に 2 つの ON/OFF が生まれ、どちらが真かユーザーにも実装にも分からなくなる | 連携は Skill に**しない**。Skill は `requires` で Source を参照するだけ。Skill 詳細の「必要な接続」行は Sources ペインの当該行へ**深リンク**する（状態の単一所有） |
| C3 | 「ソート: 人気順」 | ローカルファースト＋分析は機能カウントのみ（PostHog opt-out 単一系統）。クロスユーザー人気度はサーバ集計を要し、v1 の計測方針では作れない | v1 は**ローカル使用順**（`skill_runs` 集計）＋「要設定を上に」。人気順はサーバ集計の可否が決まるまで作らない（§13-Q6） |
| C4 | 「Skill ごとに ON/OFF トグル」＋「必要な権限を接続できる」 | 会議ノートは既定 OFF、ON を選んだ時**だけ**マイク TCC を要求する（FR-OB-06）。Visual recall も同様。トグルが権限要求を発火する設計にすると、この原則が Skill 全体に穴を開ける | **トグル ON は権限要求を一切発火しない。** 権限・接続の要求は詳細画面の専用ボタン（Connect / Enable capture）からのみ。ON かつ未接続の Skill は `NeedsSetup` として「動かない理由」を出す |

加えて Issue の Non-Goal（マーケットプレイス、課金ロジック、全機能の Skill 化）は本設計でも Non-Goal として維持する（§12）。

---

## 2. 不変条件との突き合わせ

| 不変条件 | 本設計での担保 |
|---|---|
| 1. データの重心は Rust コア | マニフェスト・状態解決・一覧 view の組み立てはすべて Rust。webview は `skills_view` の結果を描画するのみ。ON/OFF の可否判定を webview で行わない |
| 2. 画像・音声を保存しない | Skill 層は新しいキャプチャ経路を作らない。`meeting_notes` / `visual_recall` Skill は既存の例外規定（2026-08-02 / 2026-08-05）をそのまま参照し、条件を緩めない |
| 3. 生データをデバイス外に出さない | Skill の実行が外部送信を伴う場合、経路は従来どおり共通 HTTP 出口（FR-TR-03）。`skill_runs.trace_id` で実行履歴とトレーサビリティを 1 対 1 に結ぶ |
| 4. L1 に外部送信を含めない | Skill は `OpKind` を**再定義しない**。level は `OpKind::mandated_level()` から導出（`crates/shogun-agents/src/presets.rs` の既存規則）。マニフェストに level を直接書けない型設計にする（§4.3） |
| 5. キーの分離 | Skill マニフェストは `lane: batch | agent | local` を宣言し、`batch` lane の Skill が Agent 資格情報を要求しないことを型で担保。サブスク委譲を batch lane に流す経路は増やさない |
| 6. 人間 UI と AI API の対称 | `skills.list` / `skills.get` / `skills.set_enabled` / `skills.execute` を Memory API に追加。ただし consent 保持 Skill の API からの有効化は拒否（§7） |
| 7. secrets は Keychain のみ | Skill 設定（`settings_json`）に秘匿値を入れない。型で禁止（`SettingSpec` に secret 種別を作らない）。キーは従来どおり Keychain |

---

## 3. 概念モデル

### 3.1 Skill の定義

> **Skill** = ある目的を完了させるために SHOGUN が持つ能力を、ユーザーが認識・制御できる 1 単位に束ねたもの。1 つ以上の **operation** と、その動作に必要な **requirement**、および宣言的な **settings** からなる。

Skill であるもの / ないものを明確に切る。

| Skill である | Skill ではない |
|---|---|
| プリセットエージェント（Reply Drafter 等） | 連携サービス（Gmail / Google Calendar / Slack …）→ **Source** |
| バックグラウンドで走る価値生成（Morning Brief、Follow-up Sentinel） | 権限（Accessibility / マイク）→ **Permission** |
| 明示的に呼ぶ道具（Search memory、Ask SHOGUN） | 実行レベル（L1/L2/L3）→ **Permission model** |
| 入力モダリティを持つ機能（会議ノート、Push-to-Talk、Visual recall） | プラン（Standard/Pro）→ **Entitlement** |
| | Dream Cycle 個々のジョブ（内部処理。ユーザーが個別に止める対象ではない） |

### 3.2 Skill の種別（kind）

一覧に何を並べ、どう説明するかを決めるのは種別である。

| kind | 意味 | ユーザーから見た起動 | 例 |
|---|---|---|---|
| `assistive` | Fusion が文脈から候補として提示する | Notch に勝手に出る | Reply Drafter, Task Extractor, Calendar Scheduler |
| `ambient` | 条件が揃うと背景で走り、結果だけ現れる | 起動しない（結果が来る） | Morning Brief, Follow-up Sentinel, Meeting Notes, Visual recall |
| `invocable` | ユーザーが明示的に呼ぶ | ホットキー / 音声 / 入力欄 | Search Memory, Ask SHOGUN, Note Capture |

`ambient` は「OFF にできること」が価値であり、`assistive` は「なぜ今これが出たか」が価値であり、`invocable` は「どう呼ぶか」が価値である。詳細画面はこの 3 種で情報の並び順を変える（§6.2）。

### 3.3 状態機械（SkillStatus）

Skill の表示状態は**単一の純関数**で解決する。webview 側で状態を組み立てない。

```
resolve_status(manifest, user_state, entitlements, connections, permissions, consents) -> SkillStatus
```

| Status | 条件 | 一覧での見え方 | トグル |
|---|---|---|---|
| `Active` | ON かつ全 requirement 充足かつ直近 14 日に実行あり | 通常表示・使用回数の薄い表示 | 操作可 |
| `Ready` | ON かつ全 requirement 充足 | 通常表示 | 操作可 |
| `NeedsSetup { missing }` | ON だが requirement 未充足（未接続 / 未同意 / 権限なし / 資格情報なし） | 「Needs …」行＋修正ボタン | 操作可 |
| `Off` | ユーザーが OFF | 減光表示 | 操作可 |
| `Locked { plan }` | 現在のプランに含まれない | 減光＋鍵。CTA は 1 つのみ | 操作不可 |
| `Unavailable { reason }` | 環境要因（macOS バージョン、ノッチ非搭載など。v1 では該当なし想定） | 減光＋理由 | 操作不可 |
| `Core` | OFF にできない基盤 Skill | 通常表示＋"Core" バッジ | 無効化（理由をツールチップ） |

補助フラグ: `beta`, `deprecated`, `rollout`（§4.5）。

**規則**: `Locked` の Skill もカタログには出す（何が Pro なのかを隠すと期待値制御ができない）。ただしアップグレード CTA は**一覧に出さず詳細画面に 1 つだけ**。FR-CF-05 が「ロック表示はアクション候補 4 件のうち最大 1 件」と定めた思想を、カタログ面にも適用する — Standard ユーザーの UI を広告面にしない。

### 3.4 v1 カタログ（13 種）

既存 FR にすべて紐付く。新規機能はここで 1 つも増やさない。

| skill_id | 表示名 | kind | 既存 FR / 出典 | 既定 | プラン | requires |
|---|---|---|---|---|---|---|
| `reply_drafter` | Reply Drafter | assistive | FR-AG-10 | ON | 提示=Std / 実行=Pro | agent credential, (送信時) Gmail via Composio + 同意 |
| `meeting_prep` | Meeting Prep | assistive | FR-AG-11 | ON | Std（集約）/ Pro（LLM 整形） | Google Calendar |
| `task_extractor` | Task Extractor | assistive | FR-AG-12 | ON | Pro | agent credential |
| `follow_up_sentinel` | Follow-up Sentinel | ambient | FR-AG-13 | ON | Std（提示）/ Pro（ドラフト） | — |
| `calendar_scheduler` | Calendar Scheduler | assistive | FR-AG-14 | **OFF** | Pro | Google Calendar, agent credential |
| `issue_triage` | Issue Triage | assistive | FR-AG-15 | **OFF** | Pro | GitHub or Linear, agent credential |
| `note_capture` | Note Capture | invocable | FR-AG-16 | ON | Std（下書き）/ Pro（Notion 書き込み） | Notion（書き込み時） |
| `morning_brief` | Morning Brief | ambient | §6.8 FR-MB | ON | Std | — |
| `meeting_notes` | Meeting Notes | ambient | §6.16 FR-MT-01 | **OFF** | All | マイク TCC ＋ ASR 開示同意 |
| `visual_recall` | Visual Recall | ambient | CLAUDE.md 2026-08-02 例外 / #106 | **OFF** | Std | 画面録画権限 |
| `voice_ask` | Push-to-Talk | invocable | Issue #44 / `docs/push-to-talk-voice-design.md` | ON | Std（文字起こし）/ Pro（生成） | マイク TCC |
| `memory_search` | Search Memory | invocable | FR-MEM-20 | **Core** | All | — |
| `ask_shogun` | Ask SHOGUN | invocable | FR-AG-17 | ON | Pro | agent credential |

**既定 OFF の根拠**: `calendar_scheduler` / `issue_triage` は不可逆な外部書き込み（L3）を主用途とし、既定で候補に出すと L3 確認ダイアログの出現頻度が上がる。`meeting_notes` / `visual_recall` は既存規定どおり既定 OFF（FR-MT-01 / #106）。

### 3.5 Skill マニフェスト

Issue のゴール「メタデータ＋設定＋アイコンを定義すれば UI に自然に載る」を満たす宣言。Rust の静的テーブルとして `crates/shogun-agents/src/skills/manifest.rs` に置く。

```rust
pub struct SkillManifest {
    pub id: SkillId,                       // 安定識別子。分析・API・DB の join key
    pub name: &'static str,                // 英語（v1）。i18n は Rust 側カタログで将来対応
    pub summary: &'static str,             // 1 行。一覧に出す
    pub description: &'static str,         // 2〜4 行。詳細に出す
    pub kind: SkillKind,                   // assistive | ambient | invocable
    pub category: Category,                // Communication | Meetings | Tasks | Memory | Developer
    pub lane: Lane,                        // batch | agent | local（不変条件 5 の型境界）
    pub triggers: &'static [Trigger],      // Contextual | Hotkey | Voice | Schedule | Api
    pub inputs: &'static [Channel],        // Screen | Mic | Calendar | Mail | Chat | File
    pub outputs: &'static [Output],        // Draft | StateWrite | Summary | ExternalWrite | LocalNote
    pub operations: &'static [Operation],  // 既存 presets::Operation をそのまま再利用（level は導出）
    pub requires: &'static [Requirement],  // Source | Permission | Consent | AgentCredential | Plan
    pub settings: &'static [SettingSpec],  // 宣言から UI を自動生成
    pub invocation_phrases: &'static [&'static str], // 音声・コマンドパレットの別名
    pub default_enabled: bool,
    pub core: bool,                        // true なら OFF 不可
    pub maturity: Maturity,                // Stable | Beta | Deprecated
    pub manifest_version: u32,             // 設定の互換判断に使う
    pub rollout: Rollout,                  // All | Percentage(u8) | Internal
}
```

`SettingSpec` は v1 で 3 種のみ。ここを絞ることが「新しい Skill をコードほぼ無しで載せる」ための条件である。

```rust
pub enum SettingSpec {
    Toggle { key, label, help, default: bool },
    Select { key, label, help, options: &'static [(&'static str, &'static str)], default: &'static str },
    ShortText { key, label, help, max_len: usize, default: &'static str },  // ≤120 文字
}
```

**禁止**: 自由記述のプロンプト欄（Issue Non-Goal「プロンプト最適化」に該当）、秘匿値の入力欄（不変条件 7）、任意 URL 入力（外部送信先の追加は共通出口の許可リストを崩す）。

---

## 4. アーキテクチャ

### 4.1 配置

| 置き場所 | 内容 | 理由 |
|---|---|---|
| `crates/shogun-agents/src/skills/manifest.rs` | 静的マニフェスト表 | operation / level と同じクレートに置き、両者の乖離をコンパイル時に潰す |
| `crates/shogun-agents/src/skills/status.rs` | `resolve_status` 純関数 | 時計・IO を持たない。既存 `entitlement.rs` と同じ「pure + 引数で now_ms」規約に従う |
| `crates/shogun-memory/src/skills.rs` | `skill_state` / `skill_runs` の読み書き | DB 層はメモリクレート所有 |
| `crates/shogun-core/` | view 組み立て・イベント発火・分析 | webview に渡す payload の単一組立点 |
| `apps/desktop/src/fullui/` | Skills ペイン描画 | 描画のみ。判定を持たない |

新規クレートは作らない。

### 4.2 Fusion との結線

`ActionCandidate`（`crates/shogun-fusion/src/assemble.rs`）に `skill_id` を追加する。

```rust
pub struct ActionCandidate {
    pub action: Action,
    pub level: Level,
    pub rationale: String,
    pub skill_id: SkillId,   // 追加。すべての候補は必ずどれかの Skill に帰属する
}
```

- **すべての候補が既知の Skill に帰属する**ことをテストで網羅する（帰属不能な候補を作れない）。FR-CF-04 の汎用アクション（Save note / Search memory / Extract tasks）は `note_capture` / `memory_search` / `task_extractor` に帰属する。
- **OFF の Skill の候補は Fusion のランキングに入る前に落とす。** 4 件枠（`MAX_ACTIONS`）は残った候補で埋める。OFF によって枠が空くのではなく、次点が繰り上がる。
- 候補の抑止判定は**候補生成の後・スコアリングの前**の 1 箇所（`filter_by_skill_state`）に置く。cache 更新経路（300ms、SLO-05）の中で完結する純関数であること — 押してから問い合わせない。

### 4.3 Gating の合成（最重要）

Skill の ON/OFF が権限判定に**触れない**ことを、型と関数分離で担保する。

```
表示・提示するか        = skill_enabled(skill) ∧ entitled(plan, skill) ∧ requirements_met(skill)
実行してよいか（level） = OpKind::mandated_level(op)        ← skill_state を引数に取らない
実行できるか            = level 承認済 ∧ entitled(plan, op) ∧ consent(route) ∧ credential(lane)
```

- `mandated_level()` のシグネチャに `SkillState` を渡せないことをアーキテクチャテストで固定する（`skills` モジュールが `permission::Level` を**構築**する経路を持たない）。
- Skill を ON にしても Composio の 3 開示同意、TCC、OAuth、プランのいずれも自動的に満たされない。`NeedsSetup { missing }` として**動かない理由を並べる**のが Skill 層の責務であり、満たすのは各機能の既存フローである。

### 4.4 実行経路と帰属

| サーフェス | Skill の関与 |
|---|---|
| Notch Expanded | アクションボタンの補助行に Skill 名を薄く表示（`Reply Drafter` 等）。ボタン自体の情報量は増やさない。⌥ ホバーで詳細への導線 |
| Notch Hover | Skill 名を出さない（1 行プレビューの情報量を守る） |
| Full UI Today | 提案アクションに Skill 名を帰属表示。ここから詳細パネルへ |
| Voice (PTT) | `invocation_phrases` でマッチ。OFF の Skill にマッチした場合は**無言で失敗せず**「This skill is off — turn it on?」を返す（Issue UX フロー 5 の要求） |
| Memory API | `skills.execute`（§7） |

### 4.5 ロールアウトと Skill バージョン

- `rollout: Percentage(n)` は `analytics.json` の `distinct_id` を安定ハッシュしてローカル判定する。サーバ配信の feature flag は v1 で作らない（オフライン動作を壊さないため）。
- `manifest_version` を上げたときは `skill_state.manifest_version` と比較し、`settings_json` の未知キーは破棄・欠損キーは既定で補う（前方後方互換。`skill_state` 行自体は消さない）。
- `Maturity::Deprecated` の Skill は一覧の末尾に「Retiring」節を作って表示し、実行は継続可能。次のメジャーで削除。

---

## 5. データモデル（マイグレーション V15）

`V15__skills.sql`（additive）。`skill_runs` は FR-AG-18 が要求する**実行履歴そのもの**を兼ねる（別テーブルを二重に作らない）。

```sql
-- Skill layer (Issue #57). Additive.
--
-- `skill_state` はユーザーの選択だけを持つ。マニフェスト（名前・必要権限・operation）はコード側の
-- 静的表が唯一の真実であり、DB に複製しない — 複製すると更新時に両者が乖離する。
-- `skill_runs` は FR-AG-18 の実行履歴を兼ねる。**本文・宛先・対象名の列を持たない**（テレメトリ規約と
-- 同じ理由: 実行履歴に生テキストを溜めない）。送信があった行は trace_id で traceability_log を指す。

CREATE TABLE skill_state (
    skill_id         TEXT    PRIMARY KEY,
    enabled          INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    enabled_source   TEXT    NOT NULL CHECK (enabled_source IN ('default', 'user', 'onboarding')),
    settings_json    TEXT    NOT NULL DEFAULT '{}',
    manifest_version INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
) STRICT;

CREATE TABLE skill_runs (
    id           INTEGER PRIMARY KEY,
    skill_id     TEXT    NOT NULL,
    op           TEXT    NOT NULL,
    level        TEXT    NOT NULL CHECK (level IN ('l1', 'l2', 'l3')),
    surface      TEXT    NOT NULL CHECK (surface IN ('notch', 'full_ui', 'voice', 'api', 'background')),
    approval     TEXT             CHECK (approval IN ('auto', 'one_tap', 'explicit', 'rejected', 'timeout')),
    outcome      TEXT    NOT NULL CHECK (outcome IN ('ok', 'error', 'cancelled')),
    error_kind   TEXT             CHECK (error_kind IN ('permission', 'connection', 'credential',
                                                        'rate_limit', 'plan', 'timeout', 'internal')),
    duration_ms  INTEGER,
    trace_id     INTEGER REFERENCES traceability_log (id),
    undone_at    INTEGER,                       -- FR-AG-05 の undo（L1、7 日）
    started_at   INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_skill_runs_skill_time ON skill_runs (skill_id, started_at);
CREATE INDEX idx_skill_runs_time       ON skill_runs (started_at);
```

**ロールバック手順**: `DROP TABLE skill_runs; DROP TABLE skill_state;`（両テーブルとも他テーブルから参照されない。`trace_id` は片方向参照）。ユーザー設定は既定に戻る＝ Skill は既定 ON/OFF に復帰し、機能欠損は起きない。

**データ削除（FR-SET-07）との整合**: 期間指定削除は `skill_runs` を対象に含める（`started_at` 範囲）。`skill_state` はユーザー設定であり削除対象外。全削除では両方を消す。

**エクスポート（FR-SET-08）**: `skill_state` と `skill_runs` を JSON Lines に含める。

---

## 6. UI 設計

### 6.1 Full UI「Skills」ペイン（一覧）

`PaneId` に `"skills"` を追加（`apps/desktop/src/fullui/types.ts`）。ナビ上の位置は Today / **Skills** / Sources / Memory / Activity / Health / Trace（Today の直後 — 「何が起きるか」の次に「何ができるか」）。

```
┌────────────────────────────────────────────────────────────────────┐
│  Skills                                          [ search…      ]  │
│  何ができて、何が要るか。使わないものはここで止められる。            │
├──────────────┬─────────────────────────────────────────────────────┤
│ All       13 │  ● Reply Drafter                    Communication  ⏻ │
│ Communication│    Drafts a reply in your voice, from what you       │
│ Meetings     │    already agreed. Used 12× this week               │
│ Tasks        │ ─────────────────────────────────────────────────── │
│ Memory       │  ● Morning Brief                          Memory  ⏻ │
│ Developer    │    Your day, assembled overnight. Ready             │
│              │ ─────────────────────────────────────────────────── │
│ ── Status ── │  ▲ Calendar Scheduler        Meetings  · Needs setup │
│ Needs setup 2│    Google Calendar isn't connected   [ Connect → ]   │
│ Off         3│ ─────────────────────────────────────────────────── │
│ Pro only    1│  ○ Meeting Notes                       Meetings  ⏻ │
│              │    Off. Turn on to have meetings written up          │
│              │ ─────────────────────────────────────────────────── │
│              │  🔒 Issue Triage                      Developer      │
│              │    Included in Pro                                   │
└──────────────┴─────────────────────────────────────────────────────┘
```

**行に出す情報（これ以上増やさない）**: 状態ドット、名前、1 行要約、カテゴリ、状態ラベル（`Used N× this week` / `Ready` / `Needs setup` / `Off` / `Included in Pro`）、トグル。

**既定ソート**（決定事項。人気順は作らない）:
1. `NeedsSetup`（ON にしたのに動いていない = ユーザーが今すぐ直せる唯一の状態）
2. `Active`（直近 14 日の実行回数 降順）
3. `Ready`
4. `Off`
5. `Locked`
6. `Deprecated`（"Retiring" 見出しの下）

**フィルタ**: カテゴリ（5 種）＋ 状態（Needs setup / Off / Pro only）。**検索**は名前・要約・`invocation_phrases` に対する前方一致＋部分一致（ローカル）。ページングは作らない（v1 は 13 件、上限 30 件までは全件描画で十分）。

### 6.2 詳細パネル

一覧の右からスライドインするサイドパネル（別画面遷移にしない — 比較しながら判断する面のため）。

情報の並びは kind で変える。

```
┌──────────────────────────────────────────────┐
│ ⚔ Reply Drafter                    [Beta] ⏻ │
│ Communication · Suggested by context         │
├──────────────────────────────────────────────┤
│ What it does                                 │
│   Drafts a reply to the thread you're        │
│   looking at, using what you already agreed  │
│   with that person.                          │
│                                              │
│ In → Out                                     │
│   Mail / chat thread on screen  →  a draft   │
│   Nothing is sent without your confirmation. │
│                                              │
│ What it needs                                │
│   ✓ AI credential      Claude (subscription) │
│   ⚠ Gmail              Not connected  →      │
│      Only needed to send. Drafting works now.│
│                                              │
│ How it asks                                  │
│   Draft            One-tap approval    (L2)  │
│   Send             Full preview + confirm (L3)│
│                                              │
│ Settings                                     │
│   Tone            [ Match my past replies ▾ ]│
│   Draft length    [ Medium ▾ ]               │
│   Stop at draft   [✓]  (never sends)         │
│                                              │
│ If you turn this off                         │
│   Reply suggestions stop appearing in the    │
│   notch. Nothing already drafted is deleted. │
│                                              │
│ Recent                     12 runs · 1 error │
│   Today 14:20   Draft    ok                  │
│   Today 09:05   Send     needed confirmation │
│                                 [ See all → ]│
├──────────────────────────────────────────────┤
│                        [ Something's off? ↗ ]│
└──────────────────────────────────────────────┘
```

**必須セクション**（マニフェストから自動生成）:

| セクション | 生成元 | 備考 |
|---|---|---|
| What it does | `description` | — |
| In → Out | `inputs` / `outputs` | Issue の「入出力イメージ」。図ではなく 1 行で |
| What it needs | `requires` × 実状態 | 未充足行に修正導線。**押した時だけ**権限要求 |
| How it asks | `operations` の level | L1/L2/L3 を平易語で。「勝手にやる / 一度押す / 全文を見て確認」 |
| Settings | `settings` | 宣言から自動生成。無い Skill では節ごと省略 |
| If you turn this off | `off_effect`（マニフェスト文字列） | **必須**。何が止まるか言えない Skill は載せない |
| Recent | `skill_runs` | 直近 5 件＋ Activity ペインへ |
| Feedback | 固定 | Issue のリリース方針（フィードバック導線） |

`ambient` Skill では「If you turn this off」を上位に、`invocable` では「How to call it」（`invocation_phrases` とホットキー）を「In → Out」の直後に置く。

**Locked（プラン外）の詳細**: 上記のうち What it does / In → Out / How it asks のみを表示し、末尾に **1 つだけ** `Included in Pro — see plans` を置く。設定・Recent・トグルは出さない。

### 6.3 Notch 側

- Expanded のアクションボタンに Skill 名を**補助行として薄く**表示。ボタン数・レイアウト・SLO（100ms / 150ms）は変えない。
- Skill 一覧を Notch に置かない（決定）。ノッチは「今押すボタン」の面であり、能力カタログは 100ms の展開予算にも「押して終わる」体験にも合わない。
- Skill 由来のエラー（`NeedsSetup` で実行不能）はモーダルを出さず、既存のインジケータ色規則に従う（アンバー＝劣化）。Hover の 1 行に「Calendar Scheduler needs Google Calendar」を出し、クリックで詳細パネルへ。

### 6.4 オンボーディングとの接点

Issue は「オンボーディング中の紹介から一覧へ到達」を求めるが、**オンボーディングにステップを追加しない**（FR-OB-01 の 7 ステップは既に長い）。代わりに:

- ステップ 7（完了）の文面に 1 行加える：「What SHOGUN can do — and what it needs — lives in **Skills**.」＋ Skills ペインを開くリンク。
- 初回価値（FR-OB-05）到達時、Notch に出た最初のアクションに Skill 名が帰属表示されているため、そこから逆流できる（C1 の解）。

### 6.5 空・エラー状態

| 状態 | 表示 |
|---|---|
| 検索 0 件 | 「Nothing matches "xyz".」＋フィルタ解除リンク。空カード群を出さない |
| すべて Needs setup（未接続の新規ユーザー） | 一覧の先頭に 1 枚だけ「Connect Gmail and Google Calendar to light most of these up →」 |
| `skills_view` の取得失敗 | Full UI 既存規約に従い、フィクスチャに落ちず「Couldn't read your skills — {reason}」を出す |

### 6.6 SLO

| 項目 | 上限 | 計測 |
|---|---|---|
| Skills ペイン初回描画（invoke → 描画完了） | **150ms** (p95) | 既存 SLO ヒストグラムに `skills_view` を追加 |
| 詳細パネル展開 | **100ms** (p95) | 一覧取得時に詳細の材料も同梱（追加 invoke を発生させない） |
| トグル操作 → 反映（Fusion 候補への波及含む） | **200ms** (p95) | 書き込み後に cache の候補フィルタを再適用 |
| Fusion 候補の Skill フィルタ | cache 更新 300ms 予算の**内側**（追加予算を取らない） | 既存 SLO-05 計測に含める |

Skill 層はレイテンシに影響するため、S4 完了時に p50/p95 を計測して PR 本文に貼る（CLAUDE.md）。

---

## 7. API 対称性（不変条件 6）

Memory API（MCP / CLI / REST）に以下を追加。3 面で機能差を作らない。

| ツール | 内容 | レベル |
|---|---|---|
| `skills.list` | 全 Skill と `SkillStatus`（missing requirement 含む） | 読み取り |
| `skills.get` | 1 件の詳細（settings 現在値含む。秘匿値は存在しない） | 読み取り |
| `skills.set_enabled` | ON/OFF | **L2**（Notch ワンタップ承認）。ただし下記の拒否規則あり |
| `skills.update_settings` | 宣言された設定キーのみ更新 | **L2** |
| `skills.execute` | Skill の operation を起動 | **対象 operation の level に従う**（L3 は FR-AG-04 の承認フロー） |

**拒否規則（必須）**: `requires` に `Consent`（Composio 3 開示 / ASR 開示）または `Permission`（マイク / 画面録画）を含む Skill は、API 経由で**有効化できない**。`rejected: requires_human_consent` を返し、Full UI の該当詳細へのディープリンクを添える。理由 — 同意は人間が読んだことに意味があり、外部 AI のワンタップ承認で代替してはならない。

CLI 例:

```
shogun skills list --status needs-setup
shogun skills show reply_drafter
shogun skills disable calendar_scheduler
shogun skills run task_extractor --from-screen
```

---

## 8. 計測とゴール指標

### 8.1 分析イベント（PostHog / opt-out 単一系統）

既存規約どおり `analytics.json` の `opt_out` のみを見る。**キャプチャ内容・対象名・宛先・設定の自由入力値を絶対に載せない。**

| event | props |
|---|---|
| `skills_pane_opened` | `surface`（`nav` / `notch_link` / `onboarding`） |
| `skill_detail_opened` | `skill_id`, `status` |
| `skill_toggled` | `skill_id`, `enabled`, `surface`, `status_before` |
| `skill_setting_changed` | `skill_id`, `setting_key`（**値は送らない**） |
| `skill_invoked` | `skill_id`, `surface`, `level` |
| `skill_result` | `skill_id`, `outcome`, `error_kind`, `duration_bucket`（`<1s`/`<5s`/`<30s`/`30s+`） |
| `skill_setup_started` | `skill_id`, `requirement_kind` |

`skill_setting_changed` で**値を送らない**のは、`ShortText` 設定に個人情報が入りうるため。キーだけで「どの設定が触られるか」は分かる。

### 8.2 Issue のゴール指標の再定義

Issue の「3 セッション以内に 1 つ以上の Skill を認知・有効化 X%」は、主要 Skill が既定 ON である本設計では測っても意味がない（有効化操作が発生しない）。**認知ではなく到達で測る**形に置き換える。

| # | 指標 | 定義 | 初期目標（要オーナー承認） |
|---|---|---|---|
| N1 | 初回価値到達 | 初回起動から 72h 以内に `skill_result{outcome=ok}` が 1 件以上 | ≥ 60% |
| N2 | 能力の広がり | WAU のうち、週内に**異なる 2 つ以上**の skill_id で成功 | ≥ 40% |
| N3 | カタログの機能 | `skills_pane_opened` したユーザーのうち `skill_detail_opened` に進んだ率 | ≥ 50%（低ければ一覧の情報量不足） |
| N4 | 設定の詰まり | `skill_setup_started` → 24h 以内に同 skill_id で `skill_result{ok}` | ≥ 50%（低ければ接続導線の失敗） |
| G1 | 誤提示ガードレール | `skill_toggled{enabled=false}` 率が高い `assistive` Skill | 月次 5% 超で Fusion 側のスコアを見直す |
| G2 | 失敗ガードレール | `skill_result{outcome=error}` 率 | Skill 単位で 10% 超は要調査 |

G1 が本設計固有の重要指標である — **OFF にされることは失敗ではなく、Fusion の誤提示を検出する最良のシグナル**として扱う。

---

## 9. 開発要件（FR-SK 群）

`docs/requirements-v1.0.md` に **§6.17 Skills** として追加する。プラン列は既存規約（All / Std / Pro）。

### 9.1 概念・レジストリ

**FR-SK-01（MUST, All）**: Skill は Rust の静的マニフェスト表として定義する。DB・設定ファイル・webview にマニフェストを複製しない。v1 で外部からの Skill 追加・読み込み経路を実装しない。

**FR-SK-02（MUST, All）**: 全 Skill は §3.5 の全フィールドを持つ。`off_effect`（OFF にすると何が止まるか）を持たない Skill をカタログに載せない。マニフェストの完全性はコンパイル時（型）＋テストで担保する。

**FR-SK-03（MUST, All）**: Skill の `operations` は `shogun-agents` の既存 `Operation` を再利用し、level は `OpKind::mandated_level()` から導出する。マニフェストに level を直接記述できる型を作らない（不変条件 4）。

**FR-SK-04（MUST, All）**: 連携サービス（第 1 層 / Composio）を Skill として登録しない。Skill は `requires` で Source を参照し、UI は Sources ペインへ深リンクする（状態の単一所有）。

### 9.2 状態と gating

**FR-SK-05（MUST, All）**: `SkillStatus`（§3.3）は純関数 `resolve_status` が単独で決定する。引数は（マニフェスト、ユーザー状態、entitlements、接続状態、権限状態、同意状態、now_ms）。時計・IO を関数内で読まない。

**FR-SK-06（MUST, All）**: Skill の ON/OFF は**提示・起動の可否のみ**を変える。L1/L2/L3 の判定、Composio 同意、TCC 権限、プラン境界のいずれも変更しない。`skills` モジュールから `Level` を構築する経路が存在しないことをアーキテクチャテストで検証する。

**FR-SK-07（MUST, All）**: Skill トグルの ON 操作は、いかなる OS 権限要求・OAuth フロー・同意画面も**発火しない**。権限・接続の要求は詳細画面の専用ボタンからのみ行う（FR-OB-06 の原則を Skill 全体へ一般化）。

**FR-SK-08（MUST, All）**: `core: true` の Skill（v1 では `memory_search`）は OFF にできない。UI はトグルを無効化し、理由を提示する。

**FR-SK-09（MUST, All）**: `default_enabled` はマニフェストの値であり、ユーザーが一度操作した Skill は `enabled_source = 'user'` として以後の既定値変更の影響を受けない。

### 9.3 Fusion・実行との結線

**FR-SK-10（MUST, Std）**: `ActionCandidate` は `skill_id` を必ず持つ。帰属不能な候補を生成できないことをテストで網羅する（FR-CF-04 の汎用アクションを含む）。

**FR-SK-11（MUST, Std）**: OFF の Skill に帰属する候補は、スコアリング前に除外する。除外により空いた枠は次点候補で埋める。除外判定は context cache 更新経路（NFR-SLO-05: 300ms）の内側で完結し、追加のレイテンシ予算を取らない。

**FR-SK-12（MUST, All）**: OFF の Skill は音声・コマンドパレット・API のいずれからも起動できない。ただし音声・パレットで名指しされた場合は無応答にせず、「この Skill は OFF です」と有効化導線を返す。

**FR-SK-13（MUST, All）**: 全 Skill 実行を `skill_runs` に記録する（FR-AG-18 の実行履歴はこのテーブルで実現する）。記録項目に本文・宛先・対象名を含めない。外部送信を伴った実行は `trace_id` でトレーサビリティログを指す。

**FR-SK-14（MUST, All）**: L1 実行の undo（FR-AG-05、7 日）は `skill_runs.undone_at` で管理し、Activity ペインからワンクリックで実行できる。

### 9.4 UI

**FR-SK-15（MUST, All）**: Full UI に Skills ペインを設ける。一覧は §6.1 の項目・既定ソート・フィルタ・検索を持つ。**人気順ソートを実装しない**（v1）。

**FR-SK-16（MUST, All）**: 詳細パネルは §6.2 の必須セクションをマニフェストから自動生成する。新しい Skill の追加でパネル側のコード変更を必要としないこと（Issue のゴール「エンジニアリング作業を最小限に」の受け入れ条件）。

**FR-SK-17（MUST, All）**: `Locked` Skill はカタログに表示するが、アップグレード CTA は詳細画面に 1 つのみとし、一覧には出さない（FR-CF-05 の思想の適用）。

**FR-SK-18（MUST, All）**: Notch Expanded のアクションは帰属 Skill 名を補助行に表示する。Notch に Skill 一覧を実装しない。Skill 起因のエラーはモーダルを出さず既存インジケータ色規則に従う。

**FR-SK-19（MUST, All）**: Skills ペインの SLO は §6.6 の表に従い、計測コードを同梱する。

**FR-SK-20（MUST, All）**: 設定画面（FR-SET-01）の **Agents セクションは Skills ペインへ統合**する。設定側に残すのは「L2→L3 の引き上げ（FR-SET-05）」と「L1 自動実行の通知粒度」のみとし、プリセットの個別有効/無効は Skills ペインを唯一の操作面とする（同じ状態に 2 つの操作面を作らない）。

### 9.5 API・計測

**FR-SK-21（MUST, Pro）**: Memory API に §7 の 5 ツールを追加する。MCP / CLI / REST の 3 面で機能差を作らない。

**FR-SK-22（MUST, Pro）**: `Consent` または `Permission` を要求する Skill は API 経由で有効化できない。`requires_human_consent` を返し、Full UI へのディープリンクを添える。

**FR-SK-23（MUST, All）**: §8.1 の分析イベントを実装する。設定値・対象名・キャプチャ内容を props に載せない。`analytics.json` の `opt_out` を唯一の判断材料とする（旧 `privacy.json` を参照しない）。

**FR-SK-24（MUST, All）**: `manifest_version` 変更時、`settings_json` の未知キーは破棄し欠損キーは既定で補う。`skill_state` 行と `skill_runs` を破棄しない。

---

## 10. 受け入れ基準

### 自動テスト

| # | 内容 |
|---|---|
| T1 | マニフェスト網羅: 全 Skill が全必須フィールドを持ち、全 `operations` が `presets::Operation` に解決できる |
| T2 | level 導出: マニフェスト由来の全 operation の level が `OpKind::mandated_level()` と一致する（既存 presets テストの拡張） |
| T3 | **アーキテクチャテスト**: `skills` モジュールが `Level` を構築せず、`resolve_status` が `mandated_level` の結果に影響しない（依存グラフ検査。FR-CF-01 の既存アーキテクチャテストと同じ機構） |
| T4 | 候補帰属: すべての `ActionCandidate` が既知の `skill_id` を持つ（FR-CF-04 の汎用アクション経路を含む） |
| T5 | OFF 抑止: OFF の Skill が Fusion 候補 / 音声候補 / `skills.execute` のいずれにも現れない |
| T6 | 枠の繰り上がり: OFF による除外後も候補が 4 件まで埋まる |
| T7 | トグル純粋性: 同一 operation の level が `skill_state` の値に依存しない（プロパティテスト） |
| T8 | consent 保護: `meeting_notes` / `visual_recall` / Composio 依存 Skill が API から有効化できない |
| T9 | 権限非発火: トグル ON が TCC / OAuth を呼ばない（モック検証） |
| T10 | 永続化: ON/OFF・設定が再起動後に復元される。`manifest_version` 変更時の互換処理（FR-SK-24） |
| T11 | 履歴の無内容性: `skill_runs` に本文・宛先列が存在しない（スキーマ構造テスト） |
| T12 | 分析の無内容性: 全イベント props に許可キー以外が存在しない（許可リスト検査） |
| T13 | 対称性: UI からの ON/OFF と `skills.set_enabled` が同一状態に収束する |
| T14 | エンタイトルメント: プラン × Skill のアクセス制御マトリクス網羅（既存 entitlement テストへの追加） |
| T15 | UI: `Locked` Skill のアップグレード CTA が全画面通じて 1 つ以下 |
| T16 | マイグレーション: V15 適用・ロールバック・再適用でデータが壊れない |

### 手動検証（macOS 実機）

| # | 内容 |
|---|---|
| M1 | Skills ペイン 13 件が正しい状態で並ぶ（未接続・Pro・OFF の 3 状態を含む構成） |
| M2 | `calendar_scheduler` を ON にしても権限ダイアログが出ないこと。Connect を押した時だけ OAuth が始まること |
| M3 | `reply_drafter` を OFF にすると Notch の返信候補が消え、次点候補が繰り上がること |
| M4 | 音声で OFF の Skill を名指しした時、有効化導線が返ること |
| M5 | `meeting_notes` を Skills ペインから ON にした時、既存の会議ノート同意フロー（FR-MT-03 開示）が必ず通ること |
| M6 | SLO 計測: 一覧 150ms / 詳細 100ms / トグル反映 200ms（p50・p95 を PR 本文に貼る） |

---

## 11. 実装単位

Issue #57 を親とし、以下を子 Issue に割る想定。**S1〜S2 は UI を含まず、S3 以降のやり直しコストを下げる。**

| # | 単位 | 内容 | 依存 | 規模感 |
|---|---|---|---|---|
| **S1** | マニフェスト＋状態解決 | `skills/manifest.rs`（13 件）、`SkillStatus`、`resolve_status` 純関数、T1〜T3・T7 | なし | 中 |
| **S2** | 永続化＋view | V15 マイグレーション、`skill_state` / `skill_runs` 読み書き、`skills_view` / `skill_set_enabled` / `skill_update_settings` コマンド、T10・T11・T16 | S1 | 中 |
| **S3** | Full UI Skills ペイン | 一覧・詳細パネル・自動生成レンダラ・空/エラー状態、T15・M1、SLO 計測 | S2 | 大 |
| **S4** | 結線 | `ActionCandidate.skill_id`、OFF 抑止、Notch 帰属表示、音声マッチ、`skill_runs` 書き込み、設定 Agents セクション統合、T4〜T6・T13・M3・M4・M6 | S2 | 大 |
| **S5** | API＋計測 | `skills.*` 5 ツール（MCP/CLI/REST）、consent 拒否規則、分析イベント、オンボーディング完了画面の 1 行、T8・T12・T14 | S4 | 中 |

先に S1 を単体でレビューに出すこと（マニフェストの粒度と `off_effect` の文言が、この設計で唯一やり直しの高い部分のため）。

---

## 12. 既存文書への改訂点

### `docs/requirements-v1.0.md`

| 箇所 | 改訂 |
|---|---|
| §2 用語集 | **Skill** / **Source** / **SkillStatus** を追加。「Skill は権限の単位ではない」を定義文に含める |
| §6.5 FR-CF-01/03 | `ActionCandidate` に `skill_id` を追加。OFF Skill の除外をスコアリング前段として明記 |
| §6.6 FR-AG-18 | 実行履歴の実体を `skill_runs` とし、記録項目に `skill_id` / `surface` を追加 |
| §6.11 FR-API-02 | 公開ツール表に `skills.*` 5 件を追加 |
| §6.13 FR-OB-01 | ステップ 7 の文面に Skills への 1 行導線を追加（**ステップは増やさない**） |
| §6.15 FR-SET-01 | Agents セクションを Skills ペインへ統合（FR-SK-20）。設定側は L2→L3 引き上げと L1 通知粒度のみ |
| **§6.17（新設）** | FR-SK-01 〜 FR-SK-24 |
| §7.1 SLO | Skills ペイン 150ms / 詳細 100ms / トグル反映 200ms を追加 |

### `CLAUDE.md`

不変条件の追加・改訂は**不要**。ただし「データモデルの原則」に 1 行加えることを推奨:

> - **Skill は表示・起動の単位であり、権限の単位ではない。** Skill の ON/OFF が L1/L2/L3 判定・同意・権限・プラン境界を変えるコードを書かない

### `docs/design-system/components.md`

一覧行（`SkillRow`）と詳細パネル（`SkillDetail`）、状態ドットの 6 状態を追加。

---

## 13. Non-Goal と未決事項

### v1 で作らないもの（Issue の Non-Goal に加えて本設計が追加するもの）

| 項目 | 理由 | 時期 |
|---|---|---|
| 人気順ソート | サーバ集計が必要でローカルファースト方針と衝突（C3） | v2、§13-Q6 の決定後 |
| ピン留め・お気に入り | 13 件でスクロールしない。ローカル使用順ソートが同じ課題を解く | v2（Skill 数が 25 を超えたら） |
| ユーザー定義 Skill / プロンプト編集 | Issue Non-Goal（プロンプト最適化）に該当。設定は宣言 3 種に絞る | v2 |
| サードパーティ配布・マーケットプレイス | Issue Non-Goal | v2 以降 |
| Skill 単位の課金・利用上限 | Issue Non-Goal | — |
| Skill の合成（複数 Skill を束ねる「セット」） | Issue の「テンプレートから Skill セット」に相当。既定 ON 構成が事実上のセットとして機能するか、まず v1 で観測する | v2 |
| Notch 内の Skill 一覧 | 100ms 予算と「押して終わる」体験に反する | 作らない |

### 未決事項（オーナー判断が必要）

| # | 論点 | 選択肢 | 推奨 |
|---|---|---|---|
| Q1 | UI ラベルを "Skills" にするか | Skills / Abilities / 他 | **Skills 継続**。ブランド規約の「競合名を出さない」には抵触しない一般名詞であり、ユーザーの語彙として摩擦が最小。ただし UI コピーで store / marketplace / browse を使わない |
| Q2 | N1〜N4 の目標値 | §8.2 の初期値 | 初期値で開始し、v1 リリース後 4 週で再設定 |
| Q3 | `Locked` Skill をカタログに出すか | 出す / 隠す | **出す**（期待値制御。CTA は 1 つに制限） |
| Q4 | `calendar_scheduler` / `issue_triage` の既定 | ON / OFF | **OFF**（L3 の出現頻度を上げない）。ただしオーナーが「トライアル中の価値提示」を優先するなら ON も筋は通る |
| Q5 | Skills ペインのナビ位置 | Today の直後 / 設定内 | **Today の直後**（設定内に置くと `ambient` Skill の存在に気付けない） |
| Q6 | 人気順のためのサーバ集計を将来行うか | 行う / 行わない | 判断保留。行う場合は匿名集計の設計を別 Issue で（現在の PostHog 単一系統では skill_id 別カウントの取得までは可能） |
| Q7 | `voice_ask` / `visual_recall` を v1 カタログに含めるか | 含める / 実装完了後に追加 | **含める**（存在と OFF 手段を先に見せる方が誠実）。ただし未実装なら `Maturity::Beta` と `rollout: Internal` |

---

## 14. この設計が答えていること

Issue #57 の 4 つのゴールに対する対応:

| Issue のゴール | 本設計の回答 |
|---|---|
| ユーザーが「何をさせられるか」を Skill 単位で理解できる | Skills ペイン（§6.1）＋ Notch からの帰属逆流導線（§4.4）。**探させずに、出会った能力を理解できる**形にした |
| 新規ユーザーの Skill 認知・有効化率 X% | 指標を「有効化」から「到達」に置き換えた（§8.2 N1）。既定 ON 構成では有効化操作が発生しないため |
| 新 Skill をメタデータ定義だけで UI に載せる | マニフェスト＋宣言的 `SettingSpec` からの自動生成（FR-SK-16、受け入れ基準に明記） |
| Skill 単位のログ・計測 | `skill_runs`（ローカル、無内容）＋ PostHog 7 イベント（§8.1）。誤提示ガードレール G1 を設計に組み込んだ |
