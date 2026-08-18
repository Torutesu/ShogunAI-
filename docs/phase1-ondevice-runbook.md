# Phase 1 実機テスト runbook（現行フルスタック版）

対象読者: ノッチ搭載 MacBook Pro 上で、Linux で完成させた純ロジック層を実機で動かして確認する人。
上位文書: `docs/phase1-implementation-plan.md`（計画）/ `docs/requirements-v1.0.md`（正本）。
前身: `docs/phase0-on-device-runbook.md`（Phase 0 の4つの問い）、`docs/ondevice-ax-capture-runbook.md`（キャプチャ単体）。本書はそれ以降に積んだ全スタックを一枚にまとめ直したもの。

追補: Visual recall（画面OCR＋暗号化JPEGの有限保持）と会議オーバーレイ（3モード／live translate／クリックスルー）の差分確認は `docs/ondevice-runbook-visual-recall-and-meeting.md`。**画面収録・マイクという新しい TCC 権限**が必要なので、それらを触る場合は先にそちらの §1 を読むこと。

---

## 0. 前提と現状

- **純ロジックは Linux で全て green**（`cargo test --workspace --exclude shogun-desktop-spike`）。実機で「コンパイルが通るか」ではなく「動いて観測できるか」を見る。
- macOS シェル `apps/desktop/src-tauri`（`shogun-desktop-spike`）は **使い捨てスパイク**。描画は不安定（前回セッションの中断原因）。**製品shell（M1 WP1.1）で差し替える前提**なので、パネルの見た目を作り込まない。
- 実機で確かめたい本質は「**キャプチャ→メモリ→state推定→Fusion→アクション→実行**」がユーザーの実データで通ること。これは**パネル描画に依存しない self-test**（⌘⇧J）で確認できる。

### 実機で観測する対象（Linux で検証済みの実体）

| 領域 | 実体 | 確認手段 |
|---|---|---|
| キャプチャ | `capture_source.rs` → `Db::ingest_capture` | ログ `[capture]` + DB行 |
| 抽出 | `extract::extract`（commitment/open_loop） | `shogun commitments/open-loops` |
| Fusion | `Db::context_actions` | ⌘⇧J self-test ログ |
| 実行 | `ExecutionEngine`（L1/L2） | self-test の `submitted → disposition` |
| Memory API | REST 7464 / CLI / MCP | `shogun` コマンド |

---

## 1. セットアップ

| 項目 | 要件 |
|---|---|
| マシン | ノッチ搭載 MacBook Pro / Apple Silicon / macOS 14+ |
| ツール | Xcode CLT・rustup(stable)・pnpm・`cargo install tauri-cli --version '^2'` |
| 権限 | システム設定 → プライバシーとセキュリティ → **アクセシビリティ** に、ビルドした実行ファイル（またはターミナル）を追加 |

> ⚠️ 前回ハマった運用上の注意（必ず守る）
> - **`#` で始まる行をターミナルに貼らない**（zsh/cargo がコメントを引数と誤認して壊れる）。手順のコメントは頭の中で読む。
> - **grep やログ確認は別タブで**。アプリはフォアグラウンドを専有するので、同じタブに打ち込むと詰まる。
> - **`git checkout -- Cargo.lock` してから pull**（実機ビルドで Cargo.lock が動いて衝突するため）。
> - ログは `2>/tmp/shogun.log` に逃がす。`[capture]`/`[selftest]`/`[spike]` だけ見れば十分。

---

## 2. ビルドと起動

**必ず `pnpm tauri dev` で起動する**（`cargo run` はビルド時に埋め込んだ古いフロントを表示するため、UI 変更が反映されない）。`tauri dev` は live devUrl（localhost:1420）を読むので、フロントは即反映・ホットリロードされる（Rust 変更のみ再起動が必要）。

```
git checkout -- Cargo.lock
git pull origin claude/shogunai-requirements-prep-nm2tf4
cd apps/desktop            # tauri スクリプトはここにある（リポジトリ直下ではない）
pnpm install               # 初回のみ
pnpm tauri dev 2>/tmp/shogun.log
```

起動ログで確認する行（別タブで `tail -f /tmp/shogun.log`）:

- `[shell] notch panel installed (all-spaces, over menu bar, hover-reveal)` — NSPanel 化成功（これが出ればノッチ表示）
- `[spike] memory DB: …/com.syogun.shogunai/memory.db` — DB パス（後で CLI から同じ DB を叩く）
- `[spike] ⌘⇧J registered` / `[spike] ⌃⌥G registered` — ショートカット登録成功
- `[shell] panel install failed: …` が出たら NSPanel 化に失敗（プレーンウィンドウにフォールバック）

