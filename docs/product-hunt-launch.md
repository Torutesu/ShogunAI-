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
| **Tagline** | `Your personal AGI on your PC. Built to finish real work.`（56/60。**AGI で統一**）<br>**PC 表記は維持**（Windows・モバイルも作るため） |
| **launch形態** | アーリーアクセス（メール登録1フィールド）。**招待時期は言わない** |
| **カテゴリ** | Productivity / Artificial Intelligence / Mac |
| **日時** | 火曜 00:01 PT ＝ 火曜 16:01 JST |
| **伝える中身** | §1 の8点。これ以外は足さない |
| **プラットフォーム** | プロダクトは**PCの中で動くもの**として語る。**今日動くのは macOS 版**で、Windows とモバイルは開発中。この2つを必ずセットで書く（片方だけだと誤解か機会損失になる） |
| **書かないもの** | 招待の日付・ウェーブ番号・「◯月オープン」／**Windows・モバイルが今日動くかのような書き方**／競合の名指し／"AI-powered" "revolutionary" "second brain"／絵文字（⚔ のみ可）／未実測の性能数値 |

---

## 1. ユーザーに伝えるべきこと（実装の現在地に合わせた版）

> **2026-08-20 改訂の理由**: 旧版は「一日をテキストで記憶する／画像は保存しない」を軸に書いていたが、**それは今のプロダクトではない**。音声入力（hold-to-talk）と Scribe（その場書き換え）が入り、Visual recall で画像を持つ経路もできている。**「スクショは一切残しません」は、Visual recall をONにした人にとっては嘘になる。**以下は実装を読み直して書き直したもの（根拠は `docs/spec-implementation-drift-audit.md` §1.5）。

### ① 押して、話す。文字になって、そのまま入る

**事実**: ショートカットを**長押ししている間だけ**マイクが開く。離した瞬間に文字起こしが着地する。ライブ文字起こしはクラウドの音声認識を第一経路にし、オフライン時はオンデバイスへ落ちる。**音声はディスクにもDBにも書かない**（`voice_lane.rs`）。

> **EN**: Hold a key, talk, let go — the text lands where you were typing. It isn't blind dictation either: it already has your work context, so names and projects come out right. Audio is never written to disk or to the database.
>
> **JA**: キーを長押しして話し、離すと、打っていた場所にテキストが着地します。ただの音声入力ではありません。仕事の文脈を持っているので、人名やプロジェクト名が正しい形で出ます。音声はディスクにもデータベースにも書きません。

### ② どのアプリの文章でも、選んで言えば書き換わる（Scribe）

**事実**: 任意アプリの編集可能フィールドを Accessibility 経由で掴み、指示どおりに**その場で**書き換える。同一ターゲット・同一値・同一レンジであることを検証してから適用し、保護スパン（固有名詞・数値等）の保存も確認する（`scribe.rs`）。

> **EN**: Select text in any app, say what you want changed, and it rewrites in place — no copy, no paste, no separate window. It verifies it's editing exactly what you selected before it touches anything.
>
> **JA**: どのアプリでも、文章を選んで「こう直して」と言えば、その場で書き換わります。コピーもペーストも別ウィンドウも要りません。選んだ場所そのものを編集していることを確認してから触ります。

### ③ 一日を記憶する（既定はテキスト）

**事実**: 画面上のテキストを OS の accessibility 層から取る。既定では画像・動画・音声ファイルを作らない。パスワードマネージャとプライベートブラウジングは既定で除外、セキュアな入力欄はサブツリーごとスキップ。任意のアプリ・ウィンドウタイトルを除外できる。秘匿値は書き込み前にマスク。

> **EN**: By default it remembers your day as text — what you read, wrote and decided. Not a screen recorder: no video, no audio files. Password managers and private browsing are excluded out of the box, and you can exclude anything else.
>
> **JA**: 既定では、一日をテキストとして記憶します。読んだもの、書いたもの、決めたこと。画面録画ではありません。動画も音声ファイルも作りません。パスワード管理アプリとプライベートブラウジングは最初から除外で、他も自分で除外できます。

### ④ テキストが取れない画面は、あなたが許可したときだけ画像で補う（Visual recall）

**事実**: **既定オフ。**ONにすると、AXでテキストが取れない画面（Canvas系UI・画像内の文字）を圧縮JPEGとして**端末内の暗号化DB**に保持し、オンデバイスOCRでテキスト化する。保持期間は**ユーザーが選ぶ**（既定3日）。期限切れは自動削除。クラウドへは送らない。

> **EN**: Some windows give up no text — canvas apps, images, PDFs rendered as pixels. Turn visual recall on and it keeps an encrypted frame of those, on your machine, for a retention window you choose, and reads them with on-device OCR. It ships off. Nothing goes to the cloud, and expired frames delete themselves.
>
> **JA**: テキストを一切返さない画面があります。キャンバス系のアプリ、画像、画素として描かれたPDF。Visual recall をONにすると、そういう画面だけを暗号化したまま端末内に保持し、オンデバイスのOCRで読みます。既定はオフです。クラウドには送りません。期限が来たフレームは自動で消えます。

> ⚠ **コピーの注意**: 保持期間は現状**ユーザー設定で最長10年まで伸ばせる**（既定3日）。**「72時間で消えます」と書かない。**上限の扱いは `docs/spec-implementation-drift-audit.md` §2-C で判断待ち。

### ⑤ ログではなく「状態」を持つ

**事実**: state tables（people / projects / commitments / open_loops）。全レコードに根拠（provenance）と確度（confidence）。低確度を事実として混ぜない。

> **EN**: It doesn't just store a log, it keeps the state of your work — people, projects, commitments, open loops. Every record carries where it came from and how confident it is, and a low-confidence guess is never handed to you as a fact.
>
> **JA**: ログを溜めるだけでなく、仕事の状態を持ちます。人、プロジェクト、約束、やりかけ。すべてのレコードに根拠と確度が付いていて、確度の低い推測を事実として渡すことはありません。

