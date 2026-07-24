# コネクタ実装計画 — 実I/Oアダプタ層

`docs/requirements-v1.0.md`（§6.9〜6.11）で確定済みのコネクタ設計を、実際に
ネットワーク接続して動かすための**アダプタ層**の実装計画。実装は別モデルに委任する前提。

- 作成日: 2026-07-23
- 対象ブランチ: `claude/shogunai-requirements-prep-nm2tf4` の実装状態を土台にする
- **重要**: コネクタの「設計」は既に完了している。本書は設計ではなく、
  既に空いている継ぎ目（trait）を実装で埋めるための計画である。

## 実装状況（2026-07-23 更新 / 対象: Google Workspace 経由の第1層）

スコープ判断: **公式リモートMCPが実在するのは Gmail / Calendar / Drive のみ**（Google
Workspace Developer Preview で確認）。**Docs / Sheets には専用MCPサーバーが無い**ため、
その中身はDriveの `read_file_content` 経由で読む（独立サービスにしない）。Gmail MCPには
`send` ツールが無く scope も `gmail.compose` 止まり＝送信は第2層Composioのまま（要件通り）。

この環境（Linux）で**検証できる純ロジックは実装・テスト済み**。実I/O（Keychain=macOS専用、
OAuthブラウザフロー、ライブMCP接続）はmacOSビルドが必要で、継ぎ目まで用意した:

| 内容 | 状態 |
|---|---|
| `shogun-mcp/scope.rs` に `GoogleDrive` サービス追加（Wave 1、read/read_on_demand/file_create=L3） | ✅ 実装・テスト済 |
| `shogun-mcp/sync.rs` にDriveの `item_kind`（file） | ✅ |
| 新規 `crates/shogun-integrations`（純層: endpoints / toolmap / result正規化 / rpc seam / transport） | ✅ 実装・テスト済（clippyクリーン） |
| `RemoteMcpTransport` が `IntegrationTransport` + `WriteExecutor` を実装（read_sync + execute書き込み） | ✅ 純ロジック検証済 |
| **OAuth 2.1 + PKCE（`oauth.rs`）**: PKCE導出・authorize URL・token交換/refreshフォーム・レスポンス解析・redirect解析 | ✅ 純ロジック実装・テスト済 |
| **daemon配線骨格（`runtime.rs`）**: `ConnectorRuntime`（sync_service / services_due / poll_tick / execute_write）+ `IngestSink` seam | ✅ 純ロジック実装・テスト済（32テスト） |
| **トークンライフサイクル（`token.rs`）**: TokenStore seam・シリアライズ・`TokenManager`（期限切れ検知→自動リフレッシュ→保存） | ✅ 純ロジック実装・テスト済 |
| **接続状態一覧（`runtime.rs::statuses`）**: 各サービスのconnected/amber/disconnected/coming_soon + 鮮度 + endpoint有無 | ✅ 実装・テスト済 |
| `live`: reqwest JSON-RPC・OAuthループバック(`oauth_flow.rs`)・**Keychain TokenStore**・**自動リフレッシュProvider**(`ManagedTokenProvider`) | ⚠️ コンパイル可・**実接続はmacOS+実トークンで未検証** |
| **core実結線**: `Db` が `IngestSink` 実装（`db`フィーチャで依存追加） | ✅ Linuxでビルド確認済 |
| **デスクトップ実結線（`connectors.rs`）**: Tauriコマンド `connect_service`/`disconnect_service`/`connectors_list`、runtime所有、15分ポーラー、lib.rs登録 | ⚠️ macOS専用・**Linuxでコンパイル不可のため未検証**（ラフ実装） |
| **接続管理UI（`Connections.tsx`）**: サービス一覧＋Connect/Disconnect＋状態表示 | ✅ 設定ウィンドウにマウント済み・CI frontend green（見た目は後回しのラフ） |
| **async connect** + `open_settings` + settingsウィンドウ + capabilities | ✅ CI macOS green |
| **Wave 2 Slack（OPEN-03解決）**: 公式MCP `mcp.slack.com/mcp` 実在確認 → endpoints/toolmap/oauth(Slackネスト形式トークン対応・`AuthConfig::slack`) | ✅ 純ロジック実装・テスト済（ツール名は暫定 — wire-up時に `tools/list` と突合。Wave解放はFR-INT-03のゲート判断） |
| **Wave 3 Notion/GitHub/Linear**: 公式リモートMCP実在確認（`mcp.notion.com/mcp`, `api.githubcopilot.com/mcp/`, `mcp.linear.app/mcp`）→ endpoints + toolmap（暫定名）。第1層6サービス全部が endpoint を持つ状態 | ✅ endpoints/toolmap実装・テスト済（OAuth AuthConfig各社版とconnect結線はWave解放時。ツール名は`tools/list`と突合） |

