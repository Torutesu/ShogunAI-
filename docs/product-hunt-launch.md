# Product Hunt ローンチ計画 — ShogunAI（アーリーアクセス／ウェイトリスト launch）

**Status**: v2（2026-08-20。v1のDMG直配布案から**ウェイトリストlaunchへ変更**。オーナー判断）
**用途**: PHローンチの意思決定・アセット原稿・当日運用の単一ソース。EN原稿はそのままコピペして使える形。
**準拠**: `shogun-brand` skill（トーン・色・NGワード）、`docs/positioning-category-messaging.md`（差別化の言い方）、`CLAUDE.md`（不変条件・プラン構成）。

---

## 0. 結論サマリ（先に読む3分）

| 項目 | 決定 |
|---|---|
| **launch形態** | **アーリーアクセス（ウェイトリスト）**。当日のCTAはメール登録1フィールド |
| **Tagline** | `Local-first memory for your Mac that finishes the work`（54字／上限60） |
| **一言の主張** | 記録して終わらない。**記憶から実行まで**行き、外に出るものは必ずあなたが承認する |
| **カテゴリ** | Productivity / Artificial Intelligence / Mac |
| **推奨日時** | **火曜 00:01 PT**（＝火曜 16:01 JST）。候補: 2026-09-15 / 予備 09-22 |
| **PH限定オファー** | ①**Wave 1（最初の招待枠）確約** ②**Founding価格ロック**（年額$49を12か月固定） ③Founding Discord |
| **waitlist launchで勝つ条件** | 「触れない」を埋めるのは**実機のデモ映像**と**招待の期日を数字で切ること**の2つだけ（§1.1） |
| **やらないこと** | 競合の名指し、"personal AGI"、ライフタイムディール、"AI-powered / revolutionary / second brain"、**モックだけの動画** |
| **最大のリスク** | プロダクトではなく **LPの記述**（架空のテスティモニアル、根拠のない "4h saved"、未取得のPHバッジ文字列、撤去済み紹介プログラムの残骸）。§10で必ず潰す |

---

## 1. ローンチ判断

### 1.0 決定: ウェイトリストで出す

アーリーアクセス登録のみで #1 を取るプロダクトは実際に多い。PHの票は「今日インストールできたか」ではなく**「これが世に必要だと思えたか」**で入るので、ウェイトリストであること自体は不利にならない。

代わりに、負担の置き場所が変わる:

| | DMG配布launch | **ウェイトリストlaunch（今回）** |
|---|---|---|
| 説得の主役 | プロダクト本体 | **デモ映像とストーリー** |
| 当日の失敗要因 | 初回起動でクラッシュ | **「いつ触れるの？」に答えられない** |
| 当日の勝ち筋 | 「もう動いた」の報告が並ぶ | **コメント欄でのメイカーの誠実さ** |
| ローンチ後の宿題 | 即リテンション | **招待を約束どおり出すこと** |

**この形態を選んだ利点を最大化する**: アプリ側のP0（オフラインクラッシュ、meeting overlayの再レンダ）は**当日のブロッカーではなくなる**。Wave 1 招待までの2〜3週間で潰せばよい。ローンチを時間で買った、という認識で運用する。

### 1.1 ウェイトリストlaunchで必ず要る2つ（これが無いと沈む）

1. **実機のデモ映像**。ウェイトリストで最も疑われるのは「実物があるのか」。**モックだけの動画は逆効果**で、PH勢は見抜く。§3.3の動画は実ビルドの実画面で撮る。ノッチの展開、ストリーミングで出てくる下書き、承認プレビュー——この3つは絶対に実写。
2. **招待の期日を数字で言い切ること**。「soon」は最悪の回答。**"First invites go out the week of Oct 6. PH signups are in wave 1."** のように週単位で明言し、当日のコメントでも繰り返す。守れる数字だけを出す（守れなければ約束の方を小さくする）。