### ⑥ ノッチから、1ボタンで仕事が終わる

**事実**: 文脈アクションは常時プリアセンブル（押してから集めない）。返信ドラフト、会議のrecap、予定の確保、ファイリング、フォローアップ。プリセットエージェント7種。

> **EN**: Open the notch and the actions are already there — the reply drafted with the right history, the recap, the calendar hold, the follow-up. It doesn't start thinking when you press the button.
>
> **JA**: ノッチを開くと、アクションはもう並んでいます。経緯を踏まえた返信の下書き、recap、予定の確保、フォローアップ。押してから考え始めることはありません。

### ⑦ 送信は必ずあなたが承認する

**事実**: 読み取りは自動（L1/L2）、外部送信は例外なくL3。全文プレビューを見てからでないと出ない。draft-stop は既定ON。外部送信は全件トレーサビリティに残る。

> **EN**: Reading is automatic. Sending never is. Anything addressed to another human — mail, chat, a calendar invite — stops and shows you the full body first. There is no setting that lets it send on its own.
>
> **JA**: 読み取りは自動です。送信は自動になりません。人に宛てたもの（メール、チャット、招待）は必ず止まり、本文全部を見せてから確認を取ります。勝手に送る設定は用意していません。

### ⑧ 会議は議事録で終わらない

**事実**: 会議の検知、ライブ文字起こし、関係の履歴を踏まえたrecap、交わした約束の追跡。**音声はディスクに書かない**（一時ファイルも作らない）。会議機能は丸ごとオフにできる。

> **EN**: Meetings end with the next step, not a transcript. It knows what you promised this person last month, so the recap comes with the follow-up already drafted. Audio is never written to disk — not even a temp file — and you can leave meetings off entirely.
>
> **JA**: 会議が終わったときに残るのは議事録ではなく、次の一手です。先月その人と交わした約束を踏まえてrecapが出て、フォローアップの下書きまで進みます。音声はディスクに書きません。一時ファイルも作りません。会議機能ごとオフにもできます。

### ⑨ 夜のうちに整理して、朝に渡す

**事実**: Dream Cycle（アイドル・ロック中のバッチ）で一日の生データを状態へ。Morning Brief は根拠リンク付き。

> **EN**: Overnight it reprocesses the day into state. In the morning you get what moved, what's gone stale, and what you owe people — each line linking back to the evidence it came from.
>
> **JA**: 夜のうちに一日分を状態へ作り直します。朝に出るのは、動いたもの、古くなったもの、誰に何を借りているか。どの行にも根拠へのリンクが付いています。

### ⑩ 頭脳はあなたが選ぶ／記憶は開いている

**事実**: BYOK（Anthropic / OpenAI互換）またはサブスク委譲（契約済みの Claude / ChatGPT / Gemini プランをローカルCLI経由で使う）。秘密はKeychainのみ。Memory API（MCP / CLI / REST）で他のAIから同じ記憶を読める。

> **EN**: Bring your own model — your API key, or the Claude/ChatGPT/Gemini plan you already pay for. And your memory isn't locked in our UI: it's reachable over MCP, so your other AI tools can read the same context.
>
> **JA**: モデルはあなたが選びます。自分のAPIキーでも、すでに契約している Claude / ChatGPT / Gemini のプランでも動きます。記憶はこちらのUIに閉じ込めません。MCP経由で開いているので、他のAIツールから同じ文脈を読めます。

### プライバシーの言い方（**この3文で言い切る。単独で「保存しません」と書かない**）

> **EN**: By default it keeps text, not pixels. Visual recall is opt-in, stays encrypted on your machine, and deletes itself on the schedule you set. Audio is never written to disk — live transcription runs through a speech provider that is opted out of model training, and what's stored is the text.
>
> **JA**: 既定で残すのはテキストで、画素ではありません。Visual recall は自分でONにする機能で、端末内で暗号化されたまま、あなたが決めた期間で自動的に消えます。音声はディスクに書きません。ライブ文字起こしは学習利用をオプトアウトした音声認識を通り、保存されるのはテキストだけです。

### 動作環境（毎回セットで言う。片方だけ書かない）

> **EN**: On macOS today — macOS 14+, Apple Silicon. Windows and mobile are in the works, and the list is per-platform: tell us which machine you're on.
>
> **JA**: 今日動くのは macOS 版です（macOS 14 以上 / Apple Silicon）。Windows とモバイルも作っています。登録のときに、どのマシンを使っているか教えてください。

### 言わないこと

- **「スクショは一切保存しません」の単独使用**。Visual recall がある以上、そのまま書くと不正確。上の3文で言う
- **「72時間で消えます」**。上限は現状ユーザー設定次第（§1-④の注意）
- **未実装のもの**: Evening Wrap / 朝夜サマリー配達、Skills（どちらも設計のみ・未着手）
- **招待の日付・ウェーブ番号**（オーナー方針）
- **未実測の性能数値**（展開100ms・CPU 5% 等。社内SLOであって実機実測が未了）

---

## 2. 提出フォームの各欄

### 2.1 Name / Tagline

**Name（8/40）**
```
ShogunAI
```

**Tagline（56/60・確定）**
```
Your personal AGI on your PC. Built to finish real work.
```
**JA（LP・X用の対応表現）**: あなたのPCの中の、パーソナルAGI。実務を終わらせるために作りました。

**綴りは `AGI` で全アセット統一**（2026-08-20 オーナー確定）。tagline・description・first comment・返信テンプレ・LP のどこでも `AG` と略さない。略記は読み手にはタイプミスに見え、定義の説明に一往復かかる。