| **WP-F 確認済み送信の実行**: `send_bridge`（ルーティング）+ `FirstLayerSendTransport`（第1層、二重ゲート） | ✅ 実装・テスト済 |
| **WP-D Composio Gmail送信**: `ComposioApi`シーム（integrations純）+ `HttpComposioApi`（core net、`/api/v3/tools/execute/{tool}`・x-api-key）+ `ComposioSendTransport` + `RoutedSendTransport`（email→Composio、他→第1層、Composio失敗時はFR-C2-05ドラフト退避） | ✅ 実装・テスト済（実HTTPはライブキーで未検証） |

OAuthクライアント登録の人間側手順は `docs/oauth-client-setup.md` に分離。

| **item B 承認キュー→送信実行の実アプリ結線**: `exec`フィーチャ（送信パスをaxum抜きでdesktopへ）、`ApprovalQueueState`、Tauriコマンド `submit_send`/`list_approvals`/`confirm_send`/`reject_send`、`RoutedSendTransport`結線（Composio失敗時ドラフト退避含む）、L3確認UI（`Approvals.tsx`、全文表示・専用ボタン確認）、設定ウィンドウに統合 | ✅ 結線済み（CI macOSで検証） |

| **item C オンデマンド読み取り**: `IntegrationTransport::fetch_on_demand` シーム + `collect_on_demand`（read_on_demand L2ゲート）+ `RemoteMcpTransport`実装 + `ConnectorRuntime::fetch_on_demand`（ポール schedule非干渉、失敗時amber）+ desktop `fetch_on_demand` コマンド | ✅ エグゼキュータ実装・テスト済（トリガー=auto/tapはcaller判断として中立化。Gmail/Driveのみ対象） |

**残るライブ前提（人間側）**: Composio APIキー(Keychain `composio-api-key`) + connected account
の `SHOGUN_COMPOSIO_USER_ID`。送信の**プロデューサ（エージェントがsendを提案してキューへ）**は
別機能で、現状は `submit_send` コマンドが入口（UI/手動/将来のエージェント共通）。オンデマンド取得の
**フォーカス連動トリガー**（自動 or 明示タップ）はUX判断として未確定 — `fetch_on_demand` コマンドが
中立の入口。

**この環境（Linux）で検証できたのはここまで。** 純ロジック（マッピング・PKCE・ゲート・状態遷移）は
全部テスト付き。実I/O（ネットワーク・Keychain・ブラウザ）はmacOSビルドが必要で継ぎ目まで用意済み。

### daemon実結線（macOSで書く具体コード）

```rust
// 1) crates/shogun-core/Cargo.toml の `db` feature に依存追加:
//    shogun-integrations = { path = "../shogun-integrations", features = ["live"] }

// 2) Db を IngestSink に（既存 Db::ingest_integration を使うだけ）:
impl shogun_integrations::IngestSink for Db {
    fn ingest(&self, items: &[shogun_mcp::sync::IngestItem]) -> usize {
        self.ingest_integration(items).newly_inserted
    }
}

// 3) 起動時: transport を組んで ConnectorRuntime を daemon が所有:
let rpc = shogun_integrations::live::HttpMcpRpc::new(
    shogun_integrations::live::KeychainTokenProvider::new("com.selectkk.shogun"));
let transport = shogun_integrations::RemoteMcpTransport::new(rpc?);
let mut runtime = shogun_integrations::ConnectorRuntime::new(transport, Wave::One, draft_stop);

// 4) 15分ごとの tokio interval が poll_tick を回す（FR-INT-04）:
let mut tick = tokio::time::interval(Duration::from_secs(15 * 60));
loop {
    tick.tick().await;
    for (svc, res) in runtime.poll_tick(now_ms(), DEFAULT_SYNC_INTERVAL_MS, &db) {
        // res: Ok(SyncReport{inserted}) を IntegrationSynced としてバスへ / Err はインジケータ色
    }
}

// 5) 承認キュー(shogun-agents)で confirm 済みの L2/L3 書き込み → execute_write:
//    runtime.execute_write(service, op_name, args_json, &transport)  // 二重ゲート込み
```