### 1.2 ローンチ前の必須ブロッカー（ウェイトリスト版）

アプリの不具合ではなく、**登録動線と信頼**がブロッカーになる。

| 優先 | 項目 | 状態・根拠 |
|---|---|---|
| P0 | **実機デモ映像の撮影**（§3.3） | 未。ローンチ2週間前までに撮り切る。ダミーアカウントで実データを映さない |
| P0 | **LPの虚偽・未実証表記の除去** | §10。架空テスティモニアル／"4h saved"／未取得PHバッジ文字列 |
| P0 | **登録後の期待値設定**（サンクス画面＋自動返信メール1通） | 「いつ・何が届くか」「その間に何も課金されないこと」を明記。現状は `waitlist.okListed = "You're on the list."` の1行のみ（`apps/website/src/i18n/dictionaries.ts`） |
| P1 | **紹介プログラム残骸の整理** | APIは撤去済み（`/api/waitlist/{rank,status,profile,leaderboard,invite-context}` は404を返す）だが、辞書に `hero.invitedBy` / `invitedTier` が4言語分残っている。**紹介特典は当日訴求に使えない**ので、復活させないなら文言ごと消す |
| P1 | **参加者カウントの正直さ** | `/api/waitlist/count` はD1の実数、失敗時のみ `WAITLIST_IMPORTED_COUNT`（既定485）にフォールバック。当日はD1が生きていることを確認する（フォールバック値が出ていると水増しに見える） |
| P1 | **PHコホートの識別** | 現状 `waitlist_email_capture` は `email` と `created_at` のみで流入元を持たない（`apps/website/src/lib/waitlist-metrics.ts`）。**当日24時間のタイムスタンプでPHコホートを切る**運用なら改修ゼロ。属性で切りたいなら `source` カラム追加の小改修を前倒す |
| P2 | アプリのP0/P1（オフラインクラッシュ、meeting overlay、マルチディスプレイ） | `todo.md`。**Wave 1招待の前まで**に閉じる |

### 1.3 日時

- **火曜 00:01 PT**。PHの1日は 00:01 PT 始まり。火〜木が票の総量が多い。
- **00:01 PT = 16:01 JST（PDT期間）**。日本の夕方に打ち上げ → JPのゴールデンタイム（19-23時 JST）→ 23時JST以降に米国が起きて第2波。**24時間走る前提でシフトを組む**（§5）。
- 候補: **2026-09-15(火)** / 予備 **2026-09-22(火)**。§1.2 のP0が閉じなければ後ろ倒す。

---

## 2. PHでの言い方（ポジショニングの翻訳）

- 入口（1行目）: 「AIに毎回いきさつを説明するのが仕事になっている」——全員が持っている痛み。
- 中核: **記憶（memory）と実行（execution）が同じ一つのモデルを共有している**こと。機能の束ではない（`positioning §5` アンチバンドル）。
- 信頼: **録らない**（スクショも録画も音声ファイルも保存しない）、**ローカル**、**外に出るものは必ず承認（L3）**。実行の訴求には必ず承認をセットで書く。
- 抽象化の一句（競合名を出さずに位置を示す）:
  > The next layer after meeting recorders and lifeloggers.
- **ウェイトリスト固有の追加規律**: 未実装のものを実装済みのように書かない。ギャラリーとコピーで語るのは `docs/feature-status.csv` が implemented の範囲に限る。将来分は "Coming" と明示する（LPの `pricing.bundle` が既にこの作法になっている——ブラウザ／CRMは `soon: true`）。

### 使ってよい／だめな言葉

| 使う | 使わない |
|---|---|
| memory layer / execution / world model / local-first / approval / early access | AI-powered, revolutionary, game-changing, second brain |
| "on your Mac" / "nothing leaves without your approval" | "personal AGI"（PHではhype判定。デッキ限定） |
| macOS accessibility layer, MCP（**first commentとコメント返信のみ**） | tagline / description / ギャラリー字幕に技術名 |
| 絵文字なし（例外: **⚔** のみ） | 競合の名指し（相手が出したらカテゴリで返す。§6） |
| "invites start the week of ◯◯"（日付で言う） | "soon" / "very soon" / "in the coming weeks" |