**`AGI` を名乗る以上の作法**（`positioning §6` の禁じ手を満たすため）:
- **定義とセットでしか使わない。** first comment の冒頭で「人間並みの知能ではなく、**仕事の全域**という意味」と自分から定義する
- **到達宣言をしない。** "AGI is here" 型の表現は使わない
- 主張の芯は **general（全域）** に置く。知能は買える時代で、希少なのは仕事の全域に届くこと——この立て方で通す

**`PC` は維持する。** Windows とモバイルも作る以上、プロダクトを「Macアプリ」として名乗ると自分で天井を作ることになる。ウェイトリストなら、Windows ユーザーの登録は**弾かれる相手ではなく需要データ**になる。

条件は一つだけ: **今日動くOSを本文で明示する。** description・first comment・LPのフォーム直下の3か所に置く。

> **EN**: On macOS today. Windows and mobile are in the works.
> **JA**: 今日動くのは macOS 版です。Windows とモバイルも作っています。

> 語感の補足: 英語の "PC" は Windows 機を指すと読む人が一定数いる。中立に振るなら本文側は `your machine` を使う（tagline は確定どおり `PC`）。本書の原稿はこの使い分けで書いてある。

### 2.2 Description of the launch（上限500）

> **上限は500字**（PHの launch description 欄）。260字は tagline 側の制限であって、ここではない。**枠を使い切る。**主題は tagline の一文——パーソナルAGIが、あなたのPCの中で、実務を終わらせる。**機能名（音声入力・Scribe・会議）はここに書かない。**それらは first comment で「だから実務が終わる」の証拠として出す。

**採用案（EN・480字）**
```
Personal AGI won't arrive as a better chatbot. It arrives as an agent that knows the full state of your work — every person, every project, every promise — and acts on it. That is ShogunAI: general across your work rather than narrow to one task, and living inside your PC rather than someone else's cloud. It builds that state as you work, keeps the evidence behind every record, and spends it finishing real work. Reading is automatic. Sending always waits for you. macOS today.
```

**JA**
> パーソナルAGIは、賢くなったチャットボットとして来るのではありません。あなたの仕事の全状態——人、プロジェクト、交わした約束——を把握して動くエージェントとして来ます。それが ShogunAI です。単機能ではなく仕事の全域で動き、誰かのクラウドではなくあなたのPCの中に住みます。状態はあなたが働いている間に育ち、どのレコードにも根拠が残り、その状態は実務を終わらせるために使われます。読み取りは自動。送信は必ずあなたを待ちます。今日動くのは macOS 版です。

**予備案（EN・477字。「説明ではなく実行」を明示したい場合）**
```
Personal AGI won't arrive as a better chatbot. It arrives as an agent that knows the full state of your work — every person, every project, every promise — and acts on it. That is ShogunAI, general across your work rather than narrow to one task, and living inside your PC rather than someone else's cloud. It learns that state from your own day, keeps the evidence behind every record, and spends it finishing real work rather than describing it. Sending always waits for you.
```

**書き方の規律（この欄に限る）**
- 1文目で**カテゴリを定義し直す**（「AGIはチャットボットとしては来ない」）。機能の紹介文にしない
- 2文目で**主題**（全状態を把握して動く）、3文目で**固有名と2つの差別化軸**（general / あなたのPCの中）
- 4文目で**なぜ成立するか**（状態・根拠）、5〜6文目で**規律**（読み取りは自動、送信は待つ）
- **主張の芯は general（全域）**。知能は買える時代で、希少なのは仕事の全域に届くこと——この立て方を全アセットで統一する

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
| 3 | **音声入力＋Scribe**: キーを押しながら話す → テキストが着地 → 選んで指示 → その場で書き換わる | **Hold. Talk. Done.** ／ 押して、話して、終わり |
| 3b | Recall: 自然文の問いに、根拠付きで答えている実画面 | **It knows the state of your work — with receipts.** ／ 仕事の状態を、根拠付きで把握しています |
| 4 | 実行: 1ボタン → 下書き → **送信前の承認プレビュー** | **Nothing sends until you say send.** ／ 送信は、あなたが押すまで起きません |
| 5 | Morning Brief | **Overnight it organizes. Morning it briefs you.** ／ 夜のうちに整理して、朝に渡します |
| 6 | プライバシー図: *Text by default · Visual recall is opt-in, encrypted, auto-deleted · Audio never written to disk* の3行 | **We built it so we can't see your day.** ／ こちらから見られない作りにしてあります |
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
| 7 | 53-62 | 設定のプライバシー画面（Visual recall のトグルと保持期間が見えている状態） | `Text by default. Visual recall is yours to switch on. Audio never touches disk.` | 既定はテキスト。画像はあなたがONにしたときだけ。音声はディスクに触れない |
| 8 | 62-70 | ロゴ＋CTA | `ShogunAI — personal AGI for your work. On macOS today; Windows and mobile next.` | ShogunAI — 仕事のためのパーソナルAGI。今日は macOS 版 |

> 撮影ルール: ダミーアカウントで撮り、実在の人名・社名・本文を映さない。**倍速編集しない**（実機性能を疑われる）。ノッチの展開は等速で1回。

---

## 4. First comment（公開直後に投稿）

> **設計**: ①定義を先に差し出す ②**賭けの中身＝コンテキスト層**（モデルの知能は足りている／文脈が散らばっている） ③**領域の裏付け**（Garry Tan / Sam Altman。同じ層に別々の道から辿り着いている） ④**論点をずらす**——問いは「コンテキストが重要か」ではなく「それがどこに置かれるべきか」 ⑤我々の答え（あなたの手元） ⑥譲らない規律 ⑦個人向けから team / enterprise へ ⑧動作環境 ⑨**質問を4つ投げて議論を起こす**。
>
> **技術詳細は書かない。** accessibility 層・OCR・暗号化DB・3層メモリはここでは出さない。聞かれたら §7 の返信テンプレで答える（そのときは具体的に答える）。first comment は**主張の場所**であって、実装の説明の場所ではない。
>
> ⚠ **引用の扱い**: Tan / Altman とも**引用符を使わず paraphrase**にしてある。Tan の「leverage is in your context, not the model」は Startup School 2026 の要旨として複数媒体が報じているが、**一次ソース（YC公式動画）で逐語確認はできていない**。逐語で引くなら動画を確認してから。paraphrase のままなら安全。