**次にmacOS環境でやること**: OAuthクライアントID/secret登録（Developer Preview前提）→
`oauth_flow::run_loopback_flow` を設定画面の「Connect」から呼んでトークンをKeychainへ→
上記4/5をdaemonに結線→`result.rs`のフィールドマッピングを実レスポンスで確認（tolerant実装済）。

---

## 0. 現状の正確な地図（設計と実装のどこまで出来ているか）

### 既に完成しているもの（Rust・純ロジック・Linuxテスト済み）

コネクタのポリシー・状態管理は `crates/shogun-mcp` に実装済みで、テストも通っている:

| モジュール | 役割 | 状態 |
|---|---|---|
| `scope.rs` | 6サービス×操作の許可表（§6.9.2）。表にない操作は拒否、送信は必ずL3 | ✅ 完成・テスト済 |
| `service_gate.rs` | Wave解放・接続状態・draft-stopを合成した認可判断 | ✅ 完成・テスト済 |
| `connection.rs` | サービス別接続状態機械（connected/amber/disconnected, FR-INT-06/07） | ✅ 完成・テスト済 |
| `sync.rs` | read-sync取り込みの合成（gate→transport継ぎ目→正規化） | ✅ 継ぎ目まで完成 |
| `composio.rs` | 第2層Gmail送信の同意ゲート（型でガード）+ prepare_send | ✅ ゲート完成・テスト済 |
| `dispatch.rs` / `memory_api.rs` / `rest.rs` | Memory API（MCP/CLI/REST対称。L3は共通承認キューへ） | ✅ 完成・テスト済 |
| `shogun-agents` | L1/L2/L3の権限語彙・承認キュー | ✅ 完成 |

つまり「どのコネクタが何をできて、どのレベルで、どう拒否されるか」は**全部コードで確定済み**。

### まだ無いもの（実I/O = 本書の対象）

`crates/shogun-mcp` は意図的に「純ロジックのみ・Linuxでテスト可能」に保たれており、
実際のネットワーク・OAuth・Keychainは**デスクトップアダプタ側に切り出す設計**
（`lib.rs` 冒頭コメント: "the MCP client transport and OAuth-to-Keychain live in the
desktop adapter"）。以下が未実装:

1. **実MCPクライアントトランスポート** — `sync.rs` の `IntegrationTransport` trait を
   実装する実体（今はテスト用 `Fake` だけ）。公式リモートMCPへ実接続して読み取る。
2. **OAuth 2.1 + PKCE → Keychain** — サービス別のユーザー直接認可フロー、
   トークンのKeychain保存（FR-INT-02）。
3. **Composio Gmail送信のHTTP実体** — `prepare_send` の先で実際にComposio APIを叩く。
   失敗時は `on_composio_failure()` → ドラフト保存（FR-C2-05）。
4. **同期スケジューラ** — 15分間隔ポーリング + オンデマンド取得（FR-INT-04）、
   `collect_sync` を回して event log へ追記。
5. **daemon/Tauri配線** — 接続/切断コマンド、`ConnectionRegistry` 駆動、設定UI。

**既に手本がある**: 実I/Oの書き方はこのリポジトリ内に前例がある。
- HTTPクライアント: `crates/shogun-core/src/llm/transport.rs` の `ReqwestTransport`
- Keychain: `apps/desktop/src-tauri/src/inline_source.rs` の
  `security_framework::passwords::get_generic_password`（BYOKキー読み出し）
- トレーサビリティ経路: `Route::Composio` / `TraceRoute::ViaComposio` 済み

---

## 1. 最初のご相談への回答（実態版・前回の訂正）