---

## 3. 提出アセット一式

### 3.1 基本情報

**Name**
```
ShogunAI
```

**Tagline（上限60字）— 採用案**
```
Local-first memory for your Mac that finishes the work
```
予備案:
```
Private AI memory for your Mac that does the next step        (54)
The memory layer for your Mac that acts on what it sees       (55)
Your Mac remembers your work — and finishes it                (48)
```
> taglineに "early access" は入れない。availability は description と first comment で開示する（taglineの60字は製品の説明に全部使う）。

**Description（上限260字）— 採用案（259字・実測）**
```
ShogunAI turns your workday into one private memory on your Mac — screen text, meetings, mail, calendar — then acts on it: drafts, follow-ups, holds, a morning brief. No screenshots, no recordings, nothing sends without approval. Invites start in October.
```

**Topics**
```
Productivity  /  Artificial Intelligence  /  Mac
```

**CTA / Links**
- ボタン文言: `Get early access`
- Website: `https://syogun.com/?utm_source=producthunt&utm_medium=launch&utm_campaign=ph_launch`
- 登録フォームへのアンカー（ヒーロー直下）／Privacy & Security ページ／Pricing ページ

**Pricing欄**: 「Paid（from $49/mo, annual）· joining early access is free」。**ここを曖昧にしない。**あとから価格を知った人のコメントが一番荒れる。

---

### 3.2 ギャラリー（1270×760、7枚。この順）

ウェイトリストlaunchでは**ギャラリーがプロダクトの代役**。1枚目に文章を詰めない。