### EN

```
Hi Product Hunt ⚔

A definition first, since the term is doing a lot of work in our tagline. By personal AGI I don't mean human-level intelligence. I mean general across your work rather than narrow to a single task — one agent that spans your whole day, holds the state of it, and acts on that state.

Here's the bet underneath it.

The models are already smart enough for most of what you do. What they're missing isn't IQ — it's you. Your context is scattered across a dozen tools, and the only thing holding it together is your own memory and your patience. You re-explain the project, paste the thread, remind it who this person is, and then do the last mile by hand anyway. The bottleneck moved, and most products haven't.

I don't think I'm alone in reading it that way. Garry Tan has spent this year telling founders that the leverage sits in your context rather than in the model — same weights, same window, wildly different output depending on what surrounds it. Sam Altman keeps describing where OpenAI is going in nearly the same terms: less chasing raw IQ, more something that understands your whole context and remembers it, with that memory as the durable advantage. When the person who sees the most startups and the person shipping the most-used model arrive at the same layer from opposite directions, the layer is real.

Which leaves the question that actually matters: where should that context live?

That's where we answer differently. ShogunAI keeps one state of your work — the people, the projects, the promises, the things still open — assembled from your own day and held inside your own machine rather than in someone else's account. Then it spends that state on finishing things. The reply arrives already drafted, knowing what you promised last month. A meeting ends with the next step instead of a transcript. The morning opens with what moved overnight and what you still owe people. You bring your own model, and you keep the memory either way.

One rule I won't trade away: reading is automatic, sending never is. Anything addressed to another person stops and waits for you.

Today it's built for one person — you. Team and enterprise plans are on the roadmap, because shared context is worth more than private context, and most of the work that gets stuck is stuck between people. The build that runs today is macOS; Windows and mobile are being built.

What I'd genuinely like from this thread:

Tell me where the argument breaks. Is context really your bottleneck, or is it something else entirely?

Tell me your version of the problem — the thing you find yourself re-explaining every week.

Tell me which integration would decide it. Name the tool that has to be connected before this is worth having, and it moves up the list.

And tell me honestly: would you leave something like this running for a full week? If not, what stops you?
```

### JA（日本語圏向け。X・note・日本語コメントへの返信で使う）

```
Product Hunt に ShogunAI を出しました ⚔

先に定義を置きます。タグラインで重い仕事をしている言葉なので。パーソナルAGIと言っても、人間並みの知能という意味ではありません。「全域」という意味です。単機能ではなく一日全体にまたがる一つのエージェントがあり、いま何が動いているかという状態を持ち、その状態から動きます。

その下にある賭けはこうです。

モデルの知能は、あなたが日々やっていることに対してはもう足りています。足りていないのはIQではなく、あなた自身の文脈です。文脈は十いくつものツールに散らばっていて、それを繋ぎ止めているのはあなたの記憶と根気だけです。プロジェクトを説明し直し、スレッドを貼り、この人が誰かをもう一度伝え、それでも最後の仕上げは手でやる。ボトルネックは移動したのに、プロダクトの多くはまだ動いていません。

こう読んでいるのは私だけではないはずです。Garry Tan は今年、レバレッジはモデルではなくコンテキストの側にあると創業者たちに言い続けています。同じ重み、同じウィンドウでも、周りに何を置くかで出てくるものがまるで変わる、と。Sam Altman が語る OpenAI の行き先もほぼ同じ言葉です。生のIQを追うことから離れ、あなたの文脈の全体を理解して記憶し、その記憶こそが持続的な優位になる、という方向。最も多くのスタートアップを見ている人と、最も使われているモデルを出している人が、反対側から同じ層に辿り着いている。その層は本物です。

そうなると、本当に効く問いはひとつ残ります。その文脈は、どこに置かれるべきなのか。

我々の答えはそこだけ違います。ShogunAI は、仕事の状態をひとつ持ちます。人、プロジェクト、交わした約束、まだ開いたままのもの。それをあなた自身の一日から組み立て、誰かのアカウントの中ではなく、あなたのマシンの中に置きます。そして、その状態を使って終わらせます。返信は、先月の約束を踏まえた下書きとして出てきます。会議のあとに残るのは議事録ではなく次の一手です。朝いちばんに目に入るのは、昨夜動いたものと、まだ返していないものです。モデルはあなたが選び、記憶はどちらにしてもあなたのものです。

譲らない規則がひとつあります。読み取りは自動、送信は自動になりません。人に宛てたものは必ず止まって、あなたを待ちます。

今は個人のためのプロダクトです。チーム版とエンタープライズ版も予定しています。共有された文脈は個人の文脈より価値が高く、止まっている仕事のほとんどは人と人の間で止まっているからです。今日動くビルドは macOS 版で、Windows とモバイルも作っています。

このスレッドで聞きたいことがあります。

この主張のどこが崩れると思いますか。あなたにとってのボトルネックは本当に文脈ですか。それとも別のものですか。

あなた自身の問題の形を教えてください。毎週のように説明し直している、あれのことです。

どの連携が決め手になりますか。これが繋がっていなければ持つ意味がない、というツールの名前を挙げてください。優先順位を上げます。

そして正直なところ、こういうものを1週間つけっぱなしにできますか。できないとしたら、何が引っかかりますか。
```

