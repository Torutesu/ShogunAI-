# 仕様たたき: コンテキストソースの優先度・信頼度・鮮度ポリシー

> Issue #35。設計正本: `docs/context-architecture-design.md`（段階計画 P2〜P3）、`docs/requirements-v1.0.md`、`CLAUDE.md`（不変条件）
> ステータス: たたき台（Issue #35 コメントで合意した方向性の仕様化。レビュー前提のドラフト）
> 方向性メモ: https://github.com/Torutesu/ShogunAI-/issues/35#issuecomment-5299760265

## 1. 背景 / Why

コンテキストの取得元（AX キャプチャ / MCP / SaaS API / 会議文字起こし）が複数になるが、
「どのソースをどれだけ信用するか」がソース単位で定義されていない。現状の confidence は
**抽出ルールにのみ紐づく**（ローカル cue 抽出は `LOCAL_RULE_MAX_CONFIDENCE = 0.4` で上限、
`extract.rs:27`）。この設計はヒューリスティックを事実と断定させないためのもので正しいが、
MCP 経由で取得した Google Calendar の予定＝**構造化された事実**まで同じ経路を通ると
Medium 帯（`possibly:`）止まりになり、Fusion もドラフトも永遠に断定できない。

欠けているのは **ソース単位の信頼度事前分布（source trust prior）** という 1 つの概念と、
それを反映する **構造化データの直接 ingest 経路** である。既存の event_log ソース対称性・
confidence 帯ゲート（`confidence.rs`）・provenance 必須制約（`state.rs` `EmptyProvenance`）は
一切変えない。

## 2. ゴール / Done の定義

1. 全コンテキストソースに trust prior / 鮮度ポリシー / スコープが表として定義され、
   新ソース追加時にこの表へ 1 行足せば優先度設計が終わる状態
2. MCP/API 経由の構造化事実（カレンダー予定等）が **High 帯（≥0.8）の state 行**として
   生成され、Fusion / Morning Brief / ドラフトで断定的に使える
3. 同一エンティティが複数ソースから来たときのマージ結果が決定的（テストで再現可能）
4. MCP/API 障害時にローカルのみで破綻しない回答を返し、不足がログで検知できる

## 3. スコープ

**In:** ソース分類表 / effective_confidence の式 / 構造化 ingest 経路 / エンティティ競合解決 /
鮮度減衰 / トークン枠配分の方針 / フェイルセーフ / 計測イベント定義 / テストケース

**Out（Issue #35 Non Goal どおり）:** 個別 MCP・SaaS API の接続実装（認証・スキーマ詳細は
P2 実トランスポートの仕事）/ プロンプトテンプレート全文 / モデル選定 / ユーザーが優先度を
カスタマイズする設定 UI（将来拡張の余地のみ残す）/ データ保持ポリシー全般 /
**confidence 数値・優先度の一般ユーザー向け表示**（§11 の露出原則どおり出さない）

## 4. 設計原則: 優先度は「ソース」ではなく「情報の性質 × 次元」

単純な「MCP > テキスト」は誤り。**何についての情報かで権威が逆転する**:

| 次元 | 最権威ソース | 理由 |
|---|---|---|
| 事実（予定・タスク・Issue 状態） | MCP/API 構造化データ | サービス側が正本。推論不要 |
| 今この瞬間の状況・意図（screen_ctx） | AX キャプチャ | 「今」見ているものはカレンダーに無い |
| 約束・コミットメント | 会議文字起こし・メール本文 | 発話が根拠。構造化データに存在しない |

したがって優先度は「ソースの格付け」ではなく、**そのソースが生む情報の型ごとの
事前信頼度（trust prior）**として定義する。

## 5. ソース分類表（正本）

新ソース追加時はこの表に 1 行追加する。Rust 側は `shogun-memory` に静的表として置く
（DB に持たない。ポリシーはコードとともにバージョン管理され、マイグレーション不要）。

| source | 情報の型 | ingest 経路 | trust prior | 鮮度ポリシー（半減期） | スコープ |
|---|---|---|---|---|---|
| `gcal`（構造化: 予定） | 事実 | **構造化直接**（§7） | 0.90 | 同期成功中は減衰なし。amber/切断で TTL 24h → Medium 降格 | 全期間の予定 |
| `gmail` / `slack` / `notion` / `github` / `linear`（構造化メタ: 件名・状態・期日） | 事実 | **構造化直接**（§7） | 0.85 | アイテム自身の `ts_ms` 基準で 7 日半減 | 同期ウィンドウ |
| `gmail` / `slack` 本文（自然文） | 約束・文脈 | text 抽出（現行 `extract` → Dream 精緻化） | 現行どおり（ローカル ≤0.4 → Batch 較正） | `ts_ms` 基準 7 日半減 | 同期ウィンドウ |
| `capture`（AX テキスト） | 状況・約束候補 | text 抽出（現行） | 現行どおり（≤0.4 → Batch 較正） | state: 現行の減衰。screen_ctx: 分単位（cache 更新で常時上書き） | 直近の作業 |
| `meeting`（文字起こし） | 約束・決定 | text 抽出 → Dream 精緻化 | 現行どおり | 会議終了時刻基準 7 日半減 | 会議単位 |
| `agent` / `user`（自己生成・ユーザー明示） | 事実 | 直接 | user=1.0 / agent=生成時の確度 | ユーザー明示は減衰なし | — |