最初に「基本的にComposioで全部やる」という前提で計画したが、**実プロダクトは意図的に
真逆の設計**なので、5つの論点を正しく答え直す。

### 優先コネクタ
Composio経由ではない。**第1層＝各サービスの公式リモートMCPに直接接続**する6サービス固定:

| Wave | サービス | 解放条件 |
|---|---|---|
| Wave 1 | Gmail, Google Calendar | 最初 |
| Wave 2 | Slack | Wave 1が接続成功率95%以上・クラッシュ増なしを2週間 |
| Wave 3 | Notion, GitHub, Linear | Wave 2安定後 |

Composioは**第2層で、v1はGmail送信のみ**（オプトイン・Pro）。Discord/Teams等は
「予定なし」と要件で明記済み。前回の「20コネクタ」は不要。

### 費用
前回のComposio料金分析は**ほぼ的外れ**だった。第1層は公式MCP直結なので
**読み取り・大半の書き込みにComposio料金はかからない**。Composioを通るのはGmail送信だけで、
送信は1ユーザー1日数通レベル＝Free枠（20K calls/月）で数千ユーザーでも十分。
支配的コストは要件§9の通り別にある:
- **Batch API（Select KKキー）**: インデックス・分類・Dream Cycle・Morning Brief。原価の主戦場
- **BYOK（ユーザー負担）**: エージェント推論・チャット・ドラフト
- **ローカルembedding**: 同梱ONNXモデル＝クラウドAPI費ゼロ（FR-MEM-21）

→ コネクタ側のクラウド費用は実質ゼロに近い。コスト管理はBatch APIのチャンク量設計に集約される。

### どう接続するか
OAuth 2.1 Authorization Code + PKCE、**ユーザー→サービス直接**（中間サーバー無し、
Select KK運営サーバーも経由しない, FR-INT-01/02）。トークンはKeychainのみ。
接続先は各サービスの公式リモートMCPサーバー。これが本書 WP-A/WP-B の実装対象。

### スケーラビリティ
**ローカルファースト＝サーバー無し**なので、前回の「ステートレスAPIを水平スケール」は
そもそも当てはまらない。スケールの軸はデバイス内性能:
- sqlite-vecはWarm層のみ対象（FR-MEM-21）、行数増大時は期間短縮・量子化前倒し（§9リスク）
- 各サービス接続層を分離し個別に無効化・フォールバック可能（FR-INT-06、公式MCP仕様変更対策）
- Composioスケールは非論点（Gmail送信のみ・極少量）

### 精度
これも既に設計・実装済み。効いている仕組み:
- **fail-closed許可表**（`scope.rs`）: 表にない操作は実行不能。誤操作の構造的排除
- **confidence + provenance**（全stateレコード必須）: 低確度を事実として生成物に混ぜない
- **L1/L2/L3 + 承認キュー**: 外部送信は必ず人間確認（invariant 4）
- **Dream Cycleの名寄せ**（FR-ST-10）: 複数チャネルの同一人物統合、確度反映
- 追加できる余地: サービス別のツール記述最適化、実接続後のeval（本書 WP-G）

---

## 2. アダプタ層の置き場所（アーキテクチャ決定）

`shogun-mcp` の純粋性（Linuxテスト可能）を壊さないため、実I/Oは分離する。**新規クレート
`crates/shogun-integrations`（effectful, macOS/ネットワーク依存）** を作り、
`shogun-mcp` の trait を実装する。`inline_source.rs` がLLMで採った
「継ぎ目はcore/純ロジック、実体はアダプタ」の分離をコネクタにも踏襲する。

```
crates/shogun-mcp/          # 純: scope表・gate・trait定義（変更最小）
crates/shogun-integrations/ # 新規・effectful: rmcp client, oauth+pkce, keychain, composio http
apps/desktop/src-tauri/     # 配線: Tauriコマンド、daemon起動時にregistry/scheduler接続
```

依存の向き: `shogun-integrations → shogun-mcp`（trait実装）。逆流させない。
`shogun-mcp` は引き続きネットワーク依存を持たない。

---

## 3. ワークパッケージ（実装担当モデル向け）