## 4.5 Connect with Investors（PHの投資家接続フォーム・非公開）

> **前提**: この欄は公開されない（"This information will never be shared publicly"）。よって LP 向けの「競合名を出さない」規律は適用しない。**YC 出願（Fall 2026）と数字・主張を一致させる**——投資家は両方を見る可能性があり、食い違いが一番痛い。各欄の上限は5000字だが、**読み切れる長さ（800〜1,600字）で止める**。埋めるための水増しはしない。
>
> ⚠ 数字は YC 出願の記載を転記した（有料ユーザー約10名・$50–60/月・waitlist約500・広告費ゼロ・収益の約8割はホテルチェーン案件）。**提出時点の実数に更新してから出すこと。**

### Q1. Why are you the right founder/team to work on this?

**EN**
```
I'm a solo technical founder and I write essentially all of the code myself, working AI-natively with Claude Code and Codex. Three weeks ago I won the Y Combinator Hackathon in Japan; ShogunAI's core product was live within weeks of that — continuous desktop context capture, a persistent world model of the user's work, and execution over MCP — with paying users on it daily.

Two things make me the right person for this specific problem. First, I am the user. I run my entire company — engineering, sales, recruiting, marketing — with AI at the center, so the cost of scattered context is something I pay personally every day. Every feature ships to me first; we run the company on ShogunAI. Second, the product demands an unusual combination: native macOS systems work, context modeling under real latency and privacy constraints, and consumer-grade product taste. I've spent months researching exactly this — desktop capture, MCP, local memory systems — before writing the first line.

I was also selected as the youngest founder in Shido, the accelerator run by Japan's Ministry of Economy, Trade and Industry. I'm not looking for a cofounder: I can build, market, and sell, and I'd rather selectively hire a few obsessive specialists than split the helm.
```

**JA**
> 私はソロの技術系ファウンダーで、コードは実質すべて自分で書いています（Claude Code と Codex を使った AI ネイティブな開発です）。3週間前に Y Combinator の日本ハッカソンで優勝し、その数週間後には ShogunAI のコアプロダクト——デスクトップ文脈の継続キャプチャ、仕事のワールドモデル、MCP 経由の実行——が動いていて、有料ユーザーが毎日使っています。
>
> この問題に対して自分が適任だと言える理由は2つあります。第一に、私自身がユーザーであること。開発・営業・採用・マーケティングのすべてを AI 中心で回しているので、文脈が散らばっていることのコストを毎日自分で払っています。新機能はまず自分に出荷し、会社自体を ShogunAI の上で運営しています。第二に、このプロダクトは珍しい組み合わせを要求すること。macOS ネイティブのシステム開発、レイテンシとプライバシー制約下での文脈モデリング、そしてコンシューマ級のプロダクト感覚。着手前の数か月、デスクトップキャプチャ・MCP・ローカルメモリをまさにこのために研究してきました。
>
> 経済産業省のアクセラレータ「Shido」には最年少ファウンダーとして採択されています。共同創業者は探していません。作ることも売ることも自分ででき、舵を分けるより、細部に執着する専門家を少数採用する方針です。

### Q2. Why did you pick this idea to work on?

**EN**
```
Two reasons — one structural, one personal.

Structurally, I believe the context layer is where value accrues in the AI era. Models are already smart enough for most knowledge work; what they lack is the state of your work, which today is scattered across ChatGPT, Claude, Cursor, Slack, Notion, Gmail, browsers, desktop apps, and face-to-face meetings. The direction of construction only works one way: if you own the context layer, you can rebuild communication, knowledge management, CRM, and task management on top of it as AI-native applications. If you start from any single application, you can never reconstruct the full context afterward. That asymmetry is why I think this layer produces a generational company.

Personally, it was my bottleneck. I work with AI at the center of everything, and the constraint was never model capability — it was that every AI started from zero and I re-explained the same background, decisions, and history across tools all day. Our principle is simple: live in the future, then build what's missing. This is the missing piece.

We validated it the direct way: we ran our own company on it, put it in the hands of about ten intensely active paying users, and one of them was convinced enough to quit his job and join us.
```

**JA**
> 理由は2つ。構造の話と、自分自身の話です。
>
> 構造的には、AI の時代に価値が溜まるのはコンテキスト層だと考えています。モデルの知能は大半の知的労働に対してもう足りていて、欠けているのは仕事の「状態」の方です。それは今、ChatGPT、Claude、Cursor、Slack、Notion、Gmail、ブラウザ、デスクトップアプリ、対面の会議に散らばっています。そして構築の方向は一方通行です。コンテキスト層を持っていれば、その上にコミュニケーション、ナレッジ管理、CRM、タスク管理を AI ネイティブに作り直せる。逆に、単一のアプリケーションから始めた場合、後から仕事の全文脈を再構成することはできません。この非対称性ゆえに、この層から世代を代表する会社が生まれると考えています。
>
> 個人的には、これが自分のボトルネックでした。すべての仕事を AI 中心で回している中で、制約はモデルの能力ではなく、どの AI もゼロから始まることでした。同じ背景、同じ決定、同じ経緯を一日中説明し直していた。私たちの原則はシンプルです——未来に住み、足りないものを作る。これが足りなかったものです。
>
> 検証は最短の方法でやりました。自分の会社をこの上で運営し、濃く使う有料ユーザー約10名に渡し、そのうちの一人は確信して会社を辞め、チームに加わりました。

### Q3. Who are your competitors, and what do you understand about this idea that they don't?