### パネル描画を切り離して確認したいとき

```
SHOGUN_NO_NOTCH=1 pnpm tauri dev 2>/tmp/shogun.log
```

NSPanel swap をスキップして**通常ウィンドウ**で起動する。描画が壊れてもコア（capture/memory/fusion/実行）は ⌘⇧J で検証できる。

---

## 3. コアの実機確認（パネル非依存）— ⌘⇧J self-test

1. 実データを溜める: 普段使いのアプリ（メール・エディタ・ブラウザ）を数分触る。「金曜までに送ります」「legal の返信待ち」のような **約束・未処理の文** を含む画面を開く。
2. `[capture] new …` がログに出るのを確認（`[capture] candidates: …` が出れば抽出も発火）。
3. **⌘⇧J を押す**。self-test が走り、次が出れば端から端まで通っている:

```
[selftest] N context action(s) for the current screen:
[selftest]   [0] L1 Local(SaveDraft { target: "reply" }) — …
[selftest] submitted top action → AutoRan
```

- `N=0`（`no gated actions yet`）→ 約束/ループを含む画面をもっとキャプチャしてから再度 ⌘⇧J。
- **空パネルにはならない**（FR-CF-04）: state が無くても `Save a note / Search memory / Extract tasks` が出るはず。

---

## 4. メモリ API の実機確認（別タブ）

REST は `127.0.0.1:7464`。CLI から同じ DB を読む（トークンは起動時発行、無ければ `SHOGUN_API_TOKEN=dev` を付けて `shogun-api` を別途起動）。

```
cargo run -p shogun-core --features daemon-server --bin shogun-api    # 別タブ・別プロセスで
curl -s 127.0.0.1:7464/v1/status
curl -s 127.0.0.1:7464/v1/metrics                                     # SLO スナップショット（未計測は measured:false）
curl -s -H "Authorization: Bearer dev" 127.0.0.1:7464/v1/state/commitments
curl -s -H "Authorization: Bearer dev" "127.0.0.1:7464/v1/memory/search?q=roadmap"
```

CLI も対称:

```
cargo run -p shogun-cli -- --token dev commitments list
cargo run -p shogun-cli -- --token dev metrics
```

確認ポイント:
- 抽出された commitment / open_loop が **confidence ≤ 0.4** で返る（事実として断定しない）。
- `/v1/actions/execute` に send 系を投げると **202 pending**（L3 は UI 承認まで実行されない）。

---

## 5. ノッチパネルの実機確認（製品 UI）

`pnpm tauri dev` で起動（NSPanel はデフォルト有効）。ノッチ直下に細い tongue が出るので、**そこにマウスを重ねる**と:

- **Idle**（折り畳み）: tongue のみ。パネルは透明・クリックスルー（背後のアプリを操作できる）。
- **Hover**（ノッチにホバー）: peek カード（`reading {App}` ＋ due/waiting カウント）がスッと降りてくる。
- **Expanded**（peek をクリック）: フルパネル（チャット・state リスト・composer・設定）。
- ヘッダーの ◇/◆ で **ピン留め**（look-away でも閉じない）、⚙ で **設定**（外観 Dark/Light/Auto・挙動・キー状態）。
- **どのスペース/画面に移動してもバックグラウンドに残る**（canJoinAllSpaces＋Status level）。全画面アプリの上にも出る。

確認ポイント:
- ノッチにホバー → peek が降りる（`[spike] cmd painted state=hover` がログに出る）。
- Expanded で `Ask SHOGUN…` に入力 → 応答（BYOK 未設定なら echo、設定済みなら実応答）。
- 別アプリで **⌃⌥G** → カーソル位置にドラフト挿入。
- 別スペース/全画面に切り替え → パネルが残っていること。

描画が出ない/固まる場合は `SHOGUN_NO_NOTCH=1` で通常ウィンドウにフォールバックし、§3・§4 のコア確認（⌘⇧J self-test）で前に進む。原因切り分けは `docs/phase0-on-device-runbook.md` の wry/NSPanel 節。

---

## 6. 実機で埋める SLO 実測（NFR-SLO-00）

- `shogun metrics` / `/v1/metrics` は現状 **未計測（measured:false）** で返る。ランタイムが計測点を叩き始めたら p50/p95 が入る。
- Phase 0 実測（展開 p95 ≈ 18ms）との整合を SLO-01/02 で確認し、`docs/phase1-findings.md` に貼る。