各WPは1PR粒度。共通指示: Rust・既存コードのスタイル準拠・英語コメント/コミット・
`cargo test`と`cargo clippy`が通ること・secretsはKeychainのみ（invariant 7）・
生データをログ/DB/ファイルに出さない（invariant 2/3）。

### WP-A: 実MCPクライアントトランスポート
**目的**: `IntegrationTransport::read_sync` の実体を作り、公式リモートMCPから読み取る。

- 新規 `crates/shogun-integrations`。依存: Rust MCP SDK（`rmcp`）, `reqwest`（`ReqwestTransport`に倣う）, `tokio`, `serde_json`
- `RemoteMcpTransport` を実装し `shogun_mcp::sync::IntegrationTransport` を満たす。
  各 `Service` → 公式MCPエンドポイント + 必要スコープのマッピングを持つ
- MCPの `tools/list` → `tools/call` で read_sync 相当のツールを呼び、結果を
  `FetchedItem`（external_id/title/body/ts_ms）へ正規化。**本文テキストのみ**（添付・生ペイロード不可）
- Wave 1の2サービス（Gmail, GCal）から実装。残りは同じtraitに追加
- **受け入れ**: モックMCPサーバー相手の結合テストで read_sync が `FetchedItem` を返す。
  `shogun-mcp` の純テストは無改変で通り続ける

### WP-B: OAuth 2.1 + PKCE → Keychain
**目的**: サービス別のユーザー直接認可とトークン永続化（FR-INT-02）。

- Authorization Code + PKCE、システムブラウザで認可（`tauri-plugin-opener`等）、
  ローカルループバックでコールバック受信
- アクセス/リフレッシュトークンは `security_framework::passwords` でKeychainへ
  （service名 `com.selectkk.shogun`, account はサービス別。`inline_source.rs`のBYOK前例に倣う）
- トークン失効検知 → `ConnEvent::TokenExpired` を `ConnectionRegistry` に流す（amber化）。
  リフレッシュ成功 → `Reauthed`
- 切断（FR-INT-07）: Keychainトークン削除 + 同期停止。イベント削除はユーザー選択（既定保持）
- **受け入れ**: トークンがKeychain以外（DB/ファイル/ログ）に現れないことのgrep検査
  （要件§6.9受け入れ基準）。失効→amber→再認証の遷移が `connection.rs` の状態機械と一致

### WP-C: 同期スケジューラ + daemon配線
**目的**: 15分ポーリング + オンデマンド取得を回し、取り込みをevent logへ（FR-INT-04, §6.9）。

- daemon（`shogun-core`）が `ConnectionRegistry` と `RemoteMcpTransport` を所有
- 接続済み・解放済みサービスに対し15分間隔で `shogun_mcp::sync::collect_sync` を実行
  （レート制限時は間隔延長）。フォーカス文脈連動のオンデマンド取得も同経路
- 返った `IngestItem` を daemon が content hash 計算の上 event log へ追記（source列でタグ）、
  `IntegrationSynced` をバスへemit。**DB I/Oはdaemon側**（invariant 1: データ重心はコア）
- 同期失敗（`SyncFailure::Transport`）は該当サービスのみamber化、他へ波及させない
- **受け入れ**: 接続済みサービスの同期でevent logに source タグ付きイベントが入る。
  1サービスの失敗が他サービスの同期を止めないテスト

### WP-D: Composio Gmail送信のHTTP実体
**目的**: 既存の同意ゲートの先で実際に送信する（§6.10, FR-C2-01/05）。

- `crates/shogun-integrations` に Composio APIクライアント（`reqwest`）。
  `COMPOSIO_API_KEY` はKeychain。接続済みGmailアカウント（Composio側connected account）が前提
- 呼び出し経路は既存を尊重: L3承認キューで confirm 済みの `ConfirmedSend`（route=ViaComposio）
  を受けて送信。`shogun_mcp::composio::prepare_send` が作る action/preview はそのまま使う
- 成功→ `SendResult::Sent`。失敗→ `on_composio_failure()` = `FailedDraftSaved`:
  **黙って"送信済み"にしない**。Gmailドラフト保存にフォールバックしユーザー通知（FR-C2-05）
