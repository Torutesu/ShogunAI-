# MCP 連携 — ユーザー向けガイド & 接続フロー定義

> Issue #59 のアウトプット第2弾。ユーザーが「どのサービスを繋ぐと何ができるか」「どう接続・解除するか」を理解するためのガイドと、その体験を実装するためのフロー定義。
> UI 文言の規約：**画面上の文言は英語**（絵文字は ⚔ のみ。`docs/wireframes/shogun-onboarding.html` の規約に従う）。本書の日本語はあくまで説明。
> 前提：**Gmail は読み取り・ドラフト・送信のすべてが Composio（第三者サービス）経由**（2026-07 決定、`00-overview.md` 前提 2）。本書のフローはこの決定後の設計。

## 1. どのサービスを繋ぐと、何ができるか（Wave 1）

| サービス | できること | 経路とアクセス範囲 | 繋がないとどうなるか |
|---|---|---|---|
| **Google Calendar** | 予定の把握、会議の準備、空き時間の確認 | **直結**（公式サーバーと端末の直接通信）。読み取り中心（イベント読取・freebusy。書き込みは承認必須） | 「明日の予定は？」に答えられない |
| **Gmail** | メールスレッドを読んで文脈を理解、返信の下書き作成。送信は承認（L3）必須 | **Composio（第三者サービス）経由 — 読み取りを含むすべて**。接続には 3開示の明示同意が必須で、**同意するまで一切同期されない**。送信は draft-stop（既定 ON = 下書きで停止）+ 承認必須。Google のパスワードやトークンを SHOGUN が持つことはない（認可は Composio 側で管理） | メールの文脈を踏まえた回答ができない |
| **Google Drive** | 資料・ドキュメントの参照、会議関連ファイルの発見 | **直結**。読み取り（`drive.readonly`） | 資料を踏まえた回答ができない |
| Slack（Wave 2） | — | — | 「Not available yet」表示 |
| Notion / GitHub / Linear（Wave 3） | — | — | 同上 |

**推奨の繋ぎ順：まず Calendar、次に Gmail。**Calendar は直結かつ接続直後に「明日の予定は？」で即座に価値が体験できる。Gmail が加わると「会議準備」のような組み合わせ体験が生まれるが、**第三者経由の同意という一段重い判断を伴う**ので、Calendar で価値を体験した後に自分のペースで判断できる並びにする。Drive は3番手（単体での即効性が薄いため）。

## 2. 初回オンボーディングでの接続フロー

既存の初回セットアップ（`shogun-onboarding.html`：Welcome → Accessibility → Bring your own key → **Connect your work** → You're set）の4番目のステップが MCP 接続にあたる。ステップ自体は増やさないが、**Gmail の Connect だけは 3開示同意シートを1枚挟む**（下記 3'）。ここをスキップして接続だけ通す実装は不可 — 同意なしの同期は存在しない。

### ステップ仕様：Connect your work

1. **なぜ繋ぐのか、そしてデータがどこを通るのかを正直に伝える**
   - lead 文（改訂）：*"Give SHOGUN read access to the tools you already use. Calendar and Drive connect directly to Google — nothing in between. Mail works through Composio, a third-party service, and always asks for your explicit consent first."*
   - 旧 lead 文の "It connects directly to each service — nothing is routed through anyone else." は **Gmail 全面 Composio 化後は虚偽になるため使用禁止**。「直結」を語れるのは Calendar / Drive のみ。信頼の核は「直結」の一律主張ではなく、**経路を隠さず正確に言うこと**に置く
2. **Wave 1 の3サービスを行で提示、推奨2つを上に**
   - 並び順：Calendar → Mail → Drive →（区切り）→ Slack 等は "Not available yet"（押せない、opacity 落とし）
   - Mail 行には常時 *"via Composio, a third-party service"* のサブラベルを付ける（接続前から。押した後に初めて知らせるのは不誠実）
   - ※現行ワイヤーは Mail / Calendar / Slack の3行で **Drive が無い**。Drive 行の追加が必要（後続 UI Issue に含める）