- **screen_ctx（ライブ文脈経路）はこの表の対象外**。DB に書かず表示・Fusion 入力専用という
  現行設計（`integrate.rs` → `axcache`）を維持。screen_ctx は常に「今」の最権威であり、
  confidence ではなく relevance 側（`assemble.rs` の `screen_relevance` → 将来は埋め込み）で効かせる。
- trust prior の数値はレビューで調整可。**不変なのは順序**: ユーザー明示(1.0) > 構造化事実(0.85–0.9) >
  Batch 較正済み抽出(可変・較正値) > ローカル cue 抽出(≤0.4)。

## 6. effective_confidence の式

state 行の保存値と読出時評価を分離する:

```
保存時:  stored_confidence = ingest 経路ごとの基礎値
           - 構造化直接経路: trust prior（§5 の表）
           - text 抽出経路:  現行どおり（ローカル ≤0.4 / Dream 較正値）
読出時:  effective_confidence(t) = stored_confidence × freshness_decay(source, t)
```

- `freshness_decay` は §5 の半減期に基づく指数減衰（`recompute.rs` の減衰パスを拡張。
  絶対値の再計算・冪等という現行の性質を維持し、累積させない）
- 帯判定（High/Medium/Low, `confidence.rs`）は **effective_confidence に対して**行う。
  帯の境界・`possibly:` 処理・Low 破棄（FR-ST-20）は一切変更しない
- 構造化事実の「同期成功中は減衰なし」は decay=1.0 の特例。接続 FSM（`connection.rs`）が
  amber/切断に落ちて TTL を超えたら decay を掛け始め、High → Medium に自然降格する
  （「切断されたカレンダーを断定に使い続けない」の実装）

## 7. 構造化 ingest 経路（新設）

現行 `IngestItem`（`sync.rs:42`: source/kind/title/body/ts_ms）は本文テキストとして
event_log に入り cue 抽出を通る。構造化事実のためにこれを拡張する:

- `IngestItem` に `structured: Option<StructuredFact>` を追加。
  `StructuredFact` は v1 では最小の enum: `CalendarEvent { start_ms, end_ms, attendees, location }` /
  `TaskItem { due_ms, status, assignee }`。サービス固有スキーマはここに漏らさない
  （normalize がサービス側スキーマ → この enum への変換責務を持つ）
- `Db::ingest_integration`（`daemon.rs:585`）は `structured` があるアイテムについて:
  1. event_log へは現行どおり append（**provenance の根拠イベントになる**。不変
     「根拠なき状態を作らない」を構造化経路でも守る）
  2. cue 抽出（`extract`）を**スキップ**し、`StructuredFact` から state 行を直接生成
     （CalendarEvent → 予定系 commitment、TaskItem → commitment/open_loop）。
     `stored_confidence = trust prior`、provenance は 1 の event_id
- dedup は現行の `content_hash + source` に加え、構造化事実は `external_id` で同一判定
  （予定の時刻変更は「新規」ではなく既存行の**更新**。provenance に新イベントを追加）
- 後方互換: `structured = None` なら現行挙動と完全一致。スキーマ変更はマイグレーション＋
  ロールバック手順を添付（CLAUDE.md 規約）

## 8. エンティティ競合の解決（マージポリシー）

同一エンティティ（同じ会議・同じタスク）に複数ソースが紐づく場合:

1. **正本（canonical）は effective_confidence 最大のソース**。本文・期日・状態は正本から取る
2. **低い側は削除せず provenance として合流**（`state_provenance` 多対多をそのまま使用）。
   複数ソースの一致は Dream Cycle の confidence 加重で**加点**材料になる（現行設計 §4.1）
3. 同一判定は段階導入: v1 は `external_id` 一致＋タイトル/時刻の決定的一致のみ。
   曖昧な名寄せ（cross-channel identity）は未配線の `identity.rs` を Dream Cycle 側で配線
   して担わせる（write 経路でモデルを呼ばない現行原則を維持）
4. **競合時（例: ローカルメモの期日 ≠ カレンダーの期日）**: 構造化事実が勝つ。ただし
   ローカル側の provenance は残るため、トレース/デバッグビューで「メモとカレンダーが
   食い違っていた」ことを後から追える

## 9. トークン枠配分（Fusion 組み立て）

`shogun-fusion` の既存部品に載せる。新機構は作らない:

- `ContextBlock.score_inputs`（`block.rs`: relevance / freshness / task_link / confidence）の
  `confidence` に effective_confidence を、`freshness` に decay 値を渡す（現在 evidence は
  一律 1.0 の箇所を差し替え）
- `SourceKind::Structured` は定義済み。予算充填（`budget.rs`）での優先順位は
  スコア順で自然に決まり、**ソース別の固定枠は設けない**（固定枠は「関連度が低いのに
  構造化だから入る」を生むため）。ただし飢餓防止として screen_ctx 由来ブロックに
  最低 1 ブロックの保証枠のみ設ける（「今の状況」が完全に消えると回答が浮く）

