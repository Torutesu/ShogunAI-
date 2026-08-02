# 実機確認 runbook — Visual recall / 会議オーバーレイ（本ブランチ差分）

対象: `claude/verify-changes-device-54rm2z` の 8 コミット（`mikel/meeting-recap-transcript` からの差分）を、ノッチ搭載 Apple Silicon Mac で確認する人。

上位: `docs/phase1-ondevice-runbook.md`（フルスタック版。§1〜§2 のビルド前提・運用注意はそちらが正）。本書は **今回の差分だけ**を最短で確認するための追補。

対象差分:

| # | コミット | 実機で初めて動く実体 |
|---|---|---|
| 1 | `feat(desktop): meeting overlay modes, translate, and visual recall OCR` | 3モード切替・EN→JA live translate |
| 2 | `fix(integrations): open Keychain ACLs so dev rebuilds keep Always Allow` | `keychain_store.rs` ＋ `scripts/codesign-desktop-dev.sh` |
| 3 | `feat(desktop): ship visual recall OCR with search and settings surfacing` | CGWindow capture → Vision OCR → 検索 |
| 4 | `fix(desktop): meeting overlay click-through via pointer hit-testing` | `meeting_overlay_set_interactive` |
| 5 | `feat(memory): add screen_frames table with 72h retention` | V12 マイグレーション |
| 6 | `feat(desktop): store JPEG frames and expose visual recall APIs` | JPEG 保存・recall API |
| 7 | `fix(desktop): harden meeting overlay, keychain, and DB startup` | DB 破損時の隔離起動 |
| 8 | `fix(memory): correct yesterday window test expectations` | （Linux 済。実機確認不要） |

---

## 0. Linux 側で既に green のもの（実機で再確認しなくてよい）

実機の時間は「Mac でしか動かないもの」に使う。以下はこのブランチ上で検証済み:

```
cargo test --workspace --exclude shogun-desktop-spike   # 全 green（455 tests, 0 failed）
cd apps/desktop && pnpm install && npx tsc --noEmit     # 型 OK
```

さらに、実機でしか出ないランタイムエラーになりがちな配線も静的に照合済み:

- フロントの `invoke("…")` **60 個すべてが** `generate_handler!` に登録済み（未登録呼び出しゼロ）。
- フロントの `listen("…")` すべてにバックエンドの `emit` が対応（`meeting_live_translation` は `meeting_translate.rs:341`）。
- 新規 macOS モジュールの依存（`objc2-vision` / `core-graphics` / `foreign-types` / `image` / `core-foundation`）は `Cargo.toml` 宣言済み・`Cargo.lock` 収録済み。

→ **実機の失敗は「ビルドが通らない」より「権限」「Vision の実挙動」「描画」で出る**前提で臨む。

---

## 1. セットアップ差分（前回の runbook から増えたもの）

| 権限 | 何のために | 設定場所 |
|---|---|---|
| アクセシビリティ | 従来のキャプチャ（AX テキスト） | プライバシーとセキュリティ → アクセシビリティ |
| **画面収録（NEW）** | `CGWindowListCreateImage`（Visual recall OCR） | プライバシーとセキュリティ → **画面収録** |
| **マイク（NEW）** | 会議 ASR（オンデバイス whisper） | プライバシーとセキュリティ → マイク |

> ⚠️ **画面収録は Accessibility とは別の TCC カテゴリ**。Visual recall を ON にしても、ここを許可していないと OCR は無言で空振りする（真っ黒/空文字）。許可後は **アプリを再起動**する（macOS は TCC 変更をプロセス再起動まで反映しない）。

### Keychain の Always Allow を rebuild 越しに保たせる（差分 #2）

`tauri dev` のリビルドごとに Keychain ダイアログが出るのを止めるため、安定した署名 ID で署名する:

```
cd apps/desktop && pnpm tauri build --debug
cd ../.. && SHOGUN_SIGN_IDENTITY="Apple Development: 自分の名前 (TEAMID)" ./scripts/codesign-desktop-dev.sh
```

Xcode の Apple Development 証明書が無ければ ad-hoc（`SHOGUN_SIGN_IDENTITY="-"`、既定）でも動くが、Sonoma 以降は永続性が弱い。

確認: 署名後にアプリを 2 回リビルド → 起動して Select KK / BYOK を読む操作をする → **ダイアログが 1 回目だけ**なら成功。ログに以下が出れば旧サービス（`SHOGUN`）からの移行も動いている:

```
[keychain] migrated legacy <account> to com.selectkk.shogun
[keychain] normalized hex-encoded select-kk-batch to plain text
```

---

## 2. 起動

```
git checkout -- Cargo.lock
git pull origin claude/verify-changes-device-54rm2z
cd apps/desktop
pnpm install
pnpm tauri dev 2>/tmp/shogun.log
```

別タブで `tail -f /tmp/shogun.log`。今回の差分で見る起動行:

```
[visual_recall] screen OCR off (default)        ← 既定 OFF（オプトイン）であることの証明
[spike] memory DB: …/dev.shogun.spike/memory.db
[spike] connector runtime started (read-sync poller live)
```

`visual-recall-ocr` は desktop crate の **default feature**。OCR を外して切り分けたいときだけ `pnpm tauri dev -- --no-default-features`。

---

## 3. 差分 #7 — DB 起動ハードニング

意図: 壊れた/読めない memory DB でアプリが起動不能にならない（キャプチャデーモンを落とさない原則）。

```
# アプリを終了してから、DB をわざと壊す（バックアップを取ってから）
cp ~/Library/Application\ Support/dev.shogun.spike/memory.db /tmp/memory.db.bak
printf 'garbage' > ~/Library/Application\ Support/dev.shogun.spike/memory.db
```

再起動 → 期待するログ:

```
[spike] unreadable memory DB moved to …/memory.db.corrupt-<ts> — creating a fresh store
```

確認ポイント:
- **アプリが起動する**（パニックせず、空の新規 DB で立ち上がる）。
- 壊れたファイルは削除ではなく **隔離**されている（`.corrupt-*` が残る）。
- 鍵が読めないだけのケースでも同様に前に進む。

終わったら `cp /tmp/memory.db.bak ~/Library/…/memory.db` で戻す。

---

## 4. 差分 #3/#5/#6 — Visual recall（OCR ＋ JPEG 72h）

### 4.1 既定 OFF → ON

1. パネルを開く（**⌘⇧J**）→ 設定（⚙）→ **Visual recall** セグメント。既定は Off。
2. On にする。ログ: `[visual_recall] screen OCR enabled`。

### 4.2 OCR が実際に走るか

OCR は**常時**ではなくゲート付き（Screenpipe 方式）。狙って発火させる:

- **確実に発火する画面**: ターミナル（WezTerm / Alacritty / kitty / Warp / Hyper）、または Google Docs / Figma / Excalidraw / Miro / Canva（`pipeline.rs` の canvas パターン）。AX が薄い（本文 100 文字未満、会議中は 400 文字未満）画面も対象。
- 同一フォーカスへの OCR は **最短 10 秒間隔**（`MIN_OCR_INTERVAL_MS`）。連打しても増えない。
- ピクセル署名が前回と同一なら **skip**（`ocr_gate`）。**画面を少し変えてから**見る。

ログ `grep -E "screen_ocr|visual_recall" /tmp/shogun.log` と、設定画面の Visual recall ステータス（`events_24h` / `frames_count` / `frames_bytes` / 最古フレーム時刻）が増えることを確認。

### 4.3 保存されているのは JPEG だけ・DB の中だけ

```
sqlite3 ~/Library/Application\ Support/dev.shogun.spike/memory.db \
  "select count(*), min(created_at_ms), sum(length(bytes)), group_concat(distinct mime) from screen_frames;"
```