**EN**
```
Our competitors fall into four groups.

Memory and context tools — Goldfish, Screenpipe, Littlebird, Unabyss. They record context but cannot execute work. Execution tools and agent runtimes — Codex, Cursor, Dex, Aside, and self-hosted autonomous agents like Hermes (Nous Research). They execute, and the newest of them even persist and self-improve — Hermes writes a skill document every time it solves a hard problem. But what grows is procedural memory: how to do things. Their knowledge of *you* is limited to what passed through their own sessions, and your work doesn't happen inside an agent's sessions. AI-native collaboration products — PromptQL, Ando, Oasis. They aim at a "company brain," but only from conversations and documents that live inside their own platform. AI-native business applications — Monaco, Octolane, Origami. They rebuild CRM and GTM for the AI era, but each stays self-contained in its functional domain.

What we understand that they don't: the context that matters doesn't live inside any application — or any agent's session log. It exists across everything done on a computer — AI conversations, the browser, documents, email, meetings, desktop apps — and much of it evaporates before it's ever typed into Slack or a CRM. So we build the layer, not another app. A context layer can host every AI-native application on top of it; the reverse construction is impossible.

And on the "company brain": it isn't a product you build by collecting documents after the fact. Companies don't think — people do. It emerges when each individual's world model of their work connects under the right permissions. That's why we start with one person, and why the individual product isn't a wedge — it's the foundation.
```

**JA**
> 競合は4つのグループに分かれます。
>
> 記憶・文脈系（Goldfish, Screenpipe, Littlebird, Unabyss）。文脈を記録しますが、仕事を実行できません。実行系とエージェント・ランタイム（Codex, Cursor, Dex, Aside、そして Hermes〈Nous Research〉のような自己ホスト型の自律エージェント）。実行はでき、最新のものは持続と自己改善までします——Hermes は難問を解くたびにスキル文書を書き残します。ただし育つのは手続き記憶＝「どうやるか」です。彼らが「あなた」について知っているのは自分のセッションを通過した分だけで、あなたの仕事はエージェントのセッションの中では起きていません。AI ネイティブなコラボレーション系（PromptQL, Ando, Oasis）。「Company Brain」を狙っていますが、自社プラットフォーム内の会話と文書しか扱えません。AI ネイティブな業務アプリ系（Monaco, Octolane, Origami）。CRM や GTM を AI 時代に作り直していますが、それぞれ自分の職能領域に閉じています。
>
> 私たちが理解していて彼らが理解していないこと。価値のある文脈は、どのアプリケーションの中にも、どのエージェントのセッションログの中にも存在しません。それはコンピュータ上のすべての営み——AI との会話、ブラウザ、文書、メール、会議、デスクトップアプリ——にまたがって存在し、その多くは Slack や CRM に打ち込まれる前に蒸発します。だから私たちはアプリではなく層を作ります。コンテキスト層の上にはあらゆる AI ネイティブアプリを載せられますが、逆方向の構築は不可能です。
>
> そして「Company Brain」について。それは文書を後から集めて作るプロダクトではありません。会社は考えません。考えるのは人です。一人ひとりの仕事のワールドモデルが、適切な権限の下で接続されたときに、結果として立ち現れるものです。だから私たちは個人から始めます。個人向けプロダクトはくさびではなく、土台そのものです。
</br>

### Q4. What's your revenue and/or growth rate?

**EN**
```
Being precise, because the honest version is more interesting than the inflated one.

ShogunAI itself is weeks old. We won the YC Hackathon in Japan, started building immediately, and today have around 10 paying users at $50–60/month — all of them daily actives, deliberately capped while we iterate with them at high intensity. Nearly 500 people have joined the waitlist with zero ad spend, and we get 5–10 inbound DMs a day asking for access.

The company behind it (Select, Inc.) is revenue-generating: roughly $32K in June 2026, up from $4–6K/month earlier in the year. About 80% of that comes from a price-optimization project for a large hotel chain, with the remainder from product work and ShogunAI subscriptions. That services revenue funds the runway; it is not the business we're building.

The growth motion we're running: keep a small circle of intense paying users, convert the waitlist in waves as quality allows, and launch team plans once individual retention proves out. Our internal targets are aggressive — we're aiming for seven-figure net ARR within the next several months — and this launch is one of the levers.
```

**JA**
> 正確に書きます。盛った版より、正直な版の方が面白い数字なので。
>
> ShogunAI 自体はまだ生まれて数週間です。YC の日本ハッカソンで優勝して即座に作り始め、現在は月額 $50–60 の有料ユーザーが約10名。全員が毎日使っていて、高密度で一緒に改善するために人数は意図的に絞っています。ウェイトリストには広告費ゼロで500名弱が登録し、アクセスを求める DM が毎日5〜10件届きます。
>
> 運営会社（Select, Inc.）には売上があります。2026年6月で約 $32K、年初の月 $4–6K から伸びています。うち約8割は大手ホテルチェーンの価格最適化プロジェクトで、残りがプロダクト業務と ShogunAI のサブスクリプションです。この受託収益はランウェイの原資であって、私たちが作っている事業ではありません。
>
> いま回しているグロースの型はこうです。濃い有料ユーザーの小さな輪を保ち、品質が許す範囲でウェイトリストを段階的に転換し、個人のリテンションが証明でき次第チームプランを出す。社内目標は攻めていて、数か月以内にネット ARR 7桁（$1M台）を狙っています。このローンチはそのレバーの一つです。

### Q5. Anything else you would like investors to know?

**EN**
```
Three things.

The strongest signal we have isn't a metric: one of our earliest users was convinced enough to quit his job and join the team this August. That's what this product does to the people who run their day on it.

The roadmap is individual → team → enterprise, plus an infrastructure lane. Shared context is worth more than private context, and most stuck work is stuck between people — so team plans with permissions and a shared work model come next, then enterprise controls. Because execution is MCP-native, there's also a usage-based lane: other AI systems paying to read from ShogunAI as their persistent context layer.

On structure: Select, Inc. (Japan), founder holds 90%, one pre-seed investor (THESEED) holds 10%. We haven't been actively fundraising — this launch is the natural moment to start those conversations, and we'd rather meet investors who believe the context layer is the most structurally valuable position in the AI stack. Base is San Francisco with a deliberate Japan presence for hiring and brand — including building from a Kyoto machiya, because in a commoditizing market, being unmistakable is a strategy.
```