## 10. フェイルセーフ

- MCP/API のタイムアウト・エラー時: **ローカル（capture + 既存 state）のみで組み立てを続行**。
  構造化事実は「最後に同期成功した値」を TTL 内なら使い、TTL 超過は §6 の降格で自動的に
  弱まる。Fusion 側に特別な分岐を足さない（confidence が下がる → 帯が下がる、で吸収）
- 障害はサービス単位に隔離（`connection.rs` FSM の現行性質）。1 サービスの障害で
  他ソースの取得を止めない
- ユーザー向けには不足を自然文で補足（「カレンダーが同期できていないため予定は未確認」相当の
  弱い言明）。エラーの通知はノッチインジケータ色の現行原則どおり、作業を中断させない

## 11. 計測イベント・UI 露出の原則

**UI 露出の原則（2026-08-15 オーナー決定）**: 本仕様の内部値（trust prior / decay /
effective_confidence の数値・帯名）は**一般ユーザー向けサーフェスに一切出さない**。
ユーザーが見るのは結果としての言い回しだけ — High=断定文 / Medium=「possibly」相当の
弱い言明 / Low=そもそも出ない（FR-ST-20 の現行挙動そのもの。本仕様で UI は何も増えない）。
provenance「なぜ？」表示を将来出す場合も、数値ではなく定性的表現（"Based on your calendar"）に
留める。下記デバッグビューは dev ビルド・内部フラグ限定で、一般配布 UI に含めない。

分析イベントに載せるのは**機能カウントのみ**（キャプチャ内容・個人データ禁止の現行規約）:

- 組み立て毎: 採用ソースのフラグ集合（`sources_used: {gcal, capture, ...}`）、
  ソース別ブロック数、構造化事実の採用有無、フォールバック発生有無
- レイテンシ分離: ソース取得時間 / 組み立て時間 / LLM 呼び出し時間（SLO: ボタン提示 150ms /
  cache 更新 300ms との突き合わせ用）
- デバッグビュー（開発者向け、Issue #35 Design 節）: あるリクエストで「どのソースが
  どの effective_confidence で採用/棄却されたか」の一覧。traceability と同様、本文は持たせない

## 12. テストケース（再現可能な受け入れ基準）

| # | 前提 | 操作 | 期待 |
|---|---|---|---|
| T1 | gcal 接続済み | CalendarEvent を構造化 ingest | state 行 confidence=0.90、High 帯、`possibly:` なしで fact 出力（`treat_fact`） |
| T2 | T1 の予定と同じ会議のローカルメモを capture | 両方 ingest | state 行は 1 つ。正本=gcal、provenance は 2 イベント |
| T3 | T2 でメモの期日がカレンダーと食い違う | ingest | 期日はカレンダー値。メモ側 provenance は残存 |
| T4 | gcal が amber、TTL 24h 超過 | 読出 | effective_confidence < 0.8 に降格、`possibly:` 付きで出力 |
| T5 | MCP タイムアウト | 組み立て | ローカルのみで応答が返る。フォールバックフラグがログに記録 |
| T6 | `structured=None` の従来アイテム | ingest | 現行挙動と完全一致（cue 抽出、≤0.4） |
| T7 | 同一 external_id の予定が時刻変更で再同期 | ingest | 新規行を作らず既存行を更新、provenance 追加 |
| T8 | ローカル cue 抽出のみの候補 | 読出 | どのソースでも 0.5 未満（High/Medium に混入しない） |

## 13. 不変条件チェック

- ✅ データ重心は Rust コア（表・式・マージすべて `shogun-memory` / `shogun-core` / `shogun-fusion`）
- ✅ 同期読み取りは egress しない（trust prior の評価はクライアントローカル。Issue #35 権限節どおり）
- ✅ provenance 必須: 構造化経路でも根拠 event_id なしの state 行は作らない
- ✅ FR-ST-20 の帯・`possibly:`・Low 破棄は不変。変えるのは confidence の**入力値**のみ
- ✅ 鍵レーン分離: 構造化 ingest はモデルを呼ばない。Dream 較正は従来どおり Select KK Batch
- ✅ 後方互換マイグレーション（`structured` は Option、既存データは無変更）
- ✅ 分析イベントに内容を載せない（フラグとカウントのみ）

## 14. 段階計画上の位置づけ

`docs/context-architecture-design.md` §5 の **P2（実トランスポート）と P3（Dream Cycle）の間**。
P2 でコネクタから実データが流れ始める前に本仕様を確定させると手戻りがない。実装順の目安:

1. ソース分類表＋decay 拡張（`shogun-memory`、純ロジック・Linux テスト可）
2. `StructuredFact` と構造化 ingest 経路（`sync.rs` / `daemon.rs`、マイグレーション含む）
3. Fusion の score_inputs 差し替え＋screen_ctx 保証枠（`shogun-fusion`）
4. 計測イベント＋デバッグビュー（P1 の Traceability ビューアに相乗り）
