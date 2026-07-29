# MCP 連携 — ユーザー向けガイド & 接続フロー定義

> Issue #59 のアウトプット第2弾。ユーザーが「どのサービスを繋ぐと何ができるか」「どう接続・解除するか」を理解するためのガイドと、その体験を実装するためのフロー定義。
> UI 文言の規約：**画面上の文言は英語**（絵文字は ⚔ のみ。`docs/wireframes/shogun-onboarding.html` の規約に従う）。本書の日本語はあくまで説明。

## 1. どのサービスを繋ぐと、何ができるか（Wave 1）

| サービス | できること | アクセス範囲 | 繋がないとどうなるか |
|---|---|---|---|
| **Gmail** | メールスレッドを読んで文脈を理解、返信の下書き作成。送信は Composio オプトイン + 承認必須 | 読み取り + 下書き（`gmail.readonly` / `gmail.compose`。**`gmail.send` は要求しない**） | メールの文脈を踏まえた回答ができない |
| **Google Calendar** | 予定の把握、会議の準備、空き時間の確認 | 読み取り中心（イベント読取・freebusy。書き込みは承認必須） | 「明日の予定は？」に答えられない |
| **Google Drive** | 資料・ドキュメントの参照、会議関連ファイルの発見 | 読み取り（`drive.readonly`） | 資料を踏まえた回答ができない |
| Slack（Wave 2） | — | — | 「Not available yet」表示 |
| Notion / GitHub / Linear（Wave 3） | — | — | 同上 |

**推奨の繋ぎ順：まず Calendar + Gmail の2つ。**Calendar は接続直後に「明日の予定は？」で即座に価値が体験でき、Gmail が加わると「会議準備」のような組み合わせ体験が生まれる。Drive は3番手（単体での即効性が薄いため）。

## 2. 初回オンボーディングでの接続フロー

既存の初回セットアップ（`shogun-onboarding.html`：Welcome → Accessibility → Bring your own key → **Connect your work** → You're set）の4番目のステップが MCP 接続にあたる。**新しい画面は増やさない。**このステップを以下の仕様に合わせる。

### ステップ仕様：Connect your work

1. **なぜ繋ぐのかを1文で伝える**
   - 現行 lead 文を維持：*"Give SHOGUN read access to the tools you already use. It connects directly to each service — nothing is routed through anyone else."*
   - 「直結・誰も経由しない」（第1層の設計、FR-INT-01）が信頼の核なので必ず残す
2. **Wave 1 の3サービスを行で提示、推奨2つを上に**
   - 並び順：Calendar → Mail → Drive →（区切り）→ Slack 等は "Not available yet"（押せない、opacity 落とし）
   - ※現行ワイヤーは Mail / Calendar / Slack の3行で **Drive が無い**。Drive 行の追加が必要（後続 UI Issue に含める）
3. **Connect ボタン → OAuth → 戻ってきたら行が Connected に変わる**
   - ブラウザで各サービスの OAuth 同意画面 → 許可 → ローカルアプリに戻る（ループバック、`oauth_flow.rs`）
   - トークンは Keychain に保存され、接続状態がハブ層（`connection.rs` FSM）に記録される
4. **スキップ可能**
   - 接続ゼロでも先に進める。あとから Settings → Connections でいつでも接続できることを1行添える
   - 例：*"You can connect these anytime in Settings."*

**このステップでやらないこと**：Composio（Gmail 送信）のオプトインはここに入れない。初回は「読み取りで価値を体験する」ことに集中し、送信の同意は実際に送信したくなった時点（Approvals 文脈）で求める。

## 3. 設定画面：Connections パネルの項目定義

場所：Settings ウィンドウ → **CONNECTIONS** セクション（骨格実装済み：`apps/desktop` の Connections パネル）。

### 各サービス行に表示する項目