3. **Connect ボタンの挙動はサービスで異なる（正直に分ける）**
   - **Calendar / Drive**：ブラウザで各サービスの OAuth 同意画面 → 許可 → ローカルアプリに戻る（ループバック、`oauth_flow.rs`）。トークンは Keychain に保存され、接続状態がハブ層（`connection.rs` FSM）に記録される
   - **Mail（Gmail）**：Google の OAuth 画面は開かない。代わりに **3開示同意シート（下記 3'）** を表示 → 3つすべて承諾して初めて Composio 接続が確立し、同期が始まる。承諾しなければ何も起きない（外部通信ゼロ）
4. **スキップ可能**
   - 接続ゼロでも先に進める。あとから Settings → Connections でいつでも接続できることを1行添える
   - 例：*"You can connect these anytime in Settings."*

### 3'. Gmail の 3開示同意シート（FR-C2-02、`composio.rs` の `Disclosures` に対応）

同意シートには次の3開示を提示し、**3つすべての明示的な承諾**（まとめて1チェックにしない）を必須とする：

| 開示 | 文言案（英語 UI） |
|---|---|
| ① 第三者経由 | *"Your mail — including everything SHOGUN reads, not just what it sends — passes through Composio, a third-party service."* |
| ② 経由するデータ種別 | *"What passes through: your email threads and messages SHOGUN reads for context, drafts it writes, and emails you approve for sending."* |
| ③ 取り消し可能 | *"You can revoke this anytime in Settings. Syncing stops immediately; nothing new leaves your Mac."* |

補足表示（開示とは別に必ず添える）：

- *"Sending is off by default: SHOGUN stops at drafts (draft-stop). Even if you turn that off later, every send still requires your approval."*（draft-stop 既定 ON、OFF にできるのは同意後のみ。かつ送信は常に L3）
- *"Every exchange is logged for you to review — the log records what happened, never the content."*（読み取りを含む全 egress のトレーサビリティ）

**このステップでやらないこと**：draft-stop の解除（送信解放）はオンボーディングに入れない。初回は「読み取りで価値を体験する」ことに集中し、送信の解放は実際に送信したくなった時点（Approvals 文脈）で案内する。

## 3. 設定画面：Connections パネルの項目定義

場所：Settings ウィンドウ → **CONNECTIONS** セクション（骨格実装済み：`apps/desktop` の Connections パネル）。

### 各サービス行に表示する項目

| 項目 | 内容 | データ源 |
|---|---|---|
| サービス名 + アイコン | Mail / Calendar / Drive … | `Service` enum |
| できることの短い説明 | 例：*"Read threads and draft replies."* | 静的文言（§1 の表と一致させる） |
| アクセス範囲 | **Read** / **Read & Draft** / **Read & Write** のバッジ | `scope.rs` の権限表から導出 |
| **経路** | 第1層は表示なし（直結が既定）。**Mail 行には常設サブラベル** *"via Composio, a third-party service"* | 静的（サービス → 経路の対応） |
| 接続状態バッジ | **Connected**（緑）/ **Reconnect**（amber）/ **Not connected** / **Coming soon**（未リリース Wave） | `connection.rs` FSM + `service_gate` |
| 接続日時・最終同期 | 例：*"Connected Jul 12 · Last sync 5 min ago"* | ハブ層の接続記録 + `ConnectorRuntime` |
| アクション | **Connect** / **Disconnect** / **Reconnect**（amber 時） | `connect_service` / `disconnect_service` |

### 状態ごとの見せ方

- **amber は「壊れた」ではなく「再認可すれば戻る」**：赤いエラーにしない。*"Session expired — reconnect to resume sync"* + Reconnect ボタン
- **Disconnect の確認**：ワンクリックで即切断はしない。*"Disconnect Calendar? SHOGUN will stop syncing and forget its access."* の軽い確認を挟む。切断でトークンは Keychain から削除
- **Coming soon**：押せないが行としては見せる（先の広がりを示す）

### Mail（Gmail）行の扱い — Composio と別枠にしない、ただし見た目で必ず区別する

2026-07 決定により **Gmail の接続そのものが Composio 接続**なので、旧設計の「Connections の Gmail 行と Composio 設定を別枠にする」構成は成立しない。Mail は他サービスと同じ Connections リストに1行として置き、そのうえで：