---

## 6.5 インライン下書き（カーソル位置に生成）— ⌃⌥G

`compose_inline`（画面文脈＋記憶→BYOK生成→カーソル挿入）を実機で試す経路。純ロジックはLinuxテスト済み、AX読取/挿入とBYOK配線がここで初めて実機コンパイルされる（`apps/desktop/src-tauri/src/inline_source.rs`、`shogun-core` は `db,net` フィーチャ）。

### まず鍵なしで AX 経路を確認（エコー）

BYOK鍵がKeychainに無い状態では**エコーmock**が動くので、「読取→プロンプト組み立て→カーソル挿入」の配線だけを先に検証できる:

1. メールやエディタの**テキスト欄にカーソルを置く**（何か入力しておく）
2. **⌃⌥G** を押す
3. ログ（別タブ `tail -f /tmp/shogun.log`）:
```
[inline] no BYOK key in Keychain — using echo mock (AX path still runs)
[inline] inserted N chars at the cursor
```
→ カーソル位置に `draft: You are writing directly...`（プロンプトのエコー）が挿入されれば、**AX読取・挿入が実機で通った**証明。

### 実生成にする（BYOK鍵をKeychainへ）

鍵は**Keychainのみ**（不変条件7、環境変数・ファイル禁止）。ターミナルで登録:

```
security add-generic-password -s com.selectkk.shogun -a anthropic-byok -w
```
（`-w` の後、対話でAnthropic APIキーを貼る。履歴に残さないため引数で渡さない）

再度 ⌃⌥G を押すと:
```
[inline] BYOK key found — using the live Agent lane
[inline] inserted N chars at the cursor
```
→ カーソル位置に**実際に生成された文章**が挿入される。トレーサビリティ行（digestのみ）も1件記録される。

### 既知の未実装・オンデバイス調整点

- **トリガーは暫定 ⌃⌥G**。製品仕様の「Optionキー単体」は `flagsChanged` を見る CGEventTap が必要（既存hoverのtap基盤の延長）。まず ⌃⌥G で疎通確認。
- **カーソル位置**: v1はフィールド全文を「カーソル前」として扱い末尾に挿入する。`kAXSelectedTextRangeAttribute` での正確なキャレット分割は実機調整項目。
- **モデル**: 下書きは低レイテンシ優先で `claude-sonnet-5` 既定（Settingsで変更可の想定）。
- AXシンボル（`AXUIElementCreateSystemWide` / `kAXSelectedTextAttribute` / `AXUIElementSetAttributeValue`）が `accessibility-sys 0.2` で名前差異があればビルドエラーを貼って調整。

## 6.6 サブスク委譲（APIキー無しで Agent lane を動かす）— Issue #110

ユーザーが既にログイン済みのベンダー公式CLIに Agent lane を委譲する経路。ロジック・分類・トレーサビリティは Linux テスト済み（`crates/shogun-core/src/llm/subscription.rs`、24ケース）。**実機で初めて検証されるのは「各CLIの非対話呼び出し規約」**——ここが唯一の未確認点なので、まずここを潰す。

### 前提

委譲先のいずれかがインストール済み・ログイン済みであること。SHOGUN 側の設定は不要（鍵を持たないため）。

### Step 1: 呼び出し規約の素振り（SHOGUN を起動する前に）

`subscription.rs` の `Delegate::completion_args()` が定義する呼び出しを、そのままターミナルで再現する。**プロンプトは stdin**（argv に置くと同一マシンの全プロセスから `ps` で読めるため。FR-AG-06d）:

```
echo 'Reply with the single word: ok' | claude -p --output-format text
echo 'Reply with the single word: ok' | codex exec -
echo 'Reply with the single word: ok' | gemini
```

- **exit 0 + stdout に応答**が出れば規約は正しい
- 対話プロンプトで止まる／usage が出る場合は、その CLI の headless モードのフラグを確認し **`completion_args()` を直す**。これが「ベンダーCLIの変更を吸収する唯一の場所」として設計されている
- ログインしていない場合の stderr 文言を控えておく（`LOGIN_MARKERS` に一致するか。しなければマーカーを追加する）
- 上限到達時の文言も同様（`RATE_LIMIT_MARKERS`）。**上限到達を「未ログイン」と誤分類すると、壊れていないアカウントの再認証にユーザーを送ることになる**ので、ここは実文言で確認する

### Step 2: 検出と接続テスト（アプリ内）