| 項目 | 内容 | データ源 |
|---|---|---|
| サービス名 + アイコン | Mail / Calendar / Drive … | `Service` enum |
| できることの短い説明 | 例：*"Read threads and draft replies."* | 静的文言（§1 の表と一致させる） |
| アクセス範囲 | **Read** / **Read & Draft** / **Read & Write** のバッジ | `scope.rs` の権限表から導出 |
| 接続状態バッジ | **Connected**（緑）/ **Reconnect**（amber）/ **Not connected** / **Coming soon**（未リリース Wave） | `connection.rs` FSM + `service_gate` |
| 接続日時・最終同期 | 例：*"Connected Jul 12 · Last sync 5 min ago"* | ハブ層の接続記録 + `ConnectorRuntime` |
| アクション | **Connect** / **Disconnect** / **Reconnect**（amber 時） | `connect_service` / `disconnect_service` |

### 状態ごとの見せ方

- **amber は「壊れた」ではなく「再認可すれば戻る」**：赤いエラーにしない。*"Session expired — reconnect to resume sync"* + Reconnect ボタン
- **Disconnect の確認**：ワンクリックで即切断はしない。*"Disconnect Calendar? SHOGUN will stop syncing and forget its access."* の軽い確認を挟む。切断でトークンは Keychain から削除
- **Coming soon**：押せないが行としては見せる（先の広がりを示す）

### Composio（Gmail 送信）の扱い

- Connections の Gmail 行とは**別枠**で表示する（第1層の接続と第三者経由のオプトインを混ぜない — `01-architecture.md` §4）
- 表示項目：オプトイン状態 / draft-stop トグル / *"Sending uses Composio, a third-party service"* の明示
- 未同意の間は Gmail 送信系の機能が「同意フローに誘導」される

## 4. 会話中のマイクロコピー（MCP 利用の可視化）

モデルがツールを呼んでいる間、何をしているかを短く見せる。**サービス名を主語にする**（「AI が考え中」ではなく「どこを見ているか」）。

| 場面 | 文言案（英語 UI） |
|---|---|
| Calendar 読み取り | *Checking your calendar…* |
| Gmail 読み取り | *Reading recent threads…* |
| Drive 読み取り | *Looking through your files…* |
| 複数同時 | *Gathering context from Calendar and Mail…* |
| 未接続のサービスが必要だった | *This would work better with Calendar connected — connect it in Settings.* |
| amber で取得失敗 | *Calendar needs to be reconnected. Open Settings → Connections.* |
| L3 承認待ち | *Waiting for your approval to send.* |

原則：取得中コピーは**読み取りにだけ**出す。送信・書き込みは Approvals パネルの明示フローが主役なので、さりげないコピーで流さない。

## 5. 接続解除・トラブル時のユーザー向け説明（ガイド原稿）

**Q. 接続を解除するとデータはどうなる？**
解除するとトークンは端末の Keychain から削除され、以後の同期は止まる。すでに取り込まれた過去のデータ（event_log 内）は端末に残る。完全に消したい場合はデータ削除（別機能）を使う。

**Q. 「Reconnect」と出た**
サービス側のセッションが切れただけ。Reconnect を押して再認可すれば元通り。データは失われない。

**Q. 送信までしてほしくない**
Gmail 送信はデフォルトで無効（Composio オプトイン必須）。オプトインしても、すべての送信はあなたの承認（Approvals）を通り、draft-stop を有効にすれば「下書き作成まで」で必ず止まる。

**Q. 自分のデータはどこを通る？**
読み取りは各サービスの公式サーバーと端末の直接通信のみ。唯一の例外は Gmail 送信（Composio 経由）で、これは明示的な同意が無い限り動かない。すべての外部やり取りは本文なしの記録（トレーサビリティ）で後から確認できる。

## 6. Figma ワイヤーに起こす画面（骨格）

1. **オンボーディング「Connect your work」改訂版** — §2 の仕様（Drive 行追加、推奨順、スキップ文言）
2. **Settings → Connections パネル** — §3 の項目全部入り（状態バッジ4種、Composio 別枠含む）
3. **マイクロコピー表示位置** — 会話 UI 内でのステータス行の位置とタイミング（§4）

## 7. この定義から出る後続 Issue（切り出しメモ）

- オンボーディング「Connect your work」ステップの実装更新（Drive 行・推奨順・スキップ文言）
- Connections パネルの項目拡充（アクセス範囲バッジ・最終同期時刻・Disconnect 確認・Composio 別枠）
- 会話 UI のマイクロコピー実装（ツール呼び出しイベント → ステータス行）