- 行に常設サブラベル *"via Composio, a third-party service"*（第1層の行と視覚的に区別。バッジ色 or 枠で差を付ける）
- Connect は §2-3' の 3開示同意シートへ遷移（OAuth ではない）
- 行の詳細（展開 or 詳細画面）に：**同意状態** / **draft-stop トグル**（既定 ON。説明 *"Stop at drafts — SHOGUN never sends, only writes drafts for you."*）/ 同意の取り消しボタン
- Disconnect（= 同意取り消し）の確認文言は経路まで含めて正確に：*"Disconnect Mail? SHOGUN will stop syncing through Composio and revoke its access."* 切断で Composio API キーは Keychain から削除
- トレーサビリティ画面では Mail 由来の記録すべて（読み取り含む）に「第三者経由」バッジを付ける（CLAUDE.md 連携実装ルール）

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

原則：取得中コピーは**読み取りにだけ**出す。送信・書き込みは Approvals パネルの明示フローが主役なので、さりげないコピーで流さない。経路の開示はマイクロコピーの仕事ではなく Connections / トレーサビリティ画面の仕事（会話中に毎回 "via Composio" を出すとノイズになる。ただし記録には必ず残る）。

## 5. 接続解除・トラブル時のユーザー向け説明（ガイド原稿）

**Q. 接続を解除するとデータはどうなる？**
Calendar / Drive は解除するとトークンが端末の Keychain から削除され、以後の同期は止まる。Mail は解除すると Composio への同意が取り消され、同期が即座に止まり、Composio API キーが Keychain から削除される。いずれも、すでに取り込まれた過去のデータ（event_log 内）は端末に残る。完全に消したい場合はデータ削除（別機能）を使う。

**Q. 「Reconnect」と出た**
サービス側のセッションが切れただけ。Reconnect を押して再認可すれば元通り。データは失われない。

**Q. 送信までしてほしくない**
送信が勝手に起きることはない。Mail を接続しても **draft-stop が既定で ON** — SHOGUN は下書き作成までで必ず止まる。draft-stop を OFF にできるのは同意後のあなた自身の操作だけで、OFF にした後も**すべての送信はあなたの承認（Approvals）を通る**。承認を通らない送信経路は存在しない。

**Q. 自分のデータはどこを通る？**
サービスによって異なるので、正確に言う：

- **Calendar / Drive の読み取り・書き込みは、各サービスの公式サーバーと端末の直接通信のみ。**間に誰も入らない
- **Mail（Gmail）は読み取りも送信も Composio（第三者サービス）を経由する。**ただし、あなたが 3開示に明示同意しない限り**一通も同期されない**（同意前の外部通信はゼロ）。同意はいつでも取り消せて、取り消せば同期は即座に止まる
- どちらの経路でも、外部とのやり取りはすべて**本文なしの記録（トレーサビリティ）**で後から確認できる。Mail は読み取りを含む全やり取りが「第三者経由」の印付きで記録される

**Q. Mail に同意しないと SHOGUN は使えない？**
使える。Mail はオプトインであり、同意しなければ Gmail が同期されないだけ。Calendar / Drive・キャプチャ・メモリ・検索など他のすべては通常どおり動く。

## 6. Figma ワイヤーに起こす画面（骨格）

1. **オンボーディング「Connect your work」改訂版** — §2 の仕様（Drive 行追加、推奨順、Mail 行の "via Composio" サブラベル、スキップ文言、改訂 lead 文）
2. **Gmail 3開示同意シート** — §2-3' の3開示＋draft-stop/トレーサビリティ補足、個別承諾 UI
3. **Settings → Connections パネル** — §3 の項目全部入り（状態バッジ4種、Mail 行の視覚的区別と詳細＝同意状態/draft-stop/取り消し）
4. **マイクロコピー表示位置** — 会話 UI 内でのステータス行の位置とタイミング（§4）

## 7. この定義から出る後続 Issue（切り出しメモ）

- オンボーディング「Connect your work」ステップの実装更新（Drive 行・推奨順・改訂 lead 文・Mail 行の Composio サブラベル・スキップ文言）
- **Gmail 3開示同意シートの実装**（`composio.rs` の `Disclosures` / `grant_consent` に接続。同意前は同期ゼロをテストで保証）
- Connections パネルの項目拡充（アクセス範囲バッジ・最終同期時刻・Disconnect 確認・Mail 行の経路表示 + draft-stop トグル + 同意取り消し）
- 会話 UI のマイクロコピー実装（ツール呼び出しイベント → ステータス行）
- トレーサビリティ画面の「第三者経由」バッジ（Mail の読み取り記録を含む）