| # | 内容 | 画面内キャプション（EN） |
|---|---|---|
| 1 | ヒーロー: 黒(#080808)にノッチが開いた瞬間のMac上部クローズアップ。goldは1アクセント | **Your Mac already knows. Now it acts.** |
| 2 | **デモ動画**（§3.3）。2枚目に置くと再生率が高い | — |
| 3 | Recall: 自然文の問いに、根拠（provenance）付きで答えている実画面 | **Ask in plain language. Get answers with receipts.** |
| 4 | 実行: ノッチのボタン1つ → 下書き → **送信前の承認プレビュー** | **Nothing sends until you say send.** |
| 5 | Morning Brief: 昨夜動いたもの／今日開いているもの | **Overnight it organizes. Morning it briefs you.** |
| 6 | プライバシー対比図: *Text, on your Mac* vs *No screenshots. No recordings. No audio files.* | **We built it so we can't see your day.** |
| 7 | **ロードマップ＋招待の期日**（waitlist launch専用の1枚。ここが票を決める） | **Invites start the week of Oct 6. Product Hunt signups go first.** |

**7枚目の中身**（誠実さの提示。ここで差がつく）:
- Shipping now（implemented のみ）: passive memory / recall / meeting recap / morning brief / drafts & approvals / Gmail・Calendar・Drive
- Next: Slack → Notion・GitHub・Linear
- Later: browser / CRM
- 最下部に1行: `macOS 14+, Apple Silicon. Built by a small team in Tokyo.`

**Thumbnail（240×240, GIF可）**: ノッチが開いて閉じる2秒ループ。文字なし。Kamonを入れるなら余白は直径の1/6。

**画像の作り方**: **7枚のうち5枚以上を実画面**にする。埋めきれない箇所のみ `macos-mockup` skill のSVGで補い、モックには実データ風の作り込みをしない。

---

### 3.3 デモ動画スクリプト（60〜75秒・音声なし・字幕のみ・**実ビルド撮影**）

| # | 秒 | 画面 | 字幕 |
|---|---|---|---|
| 1 | 0-5 | 普通に仕事している画面（メール＋ドキュメント） | `You already did the work. Your AI just doesn't know about it.` |
| 2 | 5-13 | ノッチをクリック → 展開、文脈アクションが**もう並んでいる** | `Open the notch. The context is already there.` |
| 3 | 13-25 | 「Draft the follow-up」→ 相手の名前と前回の約束を織り込んだ下書きがストリーミング | `It knows who they are and what you owe them.` |
| 4 | 25-33 | 送信前のフルプレビュー＋承認 | `Nothing leaves your Mac without approval.` |
| 5 | 33-43 | 会議の自動検知 → 終了後のrecapとフォローアップ下書き | `Meetings end with the next step, not a transcript.` |
| 6 | 43-53 | 翌朝の Morning Brief | `It works overnight. You wake up briefed.` |
| 7 | 53-62 | 設定のプライバシー表記（保存しないもの一覧） | `No screenshots. No recordings. No audio files. Ever.` |
| 8 | 62-70 | ロゴ＋CTA | `ShogunAI — memory that acts. Early access invites start in October.` |

> 撮影ルール: ダミーアカウントで撮り、実在の人名・社名・本文を映さない。カーソルは大きめ。ノッチの展開は等速で1回だけ見せる。**倍速編集で速く見せない**（実機性能を疑われたら終わり）。

---

### 3.4 First comment（公開直後に投稿）— EN原稿

> **1段落目でウェイトリストであることを自分から言う。**あとから発覚するのが最悪。

```
Hi Product Hunt ⚔

Up front: this is early access, not a download link. The Mac app is built and running — everything in the demo is real footage, not a mockup — and we're opening invites in waves so we can fix what breaks for the first hundred people before the next hundred arrive. Product Hunt signups are in wave 1, and the first invites go out the week of Oct 6.

Now, why I built it.

Every time I open an AI tool, I have to explain my own week to it. Who this person is. What we agreed last Tuesday. What I already decided. Intelligence got cheap. Context didn't.

ShogunAI is a macOS app with two layers.

**Memory.** It quietly builds one memory of your workday — the text on your screen, your meetings, and the tools you connect (mail, calendar, docs, chat). It is not a screen recorder: no screenshots, no video, no audio files, ever. Capture runs through the macOS accessibility layer as text, and it stays in an encrypted database on your Mac.

**Execution.** That memory isn't a search box. ShogunAI keeps a live model of your work — people, projects, commitments, open loops — and every record carries where it came from and how confident it is. From the notch you get one button that finishes something: the follow-up drafted with the right history, the meeting recap that already knows the relationship, the calendar hold, the morning brief on what moved overnight.

The rule I won't break: **nothing leaves your Mac without you approving it.** Reads are automatic, sends never are. You see the full body before anything goes out, and every outbound call is traceable in the app.

You bring your own model — your own API key, or the Claude/ChatGPT/Gemini plan you already pay for. Your memory shouldn't be hostage to our margin on tokens.

**For Product Hunt:** sign up today and you're in wave 1, with the founding price locked ($49/mo annual) for your first year. No card, nothing charged to join.

What I'd genuinely like from this thread:
1. If you tried a memory tool before and dropped it — what made you quit? That's more useful to me than feature requests.
2. Which integration decides whether you'd leave it running for a week?
3. The privacy questions. Ask the uncomfortable ones here, on the record, and I'll answer them here.
```

**ハウスキーピング**: 絵文字は冒頭の **⚔** のみ／競合名ゼロ／日付（Oct 6の週）は守れる週に置換してから投稿。

### 3.5 中盤に落とす Maker follow-up

**(a) 6時間後 — 技術編（builder票）**
```
A few people asked how capture works without screenshots, so: we walk the accessibility tree of the focused window and keep text only — bounded walk, near-duplicate collapse, password managers and private browsing excluded by default, secure text fields skipped at the subtree level. Secrets are redacted before anything is written. The database is encrypted on device; hot for 24h, warm for 30 days, then compressed. Export and delete-everything are buttons in settings, not support tickets.
```

**(b) 10〜12時間後 — 招待運用の透明化（waitlist launch専用）**
```
On invites, so nobody has to guess: wave 1 goes out the week of Oct 6 to everyone who signed up during this launch, sized to a few hundred so we can actually answer every bug report. Wave 2 follows two weeks later. If we slip, I'll post it here rather than quietly moving the date.
```

---

## 4. PH限定オファー（ウェイトリスト版）

**値引きはしない。希少性と確約で払う。**

| # | オファー | 実装コスト |
|---|---|---|
| 1 | **Wave 1 招待の確約**（当日24時間に登録した人） | ゼロ。`waitlist_email_capture.created_at` の時間窓で切れる |
| 2 | **Founding価格ロック**: 年額$49を初年度固定（値上げしても据え置き） | Stripeの価格版管理のみ。**割引ではないので価格アンカーを壊さない** |
| 3 | **Founding Discord**: 要望が直接ロードマップに乗る導線 | ゼロ |

- **ライフタイムディールは絶対にやらない**（Batchレーン＝Select KKキーの継続コスト構造と噛み合わない）。
- 「先着◯名」を出すなら**本当に締める**。LPの `scarcity`（First 10,000 only）を当日訴求に使うなら、守れる数字に直してから使う。

---

## 5. 当日タイムテーブル（PT / JST・PDT基準）

| PT | JST | やること |
|---|---|---|
| 00:01 | 16:01 | 公開。**First commentを即投稿**（waitlist開示を自分から先に出す） |
| 00:05 | 16:05 | X / LinkedIn / Discord に同時告知（§7）。**「upvoteして」と書かない**（規約違反） |
| 00:30-02:00 | 16:30-18:00 | 全コメントに**15分以内**返信。初動2時間で立ち上がりが決まる |
| 02:00-05:00 | 18:00-21:00 | JPゴールデンタイム。JA投稿。日本語コメントには日本語で返す |
| 06:00 | 22:00 | follow-up (a) 技術編 |
| 08:00-11:00 | 00:00-03:00 | **米東海岸の朝＝票の本番**。返信が止まると失速。交代要員必須 |
| 10:00 | 02:00 | follow-up (b) 招待運用編 |
| 14:00 | 06:00 | 中間報告をXへ（順位ではなく**聞かれた質問**を共有すると伸びる） |
| 20:00 | 12:00 | 追い込み。未返信ゼロを確認 |
| 23:59 | 15:59 | 締め。翌日「初日に学んだこと」をXへ（順位に関係なく出す） |

**禁止**: 票の依頼（DM含む）、複数アカウント、競合の名指し批判、ネガティブコメントへの反論バトル。

---

## 6. コメント返信テンプレ（EN）

> 原則: ①懸念を認める ②構造で答える ③検証できるものへリンクする。

**Q1. ウェイトリストかよ / いつ触れるの**
```
Fair. Concretely: wave 1 goes out the week of Oct 6 to everyone who signs up during this launch, wave 2 two weeks later, and we're sizing waves to a few hundred so every bug report gets a human answer. If a date slips, I'll say so in this thread rather than move it quietly.
```

**Q2. 実物あるの？ デモはモックでは？**
```
Everything in the video is the real app on a real Mac — no mockups, no speed-up. The reason we're gating invites isn't that it doesn't run; it's that capture touches every app you use, and I'd rather fix the first hundred edge cases before inviting the next thousand.
```

**Q3. 監視ツールでは？ 全部見られているのでは**
```
It's the first thing I'd ask too. Three structural answers: capture is text only (no screenshots, no video, no audio files), it stays in an encrypted database on your Mac, and you can exclude any app or window title — password managers and private browsing are excluded by default. Full export and delete-everything are buttons in settings, not a support form.
```

**Q4. でもクラウドに送っているんでしょ**
```
Only what a request needs, and only when you ask. Reading is local. When a model call happens, the relevant chunk goes to the provider you chose, with your own key, and every outbound call is logged in the app. Sends to other people — mail, chat, calendar — are a separate category: those always stop for your explicit approval with the full body shown first.
```

**Q5. なぜmacOSだけ？**
```
Because capture quality is the product. The macOS accessibility layer lets us read on-screen text cheaply and reliably without recording anything — that's what makes the "no screenshots" promise possible. Apple Silicon, macOS 14+. Windows isn't a no, it's a not-until-we-can-do-it-at-this-quality.
```

**Q6. $49は高い**
```
Honest answer: it's priced for people whose hour is worth more than the subscription, and there's no free tier because it runs all day on real infrastructure. Joining early access costs nothing and nothing is charged at invite time — you'll see the full thing before you decide, and PH signups keep the founding price for their first year.
```

**Q7. ◯◯（競合名）とどう違うの**
```
I won't do a feature grid on someone else's product, but the categorical difference is this: recorders and lifeloggers end at "found it." ShogunAI treats memory as fuel — it keeps a live model of your work (people, commitments, open loops, each with a source and a confidence level) and the output is a finished draft or a scheduled hold, not a search result.
```

**Q8. メールを渡す理由がない**
```
Reasonable. It's one field, it's stored on its own, we don't sell or share it, and you'll get exactly two kinds of mail from it: your invite, and a short note if a date moves. Unsubscribe kills the record entirely — mail us and it's deleted, no dark pattern.
```

**Q9. オープンソースにする予定は？**
```
Not the app today. What's open in practice: your data is a local database you can export in full, and the memory is reachable over MCP, so you can point your own agents at it instead of being locked into our UI.
```

**Q10. 重くない？ バッテリーは？**
```
It's bounded on purpose: we walk the focused window only, collapse near-duplicates, and accumulate dwell instead of re-reading. The bar we hold ourselves to is under 5% CPU at idle averaged over a minute, and the panel opens in under 100ms. If your machine says otherwise, that's a bug report I want.
```

**Q11. Appleが同じことをやったら？**
```
Then a lot of people get a better OS, which is fine by me. The part that's hard to copy is the execution loop across your other tools with an approval model on top — cross-app, cross-vendor work Apple has historically not wanted to own.
```

**Q12. BYOKが面倒**
```
You don't need an API key if you already pay for Claude, ChatGPT, or Gemini — ShogunAI can run inference through the plan you already have, with your explicit opt-in. Keys are the fallback, and either way they live in the system Keychain, never in a config file.
```

**Q13. 会議の音声はどこへ行く？（ごまかさない）**
```
Live transcription for meetings runs through a cloud speech provider, opted out of any model training, and we never write audio to disk — not even a temp file. What gets stored is the text and where it came from. It's disclosed in the app before you turn meetings on, and you can leave meeting capture off entirely.
```

**Q14. 日本語コメント**
```
日本語でも問題なく動きます。画面テキストの取得も会議の文字起こしも日英どちらも扱えて、UIは今のところ英語です（日本語UIは対応予定）。招待は10月第1週から順次で、今日登録された方は最初の枠に入ります。
```

**Q15. ネガティブ/辛辣なコメント**
```
That's a fair hit and I'm not going to argue it. [認める点を1文] Here's what I'll do about it: [期限つきで1文]. Ping me when it lands and tell me if it actually fixed your case.
```

---

## 7. 外部拡散の原稿

> **共通**: どこにも「upvoteして」と書かない。「出した／ここにいる／聞いてくれ」で通す。

### X（EN）
```
ShogunAI is live on Product Hunt ⚔

A macOS app that keeps one private memory of your workday — no screenshots, no recordings — and then actually finishes things: the follow-up, the recap, the morning brief. Nothing leaves your Mac without your approval.

Early access opens in waves. Signups today are in wave 1 → [link]
```

### X（JA）
```
ShogunAI を Product Hunt に出しました ⚔

Macの中だけで一日の仕事を記憶して、そこから実行まで行くアプリです。スクショも録画も保存しません。会議のあとに残るのは議事録ではなく、次の一手の下書きです。外に出るものは必ず承認を挟みます。

招待は順次。今日登録した方は最初の枠に入ります → [link]
```

### X（中盤スレッド）
```
The most common question in the first 3 hours wasn't a feature request. It was "how do I know it isn't watching me?"

Here's the actual answer, in the order it matters: [4ツイートで技術規律を分解 → 最後にPHリンク]
```

### LinkedIn
```
We opened early access for ShogunAI on Product Hunt today.

Most AI tools ask you to re-explain your own week before they can help. ShogunAI removes that step: it keeps one private memory of your workday on your Mac — text only, never screenshots or recordings — and turns it into finished work: drafted follow-ups, meeting recaps that know the relationship, a morning brief on what moved overnight. Anything that leaves the machine stops for your approval first.

macOS 14+, Apple Silicon. Invites roll out in waves starting in October: [link]
```

### Discord / Slack
```
We're live on PH today ⚔ — ShogunAI, local-first work memory for Mac that drafts and executes instead of just recalling. Early access opens in waves; I'm in the comments all day if you want to grill me on the privacy model: [link]
```

### Reddit（r/macapps 等。サブごとの規約を必ず確認）
```
Title: I built a Mac app that remembers my workday as text (no screenshots) and drafts the follow-ups

Body: 「なぜ作ったか」「何を保存しないか」「何がまだできないか」「招待の時期」を数段落。PHリンクは末尾に1回。機能列挙はしない。
```

### Hacker News（Show HN。ウェイトリストは嫌われやすいので、事実で押す）
```
Show HN: ShogunAI – Local-first work memory for macOS that drafts and executes

本文: 技術的事実のみ。accessibility treeのバウンド走査、暗号化ローカルDB、Hot/Warm/Cold、provenance＋confidence、外部送信の承認モデル、BYOK、そして「なぜ今はウェイトリストなのか」を1段落で率直に。マーケ語ゼロ。
```

---

## 8. ハンター／動員

- **セルフハントでよい。**メイカーが終日コメント欄にいることの方が効く。
- 事前にやってよい: 予告投稿、Discord/メールへの当日アナウンス、PHでの自分のフォロワー獲得（ローンチ時に通知が飛ぶ）。
- **やってはいけない**: 票の直接依頼、インセンティブ付き投票、投票用アカウントの誘導。1件でも露見すると当日ランキングから外れる。
- 事前に確保: **実際に触った10〜20人**に「当日、正直な感想をコメントで書いてほしい」と依頼する。**票ではなくコメントを頼む**——規約内で、かつウェイトリストlaunchでは「実物がある」の最強の証拠になる。

---

## 9. KPIと事後の動線

| 指標 | 目標 |
|---|---|
| PH順位 | Top 5 of the Day（Top 3が取れたら上出来） |
| PHコメント数 | 60+（メイカー返信を除く） |
| LPセッション（当日） | 3,000〜6,000 |
| **LP→ウェイトリスト登録CVR** | **25%以上**（ウェイトリストlaunchの本KPI。下回るならヒーローかフォーム位置の問題） |
| 当日の新規登録数 | 800〜1,500 |
| Wave 1 招待 → 起動＋権限許可完了 | **50%以上**（ここが本当の勝敗。順位ではない） |
| 招待から7日後もキャプチャが動いている | 30%以上 |

**ローンチ後の動線**
- D+1: 「初日に聞かれた質問トップ5と答え」をブログ＋Xへ。PHページにも追記コメント。
- D+3: 登録者への1通目（プロダクトの中身を1つだけ深掘り。売り込まない）。
- D+7: 進捗ノート1通（招待日の再確認。**遅れるなら遅れると書く**）。
- **Wave 1（10月第1週想定）**: PHコホートに招待。この時点でアプリのP0が閉じていること（`todo.md`）。
- Wave 1 の翌週: 招待者の実データでアクティベーション率を測り、Wave 2 のサイズを決める。
- バッジ（Top◯）は**取れた場合のみ**LPに貼る（§10-3）。

---

## 10. LP側の必須修正（ローンチ前に必ず）

ウェイトリストlaunchでは**LPが唯一の実体**。粗があると票がそのまま逃げる。

1. **架空のテスティモニアル** — `apps/website/src/i18n/dictionaries.ts` の `testimonials.items` に実在しない人名・肩書き（`alex_builds` / `Maria Kowalski` / `Kenji Tanaka` 等）が入り、`Testimonials.tsx` がそのまま表示している。突かれた瞬間にプライバシー主張の信頼まで落ちる。**実名許諾コメントに差し替えるか、セクションごと非表示**。二択。
2. **`stats` の "4h saved per week, on average"** — 根拠のない平均値。実測がないなら落として、検証可能な事実（スクショを1枚も保存しない／ノッチ展開100ms 等の自社SLO）に差し替える。
3. **PHバッジ文字列** — 辞書に `badges.productHunt = '#1 Product of the Day'` が定義されている（現在レンダリングされるのは `authority` の "Coming soon on Product Hunt" のみ）。未取得実績の文字列がコードに残っているのは事故の元。削除し、当日以降は実際に取れた順位の公式バッジのみ貼る。
4. **紹介プログラムの残骸** — API群は404化済み（§1.2）だが辞書に `hero.invitedBy` / `invitedTier` が4言語分残存。復活させないなら文言ごと削除する（「紹介で特典」を期待させたまま動かないのが最悪）。
5. **ウェイトリスト動線の強化**（今回の本丸）
   - ヒーロー内にメール1フィールドを**スクロールなしで**置く（現状 `Hero.tsx` にフォームあり。PH流入で最初に目に入る位置か実機確認）
   - フォーム直下に **「いつ招待が来るか」の1行**（例: `Invites start the week of Oct 6 · macOS 14+, Apple Silicon`）
   - 送信後のメッセージを `waitlist.okListed` の "You're on the list." から、**次に何が起きるかを書いた2〜3行**へ差し替え
   - 自動返信メール1通（招待時期／課金されないこと／解除方法）
   - `?utm_source=producthunt` のときヒーロー上に細いバー: `Product Hunt: you're in wave 1.`
6. **Privacy & Security ページ**を §6 Q3/Q4/Q13 の粒度に整える（コメントから直リンクして即答するため）。
7. **参加者カウント** — 当日D1が生きていることを確認（フォールバックの既定値が出ていると水増しに見える）。

---

## 11. 出す前の最終チェック

- [ ] first comment の**1段落目**でウェイトリストであることを開示しているか
- [ ] 招待の期日が**週単位の具体日**で書かれ、守れる日付になっているか
- [ ] デモ動画が**実ビルドの実画面**か（倍速・モック混入なし）
- [ ] ギャラリーで語っている機能が `docs/feature-status.csv` の implemented 範囲内か（将来分は "Coming" 明記）
- [ ] tagline / description / ギャラリー字幕に技術スタック名が出ていないか
- [ ] 絵文字は ⚔ のみか
- [ ] "AI-powered" / "revolutionary" / "second brain" / "personal AGI" 不使用か
- [ ] 競合の固有名がこちらの原稿に一切ないか
- [ ] 実行の訴求すべてに**承認（approval）**が併記されているか
- [ ] gold の面積が5%以下か（全ギャラリー画像）／Kamonの余白1/6
- [ ] 日本語版と英語版の主張が一致しているか（翻訳ではなくネイティブ表現）
- [ ] §10 の1〜4（架空テスティモニアル・未実証数値・未取得バッジ・紹介残骸）が消えているか
- [ ] 登録後のサンクス文言と自動返信メールが動くか（本番で実登録してテスト）
