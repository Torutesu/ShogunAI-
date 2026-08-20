# Product Hunt ローンチ原稿 — ShogunAI

**Status**: v3（2026-08-20）
**方針**: **招待時期・ウェーブ・日付の約束は書かない。**伝えるのはプロダクトそのもの——何を記憶し、何を実行し、何をしないか。
**原稿ルール**: **すべての外向け原稿に日本語訳を併記する。**JAは翻訳ではなく、同じ主張の日本語ネイティブ表現にする（`shogun-brand` §11）。
**準拠**: `shogun-brand`（トーン・NGワード）、`docs/positioning-category-messaging.md`（差別化の言い方）、`CLAUDE.md`（不変条件・プラン）。

---

## 0. 確定事項

| 項目 | 内容 |
|---|---|
| **Name** | `ShogunAI` |
| **Tagline** | `Your personal AG on your PC. Built to finish real work.`（55/60。オーナー確定）<br>`AG` → `AGI` だけ投稿前に確認 → §2.1。**PC 表記は維持**（Windows・モバイルも作るため） |
| **launch形態** | アーリーアクセス（メール登録1フィールド）。**招待時期は言わない** |
| **カテゴリ** | Productivity / Artificial Intelligence / Mac |
| **日時** | 火曜 00:01 PT ＝ 火曜 16:01 JST |
| **伝える中身** | §1 の8点。これ以外は足さない |
| **プラットフォーム** | プロダクトは**PCの中で動くもの**として語る。**今日動くのは macOS 版**で、Windows とモバイルは開発中。この2つを必ずセットで書く（片方だけだと誤解か機会損失になる） |
| **書かないもの** | 招待の日付・ウェーブ番号・「◯月オープン」／**Windows・モバイルが今日動くかのような書き方**／競合の名指し／"AI-powered" "revolutionary" "second brain"／絵文字（⚔ のみ可）／未実測の性能数値 |

---

## 1. ユーザーに伝えるべき8点

PHで読まれるのは最初の3行と画像だけ。**この8点以外は当日の武器にしない。**各点に「事実（実装の裏取り）」と「言い方（EN / JA）」を付けてある。

### ① 一日を記憶する。ただし録らない

**事実**: 画面上のテキストを macOS の accessibility 層から取る。スクリーンショット・録画・音声ファイルを一切作らない（`CLAUDE.md` 不変条件2）。パスワードマネージャとプライベートブラウジングは既定で除外、セキュアな入力欄はサブツリーごとスキップ。任意のアプリ・ウィンドウタイトルを除外できる。

> **EN**: It remembers your workday as text — what you read, wrote and decided. No screenshots, no video, no audio files, ever. Password managers and private browsing are excluded by default, and you can exclude anything else.
>
> **JA**: 一日の仕事をテキストとして記憶します。読んだもの、書いたもの、決めたこと。スクリーンショットも録画も音声ファイルも作りません。パスワード管理アプリとプライベートブラウジングは既定で除外、他も自分で除外できます。

### ② データはあなたのPCから出ない

**事実**: メモリは端末上の暗号化DB（SQLCipher）。エクスポートと全削除は設定のボタン。モデル呼び出しで外に出るのは、そのリクエストに必要な分だけ。

> **EN**: Your memory lives in an encrypted database on your own machine. Export it or delete all of it from settings — not a support ticket. When a model call happens, only what that request needs leaves, and it's logged in the app.
>
> **JA**: 記憶はあなたのPCの中、暗号化されたデータベースにあります。書き出しも全削除も設定のボタンひとつで、問い合わせは要りません。モデルを呼ぶときに外へ出るのはそのリクエストに必要な分だけで、送信の記録はアプリに残ります。

### ③ ログではなく「状態」を持つ

**事実**: state tables（people / projects / commitments / open_loops）。全レコードに**根拠（provenance）と確度（confidence）**が付き、低確度を事実として混ぜない。ここが検索ツールとの分岐点。

> **EN**: It doesn't just store a log, it keeps the state of your work — people, projects, commitments, open loops. Every record carries where it came from and how confident it is, and a low-confidence guess is never handed to you as a fact.
>
> **JA**: ログを溜めるだけでなく、仕事の状態を持ちます。人、プロジェクト、約束、やりかけ。すべてのレコードに根拠と確度が付いていて、確度の低い推測を事実として渡すことはありません。

### ④ ノッチから、1ボタンで仕事が終わる

**事実**: 文脈アクションは常時プリアセンブル（押してから集めない）。返信ドラフト、会議のrecap、予定の確保、ファイリング、フォローアップ。プリセットエージェント7種。

> **EN**: Open the notch and the actions are already there — the reply drafted with the right history, the recap, the calendar hold, the follow-up. It doesn't start thinking when you press the button; the context is assembled before you ask.
>
> **JA**: ノッチを開くと、アクションはもう並んでいます。経緯を踏まえた返信の下書き、会議のrecap、予定の確保、フォローアップ。押してから考え始めるのではなく、聞かれる前に文脈を組み立ててあります。

### ⑤ 送信は必ずあなたが承認する

**事実**: 読み取りは自動（L1/L2）、外部送信は例外なくL3。全文プレビューを見てからでないと出ない。draft-stop は既定ON。外部送信は全件トレーサビリティに残る。

> **EN**: Reading is automatic. Sending never is. Anything addressed to another human — mail, chat, a calendar invite — stops and shows you the full body first. There is no setting that lets it send on its own.
>
> **JA**: 読み取りは自動です。送信は自動になりません。人に宛てたもの（メール、チャット、招待）は必ず止まり、本文全部を見せてから確認を取ります。勝手に送る設定は用意していません。

### ⑥ 会議は議事録で終わらない