**JA**
> 3点あります。
>
> 私たちが持っている最強のシグナルは指標ではありません。最初期のユーザーの一人が確信して会社を辞め、この8月にチームに加わります。一日をこの上で回している人に、このプロダクトはそういう作用をします。
>
> ロードマップは、個人 → チーム → エンタープライズ、そしてインフラの車線です。共有された文脈は個人の文脈より価値が高く、止まっている仕事のほとんどは人と人の間で止まっています。だから次は権限と共有ワークモデルを備えたチームプラン、その先にエンタープライズ向けの統制です。実行が MCP ネイティブなので、従量の車線もあります。他の AI システムが ShogunAI を永続的なコンテキスト層として読むことに課金する形です。
>
> 体制について。Select, Inc.（日本法人）、ファウンダーが90%、プレシードの THESEED が10%を保有しています。これまで能動的な資金調達はしておらず、このローンチがその対話を始める自然なタイミングだと考えています。会いたいのは、コンテキスト層が AI スタックの中で構造的に最も価値のあるポジションだと信じる投資家です。拠点はサンフランシスコで、採用とブランドのために日本にも意図的な足場を置きます。京都の町家から作る、というのもその一つです。コモディティ化する市場では、見間違えようがないことが戦略になります。

---

## 5. 中盤に落とす Maker follow-up

投げっぱなしにしない。**一本ごとに違う層へ届ける**——(a) は作る人、(b) は使う人、(c) は疑う人。

**(a) 6時間後 — 作りの話**

> **EN**: A few of you asked how the default capture path works, so, precisely: we walk the accessibility tree of the focused window and keep text — bounded walk, near-duplicates collapsed into a dwell counter instead of new rows, password managers and private browsing excluded before anything is read, secure fields skipped at the subtree level, secrets redacted ahead of the write. The database is encrypted on device and forgets on a schedule: hot for a day, warm for a month, compressed after that. Forgetting isn't a cleanup job we got around to. It's the feature that makes keeping the rest defensible.
>
> **JA**: 既定の取得経路がどうなっているか、という質問が来たので正確に書きます。フォーカス中のウィンドウのアクセシビリティツリーを範囲を区切って辿り、テキストを取ります。ほぼ同じ内容は新しい行を作らず滞在時間として畳み、パスワード管理アプリとプライベートブラウジングは読む前に除外し、セキュアな入力欄はサブツリーごと飛ばし、秘匿値は書き込む前にマスクします。データベースは端末上で暗号化され、決まったリズムで忘れます。1日はホット、1か月はウォーム、その先は圧縮。忘却は後回しにしていた掃除ではありません。残す側を正当化できるのは、これがあるからです。

**(b) 10〜12時間後 — 使ってみた人の話**

> **EN**: The moment people describe back to me isn't recall — it's Monday morning. Overnight the week turns into state: who's waiting on you, what's gone stale, which promise was made out loud in a meeting and then landed nowhere. It's a short list, and every line points at the thing it came from. Nobody has ever asked me to make that list longer.
>
> **JA**: 使った人が話してくれるのは検索の話ではなく、月曜の朝の話です。夜のうちに一週間が状態に変わります。誰が自分の返事を待っているか、何が古くなったか、会議で口に出したのにどこにも着地しなかった約束はどれか。短いリストで、どの行も出どころを指しています。これを長くしてくれと言われたことは一度もありません。

**(c) 予備 — AGI 論争が伸びたときだけ**

> **EN**: I use "general" instead of "assistant" because the way assistants fail isn't stupidity, it's narrowness. A dictation app can't know who Mika is. A meeting recorder can't know what you promised her last month. Each is excellent inside its own hour and blind the moment it ends. The work worth doing isn't a smarter answer — it's one state of your work that every one of those moments reads from and writes back to.
>
> **JA**: 「アシスタント」ではなく「全域」と言うのは、アシスタントの失敗の仕方が知能不足ではなく視野の狭さだからです。音声入力アプリは、ミカが誰かを知りません。会議ツールは、先月あなたが何を約束したかを知りません。どれも自分の1時間の中では優秀で、その1時間が終わった瞬間に目が届かなくなる。やる価値があるのは賢い回答を作ることではなく、そのすべての瞬間が読み書きする一つの状態を作ることです。

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
> **EN**: The build that runs today is macOS — 14+, Apple Silicon. Windows and mobile are being built; I'm not going to guess at dates here. This was never meant as a Mac product: we started there because the accessibility layer let us read on-screen text reliably without recording the screen, which is what keeps the default path text-only. Sign up and tell me which machine you're on — that's the input I'm using to decide what ships next.
>
> **JA**: 今日動くビルドは macOS 版です（macOS 14 以上 / Apple Silicon）。Windows とモバイルは開発中で、時期はここでは言いません。もともとMac専用のつもりで作っていません。画面を録らずにテキストを確実に読める入口がそこにあったので、最初に出ただけです。既定の取得がテキストだけで済むのはそのためです。登録するときにどのマシンを使っているか教えてください。次に何を出すかの判断材料にします。

