# 朝・夜のサマリー配達体験（Issue #10）

- **決定日**: 2026-08-15（オーナー確定: 通知=notchのみ / 朝=初回操作時・夜=固定時刻 / 夜=notchカード / Charm一言・サウンドcue・API対称を含む）
- **UI レビュー反映（同日 v2/v3、モック: claude.ai/code/artifact/c48602d9）**:
  - **Full view リンクは置かない** — Full UI に依存しない設計思想。カードは notch 内で完結
  - ヘッダは「Morning brief」ではなく**挨拶**（`Good morning` / `Good evening`＋日付）
  - カードは**ノッチが開く**表現（黒→ガラスのグラデでノッチから注がれる。浮遊パネルにしない）
  - **各行にデータソースへのディープリンク チップ**（Mail スレッド / Notion ページ / Slack メッセージ / Calendar 予定）。provenance イベントが URL を持てば直接、無ければアプリへフォーカス
  - ハンドル通知の**赤丸は廃止**。到着は確定ロゴ（兜マーク、`Logo.tsx` と同一ジオメトリ）＋ブランドブルーのソフトグロー。⚔ 絵文字はロゴに置換
- **状態**: 設計確定・実装未着手
- **関連**: FR-MB-01..06（Morning Brief）/ FR-EB-01..03（Evening Wrap）/ Issue #41（Charm）/ 不変条件 5・6

## 1. 原則

**時刻が来たら出すのではなく、「その時刻を過ぎた後、ユーザーがそこにいる瞬間」に出す。**
不在中に黙って流れる通知は無いのと同じ。macOS 通知センターは使わない（権限プロンプト不要・体験を notch に集約）。自動展開もしない（作業中断はエラーですら色で伝える規約に反する）。

## 2. 配達条件（Rust・純粋ロジック `daily_delivery.rs`）

| | Morning | Evening |
|---|---|---|
| トリガー | **そのローカル日の最初のユーザーアクティビティ**（既存のグローバル入力モニタのイベントを利用。画面ロック解除・キー・ポインタのいずれか） | 設定時刻（既定 **17:30**）を過ぎた後の最初のアクティビティ |
| 内容 | `briefs` 行（Dream Cycle が夜間 UPSERT 済み）。行が無ければ劣化 brief をその場組み立て（FR-MB-04） | `Db::evening_wrap`（`local_wrap_window` で当日窓。カレンダーは接続済みなら供給、無ければ空） |
| 抑制 | 1日1回。既読（カードを開いた）or 日付変更でリセット | 同左 |
| 状態 | `daily_summaries.json`（app-data、非秘匿）: `{ morning_seen_date, evening_seen_date, evening_hour, evening_minute, morning_enabled, evening_enabled }` | 同左 |

判定関数は `fn due(now_local, settings, seen_state, activity) -> Option<Which>` の形で純粋に切り、Linux でテスト。deep-sleep 明け・日付跨ぎ・時刻変更（設定で過去時刻に変えた場合は当日再判定しない）をテストで固定する。

## 3. UI

### 3.1 ハンドル通知（閉じた notch）

既存 `noticeLine` 系に `summary` トーンを追加。表示は兜マーク＋`Good morning` / `Good evening`（ロゴは `Logo.tsx` の `MarkFacets` を流用）。赤丸ではなくブランドブルーのソフトグローで気配を出す。クリック→ノッチが開いて該当カード。既読になるまで保持。到着時に控えめなサウンド cue（`SummaryReady`、quiet hours 尊重）。

**cue の在席ゲート（2026-08-15 実装決定）**: 判定はポーリングで走るが、cue は「直近 60 秒以内にグローバル入力があった」ときだけ鳴らす（`daily_summaries::LAST_GLOBAL_INPUT_MS`。既存の tap-to-draft グローバルモニタと panel の `interact` が刻む）。不在中に閾値を過ぎても cue は保留され、戻ってきた最初のアクティビティで鳴る — §2 の「割り込まず、そこにいる最初の瞬間に」を音にも適用する。notice のグロー自体は即時に灯る（不在中は誰も見ていないので害がない）。