1. Settings を開く → **Your subscription** セクションに検出結果が出る
2. ログ:
```
[inline] subscription delegates: claude-code=installed codex=not_installed gemini-cli=not_installed
```
3. **Test connection** を押す → 実際に1回完了を走らせて `ready` / `needs_login` / `rate_limited` を判定する
```
[inline] verified claude-code → ready
```
> 検出（`--version`）はネットワークも枠も使わないが、**Test connection はユーザーの枠を1〜2トークン消費する**。だから自動では走らせない。

### Step 3: 同意して使う

1. 開示3項目を読み **I understand — use my plan** を押す（`subscription_consent = true`）
2. **Use this** で委譲先を選択
3. ⌃⌥G / チャットを実行:
```
[inline] live Agent lane — delegating to claude-code on the user's own plan
```
同意なしで委譲先が選ばれている場合は生成せず、こう出る（FR-AG-06b）:
```
[inline] claude-code selected but the disclosure is unaccepted — not drafting
```

### Step 4: 確認すべき事実

- **APIキーを一度も入力せずに**生成が通ること（これが Issue #110 の完了条件そのもの）
- トレーサビリティ画面に `local_agent` 行が1件、**第三者バッジ無し**で出ること
- `~/.claude/.credentials.json` 等が**一切開かれていない**こと。実機で確認するなら:
```
sudo fs_usage -w -f filesys | grep -i credentials
```
→ SHOGUN プロセスからのアクセスがゼロであること（CI 側は `scripts/check-secret-exposure.py` が静的に担保）

### Step 5: SLO-03 の実測（**マージ前必須**）

プロセス spawn は数百ms〜秒かかる。SLO-03（アクション実行→初トークン 1s）を割る可能性が最も高いのがこの経路。

```
# 10回まわして wall-clock を取る
for i in $(seq 10); do /usr/bin/time -p sh -c "echo 'draft a one-line reply' | claude -p --output-format text > /dev/null" 2>&1 | grep real; done
```

p50/p95 を出して **PR本文に貼る**（CLAUDE.md「SLO関連の変更は計測結果をPR本文に貼る」）。1s を割れない場合は、常駐セッション化やストリーミング出力への変更が必要——**黙って SLO を下げない**。

### 既知の未実装

- 現状は**非ストリーミング**（完了を待って一括返却）。SLO-03 を満たすにはストリーミング出力の取り込みが要る。次の一手は `--output-format text` から **`--output-format stream-json`**（＝Agent SDK が使う、ドキュメント化された programmatic surface）への移行。フラグを当てにいく現状より契約が安定し、初トークンも取れる
- 毎回 spawn する。常駐セッション化は Step 5 の実測を見てから判断する
- 枠の使い切り時の BYOK 自動フォールバックは未実装（分類は出来ているので導線を足すだけ）

### 枠についての注意（実測時に誤解しないこと）

Claude 側は 2026-06-15 以降、**Agent SDK / `claude -p` の消費は対話利用（Claude Code / Claude）の上限とは別勘定**になり、プランに付く**月次クレジット**（Pro $20 / Max 5x $100 / Max 20x $200 相当、API標準レート）から引かれる。したがって:

- Step 5 で何度回しても**ユーザーの対話用 Claude Code は止まらない**（旧来の「5時間ウィンドウを焼く」挙動ではない）
- 一方で**月次クレジットは減る**。実測ループを回しすぎない
- 使い切り時の回復は**月替わり**。UI文言に「しばらく待って再試行」と書いてはならない（FR-AG-06e）

---

## 7. 詰まったら

| 症状 | 対処 |
|---|---|
| ⌘⇧J が効かない | 別アプリが専有。ログに `registered` が出ているか確認。JP入力の⌘⇧Spaceとは別（Jにした理由）|
| `[capture]` が出ない | アクセシビリティ権限未付与。システム設定で実行ファイルを追加し再起動 |
| ビルドが Cargo.lock で衝突 | `git checkout -- Cargo.lock` してから pull |
| パネルが描画されない | `SPIKE_NO_PANEL=1` でコアだけ先に確認。描画は M1 製品shell待ち |
| ログが多すぎる | `2>/tmp/shogun.log` に逃がし、`grep -E "capture|selftest" /tmp/shogun.log` |

---

*本 runbook は Linux セッションで作成。実機で観測した結果・SLO 実測値・詰まった点は `docs/phase1-findings.md` に追記して次セッションへ引き継ぐこと（計画書 §8.6）。*