**Q5. 監視ツールでは？ 全部見られているのでは**
> **EN**: It's the first thing I'd ask too, and I'd rather be precise than reassuring. The default capture is text, not pixels — no video, no audio files. There's an opt-in visual recall for windows that yield no text: encrypted frames, on your machine, on-device OCR, deleted on the schedule you set, and off until you turn it on. Everything stays in an encrypted database on your machine; you can exclude any app or window title, with password managers and private browsing excluded by default; and export and delete-everything are buttons in settings, not a support form.
>
> **JA**: 私も最初に聞きます。安心させる言い方より、正確な言い方をします。既定で取るのはテキストで、画素ではありません。動画も音声ファイルも作りません。テキストを返さない画面のために Visual recall があり、これは自分でONにする機能です。暗号化したフレームを端末内に置き、オンデバイスのOCRで読み、あなたが決めた期間で自動的に消えます。既定はオフです。データはすべて端末上の暗号化データベースに留まり、どのアプリもウィンドウタイトルも除外でき、パスワード管理アプリとプライベートブラウジングは既定で除外。書き出しと全削除は設定のボタンで、問い合わせフォームではありません。

**Q5-b. Visual recall って結局スクショじゃないの**
> **EN**: It's frames, and I won't pretend otherwise — that's why it ships off and stays a switch you flip. The difference that matters is where they live and how long: encrypted on your machine, never uploaded, read by on-device OCR, and deleted automatically on the retention you pick. If you never turn it on, no image is ever stored.
>
> **JA**: 画像です。そこはごまかしません。だから既定はオフで、あなたが切り替えるものにしてあります。違うのは置き場所と期間です。端末内で暗号化したまま、アップロードはせず、オンデバイスのOCRで読み、あなたが選んだ保持期間で自動的に消えます。ONにしなければ、画像は一枚も保存されません。

**Q5-c. 音声入力の声はどこへ行く**
> **EN**: Live transcription runs through a cloud speech provider that is opted out of model training, and the audio is never written to disk or to the database — it exists as a stream while you're holding the key. Offline, it falls back to on-device transcription. What gets stored is the text.
>
> **JA**: ライブ文字起こしは、学習利用をオプトアウトしたクラウドの音声認識を通ります。音声はディスクにもデータベースにも書きません。キーを押している間だけ流れているものです。オフラインではオンデバイスの文字起こしに落ちます。保存されるのはテキストだけです。

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

**Q8-b. Hermes（自律エージェント系）と何が違うの？（名指しで来たら。実行系エージェント全般に流用可）**
> **EN**: Hermes is a genuinely good project and it's aimed at the same future — agents that persist, remember, and act. The line between us is which memory grows. Hermes learns from its own sessions: every task you hand it makes it better at doing. But your work doesn't happen inside an agent's sessions — the thread you read, the meeting you sat in, the decision you made on screen all happen outside it, so it still starts your day by asking. ShogunAI holds the state of the day itself — people, promises, open loops, built passively whether or not you talk to any agent — and serves it over MCP. So it's less either/or than layers: point Hermes at ShogunAI and it stops asking you questions. They bring the hands; we bring the world.
>
> **JA**: Hermes は本当に良いプロジェクトで、目指している未来——持続し、記憶し、実行するエージェント——は同じ側です。分かれ目は「どちらの記憶が育つか」です。Hermes は自分のセッションから学びます。タスクを渡すほど「やり方」は上手くなる。でも、あなたの仕事はエージェントのセッションの中では起きていません。読んだスレッドも、出た会議も、画面の上で下した判断も、全部その外側で起きます。だから一日の始まりには、やはりあなたに聞くところから始まる。ShogunAI が持つのは一日そのものの状態——人、約束、やりかけ——で、エージェントと話していようがいまいが受動的に育ち、MCP で開いています。つまり二者択一というより層の関係です。Hermes を ShogunAI に繋げば、質問される側からされない側に変わります。手足は彼らが、世界はこちらが持つ。

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
> Hold a key and talk — the text lands where your cursor was.
> Select a sentence, say what's wrong with it — it rewrites in place.
> Say nothing, and it still knows what you promised last month.
>
> One state of your work, inside your own PC. Nothing sends without your approval.
> ```
>
> **JA**
> ```
> ShogunAI を Product Hunt に出しました ⚔
>
> キーを長押しして話すと、カーソルのあった場所にテキストが着地する。
> 一文を選んで気に入らない点を言うと、その場で書き換わる。
> 何も言わなくても、先月の約束を覚えている。
>
> 仕事の状態がひとつ、あなたのPCの中に。送信は必ず承認を挟みます。
> ```

### X（中盤スレッド）

> **EN**: The most common question in the first three hours wasn't a feature request. It was "how do I know it isn't watching me?" Here's the actual answer, in the order it matters: [4ツイートで規律を分解 → 最後にPHリンク]
>
> **JA**: 最初の3時間で一番多かったのは機能の質問ではなく、「監視されていないとどう分かるのか」でした。順番に答えます。［4ツイートで分解 → 最後にPHリンク］

### LinkedIn

> **EN**: We opened early access for ShogunAI on Product Hunt today. We call it a personal AGI in a deliberately narrow sense: general across your work rather than narrow to a single task. It remembers your workday inside your own machine — text by default, with an opt-in visual recall for windows that yield none — keeps the state of it (people, projects, commitments, open loops, each with a source and a confidence), and turns that into finished work: drafted replies, meeting recaps that know the relationship, calendar holds, a morning brief. Anything addressed to another person stops for your approval first. The macOS build runs today; Windows and mobile are in the works.
>
> **JA**: 本日、ShogunAI のアーリーアクセスを Product Hunt で公開しました。パーソナルAGIと呼んでいますが、意味は狭く取っています。単機能ではなく、仕事の全域で動くという意味です。PCの中で一日を記憶し（既定で残すのはテキストです）、人・プロジェクト・約束・やりかけの状態を根拠と確度付きで持ち、そこから返信の下書き、関係の経緯を踏まえた会議recap、予定の確保、朝のブリーフまで進めます。人に宛てたものは必ず承認を挟みます。今日動くのは macOS 版で、Windows とモバイルも作っています。

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
- [ ] 綴りが全アセットで `AGI` に統一されているか（`AG` の略記が残っていないか。`PC` は維持。§2.1）
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