**事実**: 会議の検知、ライブ文字起こし、関係の履歴を踏まえたrecap、交わした約束の追跡。**音声はディスクに書かない**（一時ファイルも作らない）。会議機能は丸ごとオフにできる。

> **EN**: Meetings end with the next step, not a transcript. It knows what you promised this person last month, so the recap comes with the follow-up already drafted. Audio is never written to disk — not even a temp file — and you can leave meetings off entirely.
>
> **JA**: 会議が終わったときに残るのは議事録ではなく、次の一手です。先月その人と交わした約束を踏まえてrecapが出て、フォローアップの下書きまで進みます。音声はディスクに書きません（一時ファイルも作りません）。会議機能ごとオフにもできます。

### ⑦ 夜のうちに整理して、朝に渡す

**事実**: Dream Cycle（アイドル・ロック中のバッチ）で一日の生データを状態へ。Morning Brief は根拠リンク付きで、材料が薄い日も空にしない。

> **EN**: Overnight it reprocesses the day into state. In the morning you get three lines: what moved, what's gone stale, and what you owe people — each linking back to the evidence it came from.
>
> **JA**: 夜のうちに一日分を状態へ作り直します。朝に出るのは3行です。動いたもの、古くなったもの、そして誰に何を借りているか。どの行にも根拠へのリンクが付いています。

### ⑧ 頭脳はあなたが選ぶ

**事実**: BYOK（Anthropic / OpenAI互換）またはサブスク委譲（契約済みの Claude / ChatGPT / Gemini プランをローカルCLI経由で使う）。秘密はKeychainのみ。加えてMemory API（MCP / CLI / REST）で他のAIから同じ記憶を読める。

> **EN**: Bring your own model — your API key, or the Claude/ChatGPT/Gemini plan you already pay for. And your memory isn't locked in our UI: it's reachable over MCP, so your other AI tools can read the same context.
>
> **JA**: モデルはあなたが選びます。自分のAPIキーでも、すでに契約している Claude / ChatGPT / Gemini のプランでも動きます。記憶はこちらのUIに閉じ込めません。MCP経由で開いているので、他のAIツールから同じ文脈を読めます。

### 動作環境（毎回セットで言う。片方だけ書かない）

いま動くものと、これから来るものを**必ず2文で並べる**。前者だけだと「Macアプリ」に閉じ込められ、後者だけだと嘘になる。

> **EN**: On macOS today — macOS 14+, Apple Silicon. Windows and mobile are in the works, and the list is per-platform: tell us which machine you're on.
>
> **JA**: 今日動くのは macOS 版です（macOS 14 以上 / Apple Silicon）。Windows とモバイルも作っています。登録のときに、どのマシンを使っているか教えてください。

> 「PC」という言葉の扱い: プロダクトは**PCの中で動くもの**として語る（tagline の通り）。ただし「今日どのOSで動くか」を隠さない。ここを曖昧にすると、Windowsで登録した人が招待時に落胆し、その1件がコメント欄に貼られる。

### 言わないこと

- **未実測の性能数値**（「展開100ms」「CPU 5%」等）。社内SLOであって実機実測が未了。**外向けに数字で約束しない**
- **招待の時期・順番**（オーナー方針）。聞かれたら §7 Q1 の答え方で返す
- 未実装機能を実装済みのように言うこと。ギャラリーとコピーで語るのは `docs/feature-status.csv` が implemented の範囲だけ

---

## 2. 提出フォームの各欄

### 2.1 Name / Tagline

**Name（8/40）**
```
ShogunAI
```

**Tagline（55/60・確定）**
```
Your personal AG on your PC. Built to finish real work.
```
**JA（LP・X用の対応表現）**: 仕事の全域で動く、あなたのパーソナルAGI。実務を終わらせるために作りました。

投稿前の確認は1点だけ:

| 箇所 | 提案 | 理由 |
|---|---|---|
| `AG` → `AGI` | `Your personal AGI on your PC. Built to finish real work.`（56字） | `AG` は英語で語として通じず、読み手にはタイプミスに見える |

**`PC` は維持する。** Windows とモバイルも作る以上、プロダクトを「Macアプリ」として名乗ると自分で天井を作ることになる。ウェイトリストなら、Windows ユーザーの登録は**弾かれる相手ではなく需要データ**になる。

条件は一つだけ: **今日動くOSを本文で明示する。** description・first comment・LPのフォーム直下の3か所に、次の1行を必ず置く。

> **EN**: On macOS today. Windows and mobile are in the works.
> **JA**: 今日動くのは macOS 版です。Windows とモバイルも作っています。

> 語感の補足: 英語の "PC" は Windows 機を指すと読む人が一定数いる。中立に振るなら本文側は `your computer` を使う（tagline は確定どおり `PC` のまま）。本書の原稿はこの使い分けで書いてある。

### 2.2 Description（上限260）

**採用案（EN・253字）**
```
Personal AGI for your work, on your PC. It remembers your day as text — no screenshots, no recordings — keeps the state of it, and finishes things: replies, recaps, calendar holds. Nothing sends without your approval. macOS now, Windows and mobile next.
```

**JA**
> 仕事の全域で動くパーソナルAGIを、あなたのPCの中に。一日をテキストとして記憶し（スクショも録画も残しません）、人・約束・やりかけの状態を持ち、返信やrecap、予定の確保まで終わらせます。送信は必ずあなたの承認を挟みます。今日動くのは macOS 版で、Windows とモバイルも作っています。

### 2.3 その他の欄

