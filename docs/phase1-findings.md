# Phase 1 findings — 進捗記録と引き継ぎ

計画書 §8.6 の指示に基づく横断セッションの引き継ぎ文書。実機実測値もここに追記する。

---

## 進行中 WP と次の一手（最新を先頭に）

### 現在地（このセッション終了時点）

- **Linux 完結・純ロジックの井戸は実質枯渇**。連携・メモリ・Fusion・Dream Cycle・エージェントの純ロジック層は端から端まで実装・テスト済み。
- 残タスクは全て **①macOS実機（Category B）②実secret/アカウント（Category C）③UI/レビュー手順** のいずれかが入口。
- **次の一手 = 実機UIテスト**。手順は `docs/phase1-ondevice-runbook.md`。パネル描画に依存しない ⌘⇧J self-test でコアを先に確認し、描画は M1 製品shell（WP1.1）で置き換える。

### 実機で回収すべき持ち越し

- Q1 常駐ソーク（≥24h）— M2 ゲート。
- SLO-01/02 実測（p50/p95）— `shogun metrics` が measured:true になったら Phase 0 実測（展開 p95≈18ms）と整合確認。
- パネル描画の安定性 — 脆い。M1 製品shellで解消する前提。

---

## このセッションで積んだもの（純ロジック、全て Linux green・pushed）

ブランチ: `claude/shogunai-requirements-prep-nm2tf4`

| 領域 | 内容 | 主な FR/NFR |
|---|---|---|
| SLOメトリクス | `shogun metrics` + `/v1/metrics`（未計測は measured:false、沈黙を成功と読まない） | NFR-SLO-00 |
| 連携 read 取り込み | `mcp::sync::collect_sync` → `Db::ingest_integration`。同期→event log（source別）→検索/Fusion 合流 | FR-INT-05, §6.9 |
| L3 送信トレース | `send_exec::execute_send`。承認確定→送信→**成功時のみトレーサビリティ必須記録**（digest のみ、本文非保存） | 不変条件3, FR-TR-03, §6.14 |
| 連携アイテム抽出 | `ingest_integration` が新規取り込み本文に第1段抽出を適用→state tables（低confidence） | WP2.7, FR-ST-02 |
| Slack fallback | `slack::resolve_post`。投稿ブロック→クリップボードドラフト降格（L3→L2、送信ではない） | FR-INT-30 |
| 空パネル禁止 | `assemble` が state 無し時に汎用アクション（Save note/Search memory/Extract tasks） | FR-CF-04 |
| Dream Cycle サマリ | `Db::summarize_dream_run`。処理イベント数・state変更数・送信チャンク数・所要時間を DB 差分で算出 | FR-DC-06 |
| 名寄せ | `identity::resolve`。exact チャネル一致のみ自動統合、名前のみ一致は閾値未満（誤統合回避） | FR-ST-10 |

補助追加: `event_log::count_in_range` / `traceability::count_since` / `state::count_changed_since` + `ALL_STATE_TABLES`。

### ガードレール状況（CI 相当・全 green）

- `cargo test --workspace --exclude shogun-desktop-spike` — green
- `cargo clippy … --all-targets`（default + daemon-server）— 警告ゼロ
- `check-http-egress.py` / `check-secret-exposure.py` / `check-migrations.py` — 全 OK
- 不変条件 1–7 は型＋テスト＋CI で維持。

---

## 要件カバレッジ棚卸し（156 FR/NFR/AR）

コード未参照 74 件の内訳（横断調査結果）:

- **Category B（macOS実機）**: FR-NU-02〜07（Notch UI）, FR-CAP-04/07/08/09, FR-AG-05, FR-OB-01〜05
- **Category C（実secret/アカウント）**: FR-BIL-01〜09, FR-INT-01/02/04, FR-AG-17, FR-C2 系
- **UI/プロセス**: FR-SET-01/04/05/08, FR-API-05, FR-MB-02/03
- **既に別実体/ガードで担保**: FR-CF-01/02, FR-DC-02, FR-ST-03/12, FR-MEM-31, FR-AG-18

結論: Linux 単独で新規に積める純ロジックはほぼ無い。以降は実機・実secret・UI が入口。

---

## 実機実測ログ（このセクションに追記していく）

> 実機セッションで観測した結果をここに貼る。テンプレ:
>
> ```
> ### YYYY-MM-DD 実機セッション
> - 環境: macOS __ / __" MBP / 外部ディスプレイ有無
> - ⌘⇧J self-test: N actions, top disposition = __
> - capture: __ 件/分, 抽出候補 __ 件
> - SLO-01 展開 p50/p95 = __ / __ ms
> - 詰まった点 / 次の一手:
> ```

（未実施）