### 3.2 Morning カード（notch パネル内・既存 Today ビューの流用＋Charm 行）

```
(兜) Good morning            Fri, Aug 15
今日の強みの一言（Charm line、生成時のみ・後述）
Today          カレンダー ≤3（Updated マーク=FR-MB-06）
Commitments due ≤5（possibly ハーフトーン=FR-MB-05）
Open loops      ≤5
```

### 3.3 Evening カード（新規）

```
(兜) Good evening            Fri, Aug 15
Today: {commitments_done} done · {loops_closed} loops closed
       · {actions_adopted}/{actions_decided} actions adopted
Still open      ≤5（overdue→期日→staleness 順）
Tomorrow first  カレンダー ≤3 ＋ 明日期日 commitments
Loose ends      ≤5
```

すべての行は provenance（根拠イベント）参照付きで、**右端にソースチップ**（Mail / Notion / Slack / Calendar）を置きワンクリックで元データへ飛ぶ。**生成プロースは無い**（FR-EB-02: Wrap は決定的集計のみ）。

### 3.4 設定

Settings に「Daily summaries」セクション: Morning ON/OFF、Evening ON/OFF＋時刻ピッカー。`Shougun.md` に `# DailySummaries` を将来追加できる余地を残す（v1 は Settings のみ）。

## 4. Charm 一言（Morning のみ）

- **生成は夜間の MorningBrief ジョブ内**（Batch/Select KK レーン、不変条件5準拠）。`Shougun.md` の `# Charm` が存在し Batch レーンが使えるときのみ、当日のカレンダー見出しと `Charm.CoreStrengths` から1行を生成し `BriefPayload.charm_line: Option<String>` に保存
- **配達時の LLM 呼び出しはしない**（オフラインでも朝は完全動作。劣化 brief では行ごと省略=正直な劣化）
- `NGCharmPatterns` はプロンプトに含め、出力は redact 済みで保存

## 5. API 対称（不変条件6）

- `memory.get_wrap` を MCP tool / `GET /v1/memory/wrap` / `shogun wrap` の3面に追加（Read、confidence ゲートは既存どおり）
- brief は既存 `briefs` 読みで API 化余地あり（本イシューでは wrap のみ必須）

## 6. 実装順

1. **M1**: `daily_delivery` 純粋ロジック＋`daily_summaries.json`＋Tauri コマンド（`evening_wrap` / `summary_state` / `mark_summary_seen` / 設定 get/set）— **済 2026-08-15**（＋`morning_card` / `open_summary_source` を M2 で追加）
2. **M2**: ハンドル notice＋Evening カード＋Morning カード導線＋Settings セクション＋`SummaryReady` cue — **済 2026-08-15**（`daily.tsx` カード、ソースチップ=接続サービス名 or キャプチャ元アプリ名、チップクリックで `open_summary_source`。イベントに URL カラムは無いため v1 はアプリ/サービスの前面化。`BriefPayload.charm_line` フィールドも serde default で先行追加済み）
3. **M3**: `charm_line` の生成（MorningBrief ジョブ拡張）＋`memory.get_wrap` 3面 — **済 2026-08-15**。charm は `Summarizer::charm_line(CharmRequest)` シーム（既定 None=劣化夜は行ごと省略。`DbDreamRunner::with_charm` で `# Charm` を注入。Batch abstractive summarizer 実装 PR が実生成を担う）。wrap は `Tool::MemoryGetWrap` = `memory.get_wrap` / `GET /v1/memory/wrap` / `shogun wrap` の3面対称（structured read、`db_backend::evening_wrap_json` が カードと同じ `Db::evening_wrap` を配る）
4. 受入: 純粋ロジックのテスト（境界3種）、wrap コマンドの統合テスト、デスクトップは vitest でカード描画＋既読遷移

## 7. やらないこと

- macOS 通知センター・自動展開・メール/モバイル配信（スコープ外）
- Evening への生成プロース追加（FR-EB-02 違反）
- 配達時刻ぴったりの割り込み（原則1に反する）