| 欄 | 値 |
|---|---|
| **Topics** | Productivity / Artificial Intelligence / Mac（今日動くビルドがmacOSなので Mac は残す。Windows版が出たら差し替える） |
| **Links** | Website（`?utm_source=producthunt&utm_medium=launch&utm_campaign=ph_launch`）／X／LinkedIn／Privacy & Security ページ |
| **Platforms** | 今日動くもの＝**Mac** を選ぶ。Windows・iOS はビルドが出るまで選ばない（選ぶと「今すぐ使える」と読まれる）。**多プラットフォーム化は本文で言う**——欄で嘘をつかず、コピーで意図を伝える |
| **Pricing** | `Paid — with a free trial`。表示文 EN: `Joining early access is free. Plans start at $49/mo billed annually ($62 month-to-month), and every plan opens with a full trial.` ／ JA: 「登録は無料です。プランは年額で月あたり$49（月払いは$62）から。どのプランもフルトライアルから始まります」 |
| **Makers** | 顔写真・一行bio・SNSを埋めてから当日を迎える。チーム全員を maker に追加（各自のフォロワーへ通知が飛ぶ） |
| **Maker bio** | EN: `Building ShogunAI — a personal AGI for your work that runs on your own machine. Tokyo.` ／ JA: 「ShogunAI を作っています。あなたのPCの中で動く、仕事のためのパーソナルAGIです。東京」 |
| **Thumbnail** | 240×240。ノッチが開いて閉じる2秒ループ、文字なし。ギャラリー1枚目の縮小は使わない（潰れる） |
| **First comment** | 公開**直後**に投稿（§4）。予約にすると空白時間の質問が無回答で並ぶ |
| **通知設定** | コメント通知をメール/Slackへ。15分以内返信を維持できる体制にしてから公開する |

---

## 3. ギャラリーとデモ動画

### 3.1 ギャラリー（1270×760、7枚。この順）

**5枚以上を実画面にする。**モックだけのウェイトリストlaunchは見抜かれる。