- `mime` が `image/jpeg` のみ（品質 65）。
- **ディスク上に画像ファイルが1つも生まれていない**こと（不変条件2）:
  ```
  find ~/Library/Application\ Support/dev.shogun.spike -type f \
    \( -name '*.png' -o -name '*.jpg' -o -name '*.jpeg' -o -name '*.wav' -o -name '*.caf' -o -name '*.m4a' \)
  ```
  → **何も出ないのが正**。会議を録っても音声ファイルは生まれない。
- DB は暗号化済みで開く。上記 `sqlite3` が失敗する場合はアプリ内ステータス（`get_visual_recall_status`）を正とする。

### 4.4 検索からの recall

Visual recall のフレームは **クエリが「画面/時間」を訊いている時だけ**引かれる（`query_wants_visual_recall`）。

- 引かれる例: `what was on my screen this morning` / `which window did I see yesterday` / `show the app I was looking at`
- 引かれない例: `roadmap`（純粋な語句検索 → テキスト証跡のみ）

パネルのチャット/検索に上の「引かれる例」を入れて、**画面由来の証跡（`source = screen_ocr`）** が返ることを確認。フレームの再 OCR は `rescan_screen_frame`。

### 4.5 72h 保持と自動削除

- purge は **起動 30 分後から 30 分間隔**（`FRAME_PURGE_INTERVAL_MS`）で走る。削除が起きた時だけログ:
  ```
  [screen_ocr] purged N frame(s) older than 72 h
  ```
- 短時間の検証では発火しないので、**古いフレームを差し込んで**確かめる:
  ```
  sqlite3 …/memory.db "update screen_frames set created_at_ms = created_at_ms - 4*24*3600*1000 where id = (select min(id) from screen_frames);"
  ```
  → 次の purge 窓（最長 30 分）で消えること。

> ⚠️ **実機で必ず確認したい既知の穴**: purge ループは `visual.enabled` が true の間だけ回る（`capture_source.rs:302`）。**Visual recall を Off に戻すと、既に保存済みの JPEG は 72h を過ぎても消えない**（Off の間は purge も走らず、Off 時の即時パージも無い）。
> 手順: フレームを数枚溜める → Off にする → 上の SQL で `created_at_ms` を 4 日前にする → 30 分以上待つ → 行が**残っていれば穴が再現**。
> CLAUDE.md の invariant-2 例外は「期限切れは自動削除する」を条件にしているので、再現したら Off 時の即時パージ（または enabled に依らない purge）を入れる必要がある。

---

## 5. 差分 #1/#4 — 会議オーバーレイ

### 5.1 3 モード切替

会議を検知するとオーバーレイが出る（Zoom 起動、または Google Meet の URL がフォアグラウンド）。ヘッダーのモードピッカーで:

| モード | 期待挙動 |
|---|---|
| Transcription | 文字起こしのみ。ネットワーク送信なし |
| One-way | ASR 行 ＋ EN→JA 訳が後追いで埋まる |
| Two-way | 双方向（JA→EN はオンデバイス whisper translate） |

`set_meeting_mode` が飛び、モードは設定に永続。

### 5.2 EN→JA live translate

**前提: Select KK キーが Keychain に必要**（`com.selectkk.shogun` / `select-kk-batch`）。モデルは `claude-haiku-4-5-20251001`（Messages API 同期。Batch は recap/dream 用）。

```
security add-generic-password -s com.selectkk.shogun -a select-kk-batch -w
```

期待挙動（`grep "live translate" /tmp/shogun.log`）:

- **ASR 行が先に出て、訳が後から埋まる**（訳待ちで字幕が止まらない ＝ 設計意図）。1 行あたり 1〜3 秒。
- 鍵が無い → オーバーレイに「鍵が必要」表示（`meeting_translate_needs_key`）＋ `skipped — no Select KK key`。
- 鍵が不正 → `meeting_translate_key_invalid` ＋ `check Select KK key format`。
- 早口 → `skipped — 3 already in flight`（スレッドが積み上がらない）。
- 2.5 秒以内の同一 ASR → `skipped — duplicate ASR`。
- 429 → `rate limited … retry n/4` → 以降 30 秒 `rate-limit cooldown active`。**この間も ASR 行は残る**（訳が付かないだけ）。
- 無音・非発話 → `skipped — non-speech/blank ASR`。
- モデルが「訳すものがありません」等を返した場合 → `dropped refusal/meta-chat`（ASR 行は保持）。

ASR が遅い場合は Metal を有効化: `SHOGUN_WHISPER_GPU=1 pnpm tauri dev`（既定 OFF。ログ `[meeting] whisper GPU off …`）。

### 5.3 クリックスルーのポインタ hit-test（差分 #4 — 前回の実機ブロッカー）

意図: オーバーレイのガラスカードの**上だけ**クリックを受け、それ以外は背後のアプリに抜ける。

確認:
1. オーバーレイ表示中、**カードの外側**（透明部分）で背後の Zoom / ブラウザをクリック → **背後が反応する**。
2. カードの上にカーソルを乗せる → ログ `[meeting] overlay interactive=true`、ボタンが押せる。
3. カードから外す → `[meeting] overlay interactive=false`。
4. カードのヘッダー/グリップ/フッターをドラッグ → オーバーレイが動く（本文・アクション・モード/言語メニューの上ではドラッグしない）。

`interactive=true` のまま張り付く / `false` のままボタンが押せない、が**最重要の観測点**。

### 5.4 会議終了の検知

- Zoom: アプリ終了で終了判定。
- Meet: タブを離れても **猶予時間内はマイクが開いている限り継続**（`meet_url_session_present`）。猶予超過＋マイク静音で終了（`call_clearly_ended`）。
- 確認: Meet 中に別タブへ移って戻る → セッションが切れない。会議を閉じてマイクが閉じる → 終了して recap が走る。

---

## 6. プライバシー不変条件のスポットチェック（毎回やる）

| 確認 | 方法 | 期待 |
|---|---|---|
| 音声ファイルが生まれない | §4.3 の `find` | ヒット 0 |
| 画像は `screen_frames` の中だけ | §4.3 の `find` | ヒット 0 |
| Visual recall 既定 OFF | 初回起動ログ | `off (default)` |
| ログにキャプチャ本文が出ない | `grep -iE "live translate|screen_ocr" /tmp/shogun.log` | 本文ではなく件数・理由・ts のみ |
| 第三者経由の明示 | トレーサビリティ画面 | Composio 経由に「第三者経由」表示 |

---

## 7. 詰まったら

| 症状 | 対処 |
|---|---|
| Visual recall を On にしても OCR が動かない | **画面収録**権限（§1）→ アプリ再起動。次にターミナル/Docs を開いて 10 秒待つ |
| `frames_count` が増えない | ピクセル署名が同一で skip されている。画面内容を変える |
| 訳が全く付かない | Select KK キー（§5.2）。ログの `skipped —` 理由を読む |
| オーバーレイのクリックが抜けない/抜けすぎる | §5.3。`[meeting] overlay interactive=` の遷移をログで追う |
| Keychain ダイアログが毎回出る | §1 の codesign。ad-hoc なら Apple Development 証明書に変える |
| DB 関連で起動しない | §3 の隔離が動くはず。動かなければそのログを貼る |
| パネル描画が壊れる | `SHOGUN_NO_NOTCH=1` で通常ウィンドウ。コアは ⌘⇧J で確認（製品 shell 待ち） |

---

*観測結果・詰まった点・§4.5 の穴の再現有無は `docs/phase1-findings.md` に追記して次セッションへ引き継ぐこと。*