- トレーサビリティ: 送信箇所で `TraceRoute::ViaComposio` + `third_party=true` を記録（既存 sink）
- **受け入れ**: 送信成功/失敗の両分岐テスト（Composioはモック）。失敗時にドラフト保存＋
  トレースが third_party で残る。draft-stop ON では送信経路に到達しない（既存ゲートで保証済）

### WP-E: 接続管理のTauriコマンド + 設定UI
**目的**: ユーザーがサービスを接続/切断/状態確認できる（§6.9, FR-INT-06/07）。

- Tauriコマンド: `connect(service)` / `disconnect(service, delete_events)` /
  `connection_status()`（amber・最終同期時刻の鮮度を返す）
- 設定画面: 6サービスの接続状態、未解放Waveは「Coming soon」表示（FR-INT-03）、
  amberは再認証導線。UIは既存 desktop フロントの規約に従う（新デザインシステムを作らない）
- **受け入れ**: Wave 1でGmail接続→同期→amber化→再認証→切断の一連がUIから通る（手動確認可）

### WP-F: 確定済み書き込み/送信のMCP実行経路
**目的**: L2/L3で確認された第1層の書き込み（Gmailドラフト、カレンダー作成、Slack投稿等）を
実際にMCP経由で実行する。

- 承認キューで confirm 済みのアクション → `RemoteMcpTransport` の write/execute で実行
- 実行前に必ず `service_gate::authorize_op` を通す（二重ゲート）。
  `requires_traceability` が true の操作はトレース記録
- Wave順に対応（Wave1: Gmailドラフト/ラベル, カレンダー作成/更新）
- **受け入れ**: 許可表にない操作の実行が拒否されるテスト。カレンダー作成がL3確認を
  経てのみ実行されるテスト

### WP-G: 実接続eval
**目的**: 実MCP接続後の回帰検知（精度）。

- テスト用アカウントに対しゴールデンタスク（「未読メール同期に送信者が含まれる」
  「明日15時の予定作成がL3確認待ちになる」等）を実行し成功率を計測
- 通常CIはモックのみ。実接続evalは手動/週次に隔離（テスト用秘密はCI secrets）
- **受け入れ**: 成功率レポート出力、しきい値割れでfail

---

## 4. 実装順序と依存

```
WP-A 実MCPトランスポート   ┐
WP-B OAuth+Keychain       ┼─→ WP-C 同期スケジューラ ─→ WP-E 接続UI
                          ┘                          ─→ WP-F 書き込み実行
WP-D Composio送信HTTP（WP-B のKeychainパターン再利用。他WPと並行可）
WP-G eval（WP-C/WP-F後）
```

推奨: WP-A + WP-B を先に固め、WP-C で繋いでWave 1を実接続で動かす（ここが最初のマイルストン）。
Composio送信（WP-D）はGmail読み取りが動いた後でよい。

## 5. 人間側の事前準備（実装モデルにはできないこと）

1. **OPEN-03の解決**: Slack公式リモートMCPの提供状況・WS管理者承認フローの実態確認
   （Wave 2着手前, 要件§9.1）。不可なら FR-INT-30 のクリップボードフォールバックが恒常運用
2. 各サービスのOAuthアプリ登録とリダイレクトURI設定（Gmail restricted scopeは
   Google審査が絡む可能性 — 公式MCP経由なら軽減されるか要確認）
3. 公式リモートMCPの**実在確認**: Gmail/GCal/Slack/Notion/GitHub/Linearの公式MCPエンドポイントURL・
   対応ツール・必要スコープの調査（WP-A実装の前提。存在しないサービスはWaveから外すか代替検討）
4. Composioアカウント + `COMPOSIO_API_KEY`、テスト用Gmailのconnected account作成（WP-D）
5. テスト用アカウント一式（WP-G）

## 6. 前回の誤りの記録（教訓）

当初 `apps/api` に TS/Hono サーバーを立ててComposioで全コネクタを賄う計画を作ったが、
実プロダクトはRust/Tauriのローカルファーストで、コネクタは公式MCP直結（第1層）が主・
Composioは第2層Gmail送信のみ、という設計だった。前提を実コード・要件定義で検証せず
スキャフォールド状態から推測したのが原因。**設計前に必ず既存実装と要件定義を読むこと。**