| # | 内容 | キャプション（EN / JA） |
|---|---|---|
| 1 | ヒーロー: 黒(#080808)にノッチが開いた瞬間。goldは1アクセント | **Personal AGI, scoped to your work.** ／ 仕事の全域で動くAGIを、あなたのPCの中に |
| 2 | デモ動画（§3.2） | — |
| 3 | Recall: 自然文の問いに、根拠付きで答えている実画面 | **It knows the state of your work — with receipts.** ／ 仕事の状態を、根拠付きで把握しています |
| 4 | 実行: 1ボタン → 下書き → **送信前の承認プレビュー** | **Nothing sends until you say send.** ／ 送信は、あなたが押すまで起きません |
| 5 | Morning Brief | **Overnight it organizes. Morning it briefs you.** ／ 夜のうちに整理して、朝に渡します |
| 6 | プライバシー対比図: *Text, on your machine* ↔ *No screenshots. No recordings. No audio files.* | **We built it so we can't see your day.** ／ こちらから見られない作りにしてあります |
| 7 | できること一覧＋動作環境 | **What it does today.** ／ いま出来ること（最下部に小さく `On macOS today. Windows and mobile in the works.`） |

**7枚目の中身**（`docs/feature-status.csv` が implemented の範囲だけ）:
- Passive memory / recall（EN: Passive memory and recall ／ JA: 受動的な記憶と想起）
- Meeting detection, transcript, recap（会議の検知・文字起こし・recap）
- Morning brief（朝のブリーフ）
- Drafts, replies, calendar holds — with approval（下書き・返信・予定の確保、承認付き）
- Connected: Gmail, Google Calendar, Google Drive（接続済み）
- Runs on macOS today; Windows and mobile in the works（今日は macOS 版。Windows とモバイルも開発中）
- Next: Slack, then Notion, GitHub, Linear（次に来るもの）
- Your memory over MCP（MCP経由で他のAIから読める）

### 3.2 デモ動画（60〜70秒・音声なし・字幕・**実ビルド撮影**）

字幕は英語。**日本語版を作る場合は同じカットに下段字幕で重ねる**（別編集にしない）。

| # | 秒 | 画面 | 字幕 EN | 字幕 JA |
|---|---|---|---|---|
| 1 | 0-5 | 普通に仕事している画面 | `You already did the work. Your AI just doesn't know about it.` | 仕事はもう終えている。AIだけが知らない |
| 2 | 5-13 | ノッチを開く → アクションがもう並んでいる | `Open the notch. The context is already there.` | ノッチを開く。文脈はもう揃っている |
| 3 | 13-25 | 「Draft the follow-up」→ 経緯を踏まえた下書きがストリーミング | `It knows who they are and what you owe them.` | 相手が誰で、何を借りているかを知っている |
| 4 | 25-33 | 送信前の全文プレビュー＋承認 | `Nothing leaves your machine without approval.` | 承認なしにPCから出るものはない |
| 5 | 33-43 | 会議の検知 → recap＋フォローアップ下書き | `Meetings end with the next step, not a transcript.` | 会議のあとに残るのは、議事録ではなく次の一手 |
| 6 | 43-53 | 翌朝の Morning Brief | `It works overnight. You wake up briefed.` | 夜のうちに動く。起きたら整理が終わっている |
| 7 | 53-62 | 設定のプライバシー表記 | `No screenshots. No recordings. No audio files. Ever.` | スクショも録画も音声ファイルも、一切作らない |
| 8 | 62-70 | ロゴ＋CTA | `ShogunAI — personal AGI for your work. On macOS today; Windows and mobile next.` | ShogunAI — 仕事のためのパーソナルAGI。今日は macOS 版 |

> 撮影ルール: ダミーアカウントで撮り、実在の人名・社名・本文を映さない。**倍速編集しない**（実機性能を疑われる）。ノッチの展開は等速で1回。

---

## 4. First comment（公開直後に投稿）

### EN（原稿）

```
Hi Product Hunt ⚔

"Personal AGI" gets thrown around, so let me define it before anything else. I don't mean human-level anything. I mean general across your work instead of narrow to one task: one memory and one agent that span your mail, calendar, docs, chat, meetings and the text on your screen — and that act on what they know instead of answering questions about it.

Here's what that looks like in practice.

**It remembers your day as text.** What you read, wrote and decided. It is not a screen recorder: no screenshots, no video, no audio files, ever. Capture runs through the operating system's accessibility layer, and it stays in an encrypted database on your own machine. Password managers and private browsing are excluded by default; you can exclude anything else; export and delete-everything are buttons in settings.

**It keeps the state, not just the log.** People, projects, commitments, open loops — each record carrying where it came from and how confident it is. A low-confidence guess never reaches you dressed as a fact. That's the difference between a search box and something that can act.

**It finishes things.** Open the notch and the actions are already assembled: the reply drafted with the right history, the meeting recap that knows what you promised this person last month, the calendar hold, the morning brief on what moved overnight. Meetings end with the next step instead of a transcript, and audio is never written to disk — not even a temp file.

**And it stops before it sends.** Reading is automatic. Sending never is. Anything addressed to another human shows you the full body first and waits. There's no setting that lets it send on its own, because that's the one mistake you can't take back.

You bring your own model — your API key, or the Claude/ChatGPT/Gemini plan you already pay for. Your memory isn't locked in our UI either: it's reachable over MCP, so your other AI tools can read the same context.

On platforms: the build that runs today is macOS (14+, Apple Silicon). Windows and mobile are being built — this isn't a Mac product, it's a product that happens to have shipped its Mac build first. The list is per-platform, so tell me which machine you're on and it'll shape the order we ship in.

Early access is open — joining is free and nothing is charged to join.

Three things I'd genuinely like from this thread:
1. If you tried a memory tool before and dropped it, what made you quit? That's more useful to me than feature requests.
2. Which connection decides whether you'd leave it running for a week?
3. The privacy questions, and the "is AGI the right word" ones. Ask them here and I'll answer them here, on the record.
```

### JA（日本語圏向け。Xやnote、日本語コメントへの返信で使う）

```
Product Hunt に ShogunAI を出しました ⚔

「パーソナルAGI」は使われすぎた言葉なので、先に定義します。人間並みの知能という意味ではありません。単機能アプリの対義語としての「全域」です。メール、カレンダー、ドキュメント、チャット、会議、そして画面上のテキスト——その全部にまたがる一つの記憶と一つのエージェントがあり、知っていることについて答えるのではなく、それを使って動きます。

具体的にはこうなります。

**一日をテキストとして記憶します。** 読んだもの、書いたもの、決めたこと。画面録画ではありません。スクリーンショットも録画も音声ファイルも、一切作りません。取得は OS の accessibility 層を通り、データはあなたのPCの中、暗号化されたデータベースに残ります。パスワード管理アプリとプライベートブラウジングは既定で除外、他も自分で除外できます。書き出しと全削除は設定のボタンです。

**ログではなく状態を持ちます。** 人、プロジェクト、約束、やりかけ。すべてのレコードに根拠と確度が付いています。確度の低い推測が事実の顔をして出てくることはありません。ここが、検索窓と「動けるもの」の分かれ目です。

**そして、終わらせます。** ノッチを開くとアクションはもう組み上がっています。経緯を踏まえた返信の下書き、先月その人と交わした約束を知っているrecap、予定の確保、夜のうちに動いたものをまとめた朝のブリーフ。会議のあとに残るのは議事録ではなく次の一手で、音声はディスクに書きません。一時ファイルも作りません。

**送る前に必ず止まります。** 読み取りは自動、送信は自動になりません。人に宛てたものは本文全部を見せて待ちます。勝手に送る設定は用意していません。取り返しがつかない失敗はそこだけだからです。

モデルはあなたが選びます。自分のAPIキーでも、すでに契約している Claude / ChatGPT / Gemini のプランでも動きます。記憶もこちらのUIに閉じ込めません。MCP経由で開いているので、他のAIツールから同じ文脈を読めます。

動作環境について。今日動くビルドは macOS 版です（macOS 14 以上 / Apple Silicon）。Windows とモバイルも作っています。Macのためのプロダクトではなく、Mac版が先に出ただけです。登録のときにどのマシンを使っているか教えてもらえると、出す順番の判断材料になります。

アーリーアクセスの登録は無料で、登録時に課金は発生しません。

聞きたいことが3つあります。
1. 記憶系のツールを使ってやめた経験がある人。何がきっかけでやめましたか。機能要望より、そちらが知りたいです。
2. どの連携があれば、1週間つけっぱなしにできますか。
3. プライバシーの質問と、「AGIという言葉は適切か」という指摘。ここで聞いてもらえれば、ここで答えます。
```

---

## 5. 中盤に落とす Maker follow-up

**(a) 6時間後 — 技術編（builder票を拾う）**

> **EN**: A few people asked how capture works without screenshots, so: we walk the accessibility tree of the focused window and keep text only — bounded walk, near-duplicate collapse, password managers and private browsing excluded by default, secure text fields skipped at the subtree level. Secrets are redacted before anything is written. The database is encrypted on device; hot for 24 hours, warm for 30 days, then compressed. Forgetting is part of the design, not a cleanup job.
>
> **JA**: スクショなしでどう取っているのか、という質問が来たので。フォーカス中のウィンドウのアクセシビリティツリーを範囲を区切って辿り、テキストだけを残します。ほぼ同じ内容は畳み、パスワード管理アプリとプライベートブラウジングは既定で除外、セキュアな入力欄はサブツリーごと飛ばします。秘匿値は書き込む前にマスクします。データベースは端末上で暗号化。24時間はホット、30日はウォーム、その先は圧縮。忘れることは後片付けではなく設計の一部です。

**(b) 10〜12時間後 — 使い方の実例（滞在時間を伸ばす）**

> **EN**: The clearest "aha" from early users isn't recall — it's Monday morning. Overnight the app turns the week into state: who you owe something to, what's gone stale, what got promised in a meeting and never landed anywhere. The brief is three lines and every line links back to the evidence. That's the part people say they can't unsee.
>
> **JA**: 最初に効くのは検索ではなく、月曜の朝でした。夜のうちに一週間分が状態へ変わります。誰に何を借りているか、何が古くなったか、会議で約束されたのにどこにも着地していないものは何か。ブリーフは3行で、各行が根拠へリンクします。ここを見たら戻れない、と言われる部分です。

**(c) 予備 — AGI 論争が伸びたときだけ**

> **EN**: The reason I say "general" rather than "assistant" is that the failure mode of assistants is narrowness, not intelligence. A dictation app can't know who Mika is. A meeting recorder can't know what you promised her last month. Each is excellent inside its own hour and blind outside it. The engineering that matters isn't a smarter answer — it's one state of your work that every one of those moments reads from and writes back to.
>
> **JA**: 「アシスタント」ではなく「全域」と言うのは、アシスタントの失敗の仕方が知能不足ではなく視野の狭さだからです。音声入力アプリは、ミカが誰かを知りません。会議ツールは、先月あなたが何を約束したかを知りません。どれも自分の1時間の中では優秀で、外側には目が届かない。効くのは賢い回答ではなく、そのすべての瞬間が読み書きする一つの状態です。

---

## 6. 当日タイムテーブル（PT / JST・PDT基準）

| PT | JST | やること |
|---|---|---|
| 00:01 | 16:01 | 公開。**First comment を即投稿** |
| 00:05 | 16:05 | X / LinkedIn / Discord に同時告知（§8）。**「upvoteして」と書かない**（規約違反） |
| 00:30-02:00 | 16:30-18:00 | 全コメントに**15分以内**返信。初動2時間で立ち上がりが決まる |
| 02:00-05:00 | 18:00-21:00 | JPゴールデンタイム。JA投稿。日本語コメントには日本語で返す |
| 06:00 | 22:00 | follow-up (a) 技術編 |
| 08:00-11:00 | 00:00-03:00 | **米東海岸の朝＝票の本番**。返信が止まると失速。交代要員必須 |
| 10:00 | 02:00 | follow-up (b) 使い方編 |
| 14:00 | 06:00 | 中間報告をXへ（順位ではなく**聞かれた質問**を共有すると伸びる） |
| 20:00 | 12:00 | 追い込み。未返信ゼロを確認 |
| 23:59 | 15:59 | 締め。翌日「初日に学んだこと」をXへ（順位に関係なく出す） |

**禁止**: 票の依頼（DM含む）、複数アカウント、競合の名指し批判、ネガティブコメントへの反論バトル。

---

## 7. コメント返信テンプレ（EN / JA）

> 原則: ①懸念を認める ②構造で答える ③検証できるものへリンクする。**日本語コメントには日本語で返す。**

**Q1. いつ使えるの / 招待はいつ**（日付は言わない）
> **EN**: I'm not going to give you a date I can't hold, so here's the honest version: we're letting people in gradually, and the list is the queue. Sign up and you'll get an email when it's your turn — no drip campaign in the meantime.
>
> **JA**: 守れない日付を言いたくないので、正直に書きます。順番に開けていて、登録リストがそのまま順番待ちです。登録しておいてもらえれば、順番が来たときにメールを送ります。それまで宣伝メールは送りません。

**Q2. 実物あるの？ デモはモックでは**
> **EN**: Everything in the video is the real app on a real machine — no mockups, no speed-up. The reason access is gradual isn't that it doesn't run; it's that capture touches every app you use, and I'd rather fix the first hundred edge cases before inviting the next thousand.
>
> **JA**: 動画は全部、実機で動いている実物です。モックも倍速編集もありません。順番に開けているのは動かないからではなく、キャプチャが使っている全アプリに触れる以上、最初の100人分の例外を潰してから次を呼びたいからです。

**Q3. AGI って言い過ぎでは**
> **EN**: Fair challenge, and I'm not claiming human-level anything. I mean it narrowly: general across your work instead of narrow to one task. One memory and one agent spanning mail, calendar, docs, chat, meetings and your screen, keeping the state of it — people, promises, open loops, each with a source and a confidence — and acting on that state. If there's a better word for that, I'll take it.
>
> **JA**: もっともな指摘で、人間並みの知能を主張してはいません。狭い意味で使っています。単機能に対する「全域」です。メール、カレンダー、ドキュメント、チャット、会議、画面にまたがる一つの記憶とエージェントが、人・約束・やりかけの状態を根拠と確度付きで持ち、その状態から動く。これを指す良い言葉があれば、乗り換えます。

**Q4. Windows は？ / iPhone は？（PC と書いてある以上、必ず来る）**
> **EN**: The build that runs today is macOS — 14+, Apple Silicon. Windows and mobile are being built; I'm not going to guess at dates here. This was never meant as a Mac product: we started there because the accessibility layer let us read on-screen text reliably without recording anything, which is what makes the no-screenshots promise work. Sign up and tell me which machine you're on — that's the input I'm using to decide what ships next.
>
> **JA**: 今日動くビルドは macOS 版です（macOS 14 以上 / Apple Silicon）。Windows とモバイルは開発中で、時期はここでは言いません。もともとMac専用のつもりで作っていません。録画せずに画面のテキストを確実に読める入口がそこにあったので、最初に出ただけです。「スクショを撮らない」という約束はそこで成立しています。登録するときにどのマシンを使っているか教えてください。次に何を出すかの判断材料にします。

**Q5. 監視ツールでは？ 全部見られているのでは**
> **EN**: It's the first thing I'd ask too. Three structural answers: capture is text only — no screenshots, no video, no audio files; it stays in an encrypted database on your own machine; and you can exclude any app or window title, with password managers and private browsing excluded by default. Full export and delete-everything are buttons in settings, not a support form.
>
> **JA**: 私も最初に聞きます。構造で3つ答えます。取得するのはテキストだけで、スクショも録画も音声ファイルも作りません。データは端末上の暗号化データベースに留まります。どのアプリもウィンドウタイトルも除外でき、パスワード管理アプリとプライベートブラウジングは既定で除外です。書き出しと全削除は設定のボタンで、問い合わせフォームではありません。

**Q6. でもクラウドに送っているんでしょ**
> **EN**: Only what a request needs, and only when you ask. Reading is local. When a model call happens, the relevant chunk goes to the provider you chose with your own credentials, and every outbound call is logged in the app. Sends to other people are a separate category: those always stop for your approval with the full body shown first.
>
> **JA**: 必要な分だけ、頼まれたときだけです。読み取りはローカルで完結します。モデルを呼ぶときは、あなたが選んだ提供元へ、あなたの資格情報で該当部分だけを送り、送信は毎回アプリに記録が残ります。人への送信は別扱いで、必ず本文全部を見せて承認を待ちます。

**Q7. $49 は高い**
> **EN**: Honest answer: it's priced for people whose hour is worth more than the subscription, and there's no free tier because it runs all day on real infrastructure. Joining costs nothing and nothing is charged at that point — every plan opens with a full trial, so you decide after you've seen it work on your own week.
>
> **JA**: 正直に言うと、1時間の価値がこの購読料を上回る人向けの値付けです。一日中動くものなので無料プランは置いていません。登録は無料で、その時点の課金もありません。どのプランもフルトライアルから始まるので、自分の一週間で動くところを見てから決めてもらえます。

**Q8. ◯◯（競合名）とどう違うの**
> **EN**: I won't do a feature grid on someone else's product, but the categorical difference is this: recorders and lifeloggers end at "found it." Here memory is the fuel — it keeps a live model of your work, each record with a source and a confidence, and the output is a finished draft or a scheduled hold, not a search result.
>
> **JA**: 他社のプロダクトと機能表で比べることはしませんが、カテゴリとしての違いははっきりしています。記録系のゴールは「見つかった」です。こちらでは記憶は燃料で、仕事の状態を根拠と確度付きで持ち、出てくるのは検索結果ではなく完成した下書きや確保済みの予定です。

**Q9. モデルのラッパーでは**
> **EN**: The model part is a wrapper — deliberately, you bring your own. What isn't: passive capture at the OS level, a world model where every record carries its source and confidence, forgetting by design, and an approval gate on anything outbound. Swap the model tomorrow and the memory is still yours, on your disk. That's the asset.
>
> **JA**: モデルの部分はラッパーです。意図的にそうしていて、あなたが選んだものを使います。ラッパーでないのは、OSレベルの受動的な取得、根拠と確度を全レコードに持つワールドモデル、設計としての忘却、そして外に出るものへの承認ゲートです。明日モデルを差し替えても、記憶はあなたのディスクに残ります。そこが資産です。

**Q10. メールを渡す理由がない**
> **EN**: Reasonable. It's one field, stored on its own, we don't sell or share it, and you'll get exactly two kinds of mail: your invite, and a short note if something material changes. Ask and the record is deleted — no dark pattern.
>
> **JA**: もっともです。入力は1項目、単独で保管し、販売も共有もしません。届くメールは2種類だけです。招待と、重要な変更があったときの短い連絡。削除を頼まれたらレコードごと消します。引き止める仕掛けは入れていません。

**Q11. オープンソースにする予定は**
> **EN**: Not the app today. What's open in practice: your data is a local database you can export in full, and the memory is reachable over MCP, so you can point your own agents at it instead of being locked into our UI.
>
> **JA**: 今のところアプリ本体は公開していません。実質的に開いているのはデータの側です。ローカルのデータベースを丸ごと書き出せますし、記憶はMCP経由で読めるので、自分のエージェントを繋げます。こちらのUIに縛られません。

**Q12. 重くない？ バッテリーは**
> **EN**: It's bounded on purpose: we walk the focused window only, collapse near-duplicates, and accumulate dwell instead of re-reading. If your machine says otherwise, that's a bug report I want — send it and I'll chase it.
>
> **JA**: 意図的に範囲を絞っています。見ているのはフォーカス中のウィンドウだけで、ほぼ同じ内容は畳み、読み直す代わりに滞在時間を積みます。実機で重いなら、それは不具合報告として受け取りたいです。送ってもらえれば追いかけます。

**Q13. Apple が同じことをやったら**
> **EN**: Then a lot of people get a better OS, which is fine by me. The part that's hard to copy is the execution loop across your other tools with an approval model on top — cross-app, cross-vendor work Apple has historically not wanted to own.
>
> **JA**: そのときは多くの人がより良いOSを手にするので、それでいいと思っています。真似しにくいのは、他社ツールをまたぐ実行と、その上に載る承認の仕組みです。アプリとベンダーをまたぐ領域は、Appleが歴史的に持ちたがってこなかった部分です。

**Q14. BYOK が面倒**
> **EN**: You don't need an API key if you already pay for Claude, ChatGPT or Gemini — it can run through the plan you already have, with your explicit opt-in. Keys are the fallback, and either way they live in the system Keychain, never in a config file.
>
> **JA**: すでに Claude / ChatGPT / Gemini を契約しているなら、APIキーは要りません。明示的に同意してもらったうえで、そのプランの枠で動かせます。キーはフォールバックで、どちらの経路でも保存先はシステムのKeychainだけです。設定ファイルには書きません。

**Q15. 会議の音声はどこへ行く**
> **EN**: Live transcription for meetings runs through a cloud speech provider, opted out of any model training, and audio is never written to disk — not even a temp file. What gets stored is the text and where it came from. It's disclosed in the app before you turn meetings on, and you can leave meeting capture off entirely.
>
> **JA**: 会議のライブ文字起こしはクラウドの音声認識を通ります。学習利用はオプトアウトしてあり、音声はディスクに書きません。一時ファイルも作りません。保存されるのはテキストと、その出どころだけです。会議機能を有効にする前にアプリ内で開示しますし、丸ごとオフのままでも使えます。

**Q16. ネガティブ／辛辣なコメント**
> **EN**: That's a fair hit and I'm not going to argue it. [認める点を1文] Here's what I'll do about it: [具体的に1文]. Ping me when it lands and tell me if it actually fixed your case.
>
> **JA**: もっともな指摘で、言い返すつもりはありません。［認める点を1文］。これに対してやることはこれです：［具体的に1文］。反映されたら声をかけてください。実際に解決したかどうかを教えてほしいです。

---

## 8. 外部拡散の原稿（EN / JA）

> **共通**: どこにも「upvoteして」と書かない。「出した／ここにいる／聞いてくれ」で通す。

### X

> **EN**
> ```
> ShogunAI is live on Product Hunt ⚔
>
> Personal AGI for your work — narrowly: general across your work instead of narrow to one task.
>
> It remembers your day inside your own machine as text (no screenshots, no recordings), keeps the state of it, and finishes things. Nothing sends without your approval.
>
> On macOS today. Windows and mobile next.
> ```
>
> **JA**
> ```
> ShogunAI を Product Hunt に出しました ⚔
>
> 仕事の全域で動くパーソナルAGIです。人間並みの知能という意味ではなく、単機能アプリの対義語としての「全域」です。
>
> PCの中で一日をテキストとして記憶し、人・約束・やりかけの状態を持って、実行まで行きます。送信は必ず承認を挟みます。
>
> 今日動くのは macOS 版です。Windows とモバイルも作っています。
> ```

### X（中盤スレッド）

> **EN**: The most common question in the first three hours wasn't a feature request. It was "how do I know it isn't watching me?" Here's the actual answer, in the order it matters: [4ツイートで規律を分解 → 最後にPHリンク]
>
> **JA**: 最初の3時間で一番多かったのは機能の質問ではなく、「監視されていないとどう分かるのか」でした。順番に答えます。［4ツイートで分解 → 最後にPHリンク］

### LinkedIn

> **EN**: We opened early access for ShogunAI on Product Hunt today. We call it a personal AGI in a deliberately narrow sense: general across your work rather than narrow to a single task. It remembers your workday inside your own machine as text — never screenshots or recordings — keeps the state of it (people, projects, commitments, open loops, each with a source and a confidence), and turns that into finished work: drafted replies, meeting recaps that know the relationship, calendar holds, a morning brief. Anything addressed to another person stops for your approval first. The macOS build runs today; Windows and mobile are in the works.
>
> **JA**: 本日、ShogunAI のアーリーアクセスを Product Hunt で公開しました。パーソナルAGIと呼んでいますが、意味は狭く取っています。単機能ではなく、仕事の全域で動くという意味です。PCの中で一日をテキストとして記憶し（スクショや録画は残しません）、人・プロジェクト・約束・やりかけの状態を根拠と確度付きで持ち、そこから返信の下書き、関係の経緯を踏まえた会議recap、予定の確保、朝のブリーフまで進めます。人に宛てたものは必ず承認を挟みます。今日動くのは macOS 版で、Windows とモバイルも作っています。

### Discord / Slack

> **EN**: We're live on PH today ⚔ — ShogunAI, local-first work memory that drafts and executes instead of just recalling (macOS build today, Windows and mobile in the works). I'm in the comments all day if you want to grill me on the privacy model: [link]
>
> **JA**: 本日 PH に出しました ⚔ ShogunAI — 思い出すだけで終わらず、下書きと実行まで行く、ローカルファーストな仕事の記憶です（今日は macOS 版、Windows とモバイルも開発中）。プライバシー設計を突きたい人向けに、一日コメント欄にいます: [link]

### Show HN（AGIという語は出さない。機能と規律の言葉だけで通す）

```
Show HN: ShogunAI – Local-first work memory for macOS that drafts and executes
```
本文: accessibility tree のバウンド走査、暗号化ローカルDB、Hot/Warm/Cold、provenance＋confidence、外部送信の承認モデル、BYOK。マーケ語をゼロにする。JA版は不要（HNは英語圏）。

---

## 9. KPI

| 指標 | 目標 |
|---|---|
| PH順位 | Top 5 of the Day |
| PHコメント数 | 60+（メイカー返信を除く） |
| LPセッション（当日） | 3,000〜6,000 |
| **LP→登録CVR** | **25%以上**（ウェイトリストlaunchの本KPI。下回るならヒーローかフォーム位置の問題） |
| 当日の新規登録 | 800〜1,500 |
| 招待後の起動＋権限許可完了 | 50%以上（**順位より重要**） |
| 招待から7日後もキャプチャが動いている | 30%以上 |

---

## 10. LP側の必須修正（ローンチ前に必ず）

ウェイトリストlaunchでは**LPが唯一の実体**。粗があると票がそのまま逃げる。

1. **架空のテスティモニアル** — `apps/website/src/i18n/dictionaries.ts` の `testimonials.items` に実在しない人名（`alex_builds` / `Maria Kowalski` / `Kenji Tanaka` 等）。`Testimonials.tsx` がそのまま表示している。**実名許諾コメントに差し替えるか、セクションごと非表示**の二択。
2. **`stats` の "4h saved per week, on average"** — 根拠のない平均値。落とすか、検証可能な事実に差し替える。
3. **PHバッジ文字列** — 辞書に `badges.productHunt = '#1 Product of the Day'` が残っている（現在の表示は `authority` の "Coming soon on Product Hunt" のみ）。未取得実績の文字列は削除し、取れた場合のみ公式バッジを貼る。
4. **紹介プログラムの残骸** — API群は404化済みだが辞書に `hero.invitedBy` / `invitedTier` が4言語分残存。動かない特典を期待させない。
5. **登録動線**（今回の本丸）
   - ヒーロー内にメール1フィールドをスクロールなしで置く
   - フォーム直下に1行: `On macOS today. Windows and mobile in the works.` ／「今日動くのは macOS 版です。Windows とモバイルも作っています」
   - **フォームにプラットフォーム選択（Mac / Windows / Mobile）を足す。** 現状 `waitlist_email_capture` は email と created_at のみで、どのOSの需要が来ているか取れない。Windows・モバイルを作ると決めた以上、**この1フィールドがローンチ当日の最大の収穫になる**（小改修）
   - 送信後の文言を `waitlist.okListed`（"You're on the list."）から、**次に何が起きるかを書いた2〜3行**へ。**日付は書かない**
   - 自動返信メール1通（何が届くか・課金は発生しないこと・解除方法）。ここでも日付を約束しない
6. **Privacy & Security ページ**を §7 Q5/Q6/Q15 の粒度に整える（コメントから直リンクして即答するため）
7. **参加者カウント** — 当日D1が生きていることを確認（フォールバックの既定値が出ていると水増しに見える）

---

## 11. 出す前の最終チェック

- [ ] 招待の日付・ウェーブ番号・「◯月オープン」が原稿に**一つも残っていない**か
- [ ] すべての外向け原稿に**日本語版**が付いているか（Show HN を除く）
- [ ] 未実測の性能数値（100ms / CPU 5% 等）を外向けに書いていないか
- [ ] ギャラリーで語る機能が `docs/feature-status.csv` の implemented 範囲内か（将来分は "Next" と明示）
- [ ] `personal AGI` の定義が first comment の冒頭にあるか（定義なしの単独使用はブランド規約の禁じ手）
- [ ] タグラインの `AG` を確認したか（`PC` は維持。§2.1）
- [ ] 「今日動くのは macOS 版／Windows・モバイルは開発中」の2文が description・first comment・LPフォーム直下の3か所にあるか
- [ ] Windows・モバイルが**今日動くかのような書き方**になっていないか
- [ ] デモ動画が実ビルドの実画面か（倍速・モック混入なし）
- [ ] 絵文字は ⚔ のみか／競合の固有名がゼロか
- [ ] "AI-powered" / "revolutionary" / "second brain" 不使用か
- [ ] 実行の訴求すべてに**承認（approval）**が併記されているか
- [ ] gold の面積が5%以下か（全ギャラリー画像）／Kamonの余白1/6
- [ ] §10 の1〜4（架空テスティモニアル・未実証数値・未取得バッジ・紹介残骸）が消えているか
- [ ] 登録後の文言と自動返信メールが動くか（本番で実登録してテスト）

---

## 12. 付録: ローンチ運用（v2から引き継ぎ。日付の約束は含まない）

### 12.1 ローンチ前のブロッカー

ウェイトリストlaunchでは、アプリの不具合ではなく**登録動線と信頼**がブロッカーになる。

| 優先 | 項目 | 状態 |
|---|---|---|
| P0 | 実機デモ映像の撮影（§3.2） | 未。モック混入なしで撮り切る |
| P0 | LPの虚偽・未実証表記の除去 | §10 の1〜4 |
| P0 | 登録後の期待値設定（サンクス文言＋自動返信1通、**日付は書かない**） | 現状は "You're on the list." の1行のみ |
| P1 | 参加者カウントの正直さ | `/api/waitlist/count` がD1の実数を返しているか当日確認 |
| P1 | PH流入の識別 | `waitlist_email_capture` は email と created_at のみ。当日の時間窓で切るなら改修ゼロ、属性で切るなら `source` カラム追加 |
| P2 | アプリのP0/P1（オフラインクラッシュ、meeting overlay の再レンダ、マルチディスプレイ） | `todo.md`。**招待を出し始める前**に閉じる |

### 12.2 ハンター／動員

- **セルフハントでよい。**メイカーが終日コメント欄にいる方が効く。
- やってよい: 予告投稿、Discord/メールへの当日アナウンス、PHでの自分のフォロワー獲得（公開時に通知が飛ぶ）。
- **やってはいけない**: 票の直接依頼、インセンティブ付き投票、投票用アカウントの誘導。露見すると当日ランキングから外れる。
- 事前に確保: 実際に触った10〜20人に「当日、正直な感想をコメントで書いてほしい」と依頼する。**票ではなくコメントを頼む**——規約内で、かつ「実物がある」の最強の証拠になる。

### 12.3 ローンチ後（PHの1日より大事）

- **D+1**: 「初日に聞かれた質問トップ5と答え」をブログ＋Xへ。PHページにも追記コメント。
- **D+3**: 登録者への1通目。プロダクトの中身を1つだけ深掘りする。売り込まない。**日付を書かない。**
- **その後**: 招待を出した分だけ、起動→権限許可の完了率を実測し、次の規模を決める。数字が良くない間は募集より修理を優先する。
- バッジ（Top◯）は**取れた場合のみ**LPに貼る（§10-3）。
