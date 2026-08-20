# Product Hunt ローンチ計画 — ShogunAI

**Status**: v1 draft（2026-08-20）
**用途**: PHローンチの意思決定・アセット原稿・当日運用の単一ソース。EN原稿はそのままコピペして使える形にしてある。
**準拠**: `shogun-brand` skill（トーン・色・NGワード）、`docs/positioning-category-messaging.md`（差別化の言い方）、`CLAUDE.md`（不変条件・プラン構成）。

---

## 0. 結論サマリ（先に読む3分）

| 項目 | 決定 |
|---|---|
| **出す物** | macOS アプリ本体（Developer ID 署名＋公証済み DMG を LP から直配布）。ウェイトリストではなく「今すぐ落として動く」状態で出す |
| **Tagline** | `Local-first memory for your Mac that finishes the work`（54字／上限60） |
| **一言の主張** | 記録して終わらない。**記憶から実行まで**行き、外に出るものは必ずあなたが承認する |
| **PHでのカテゴリ** | Productivity / Artificial Intelligence / Mac |
| **推奨日時** | **火曜 00:01 PT**（＝火曜 16:01 JST）。候補: 2026-09-15 または 09-22 |
| **PH限定オファー** | **30日フルトライアル（通常7日）＋カード不要**。値引きはしない（理由 §4） |
| **やらないこと** | 競合の名指し、"personal AGI" の使用、ライフタイムディール、"AI-powered / revolutionary / second brain" |
| **最大のリスク** | プロダクトではなく **LPの記述**（架空のテスティモニアル、根拠のない "4h saved"、PHバッジのプレースホルダ）。§10 で必ず潰す |

---

## 1. ローンチ判断

### 1.1 なぜ「ウェイトリスト launch」にしないか

PHの評価軸は「今日試せるか」。ShogunAI は署名・公証込みの配布経路が `docs/release-signing-and-distribution.md` で確立済みで、`docs/feature-status.csv` 上もキャプチャ／メモリ／state／Fusion／L1-L3／Dream Cycle／Morning Brief／会議ノート／Wave-1連携が implemented になっている。ここでウェイトリスト出しをすると「まだ動かないやつ」枠に落ちて、二度目のローンチのカードも失う。**DMGを置いて出す。**

### 1.2 ローンチ前の必須ブロッカー（`todo.md` 由来。全部潰してから日付を確定する）

| 優先 | 項目 | なぜPHローンチのブロッカーか |
|---|---|---|
| P0 | **オフライン時のクラッシュ**（`todo.md`「Offline crash」） | 初回起動が空港/カフェ/社内プロキシ下で落ちると、その1件がPHコメント欄に貼られて当日の空気が決まる |
| P0 | **Meeting overlay の 60fps 再レンダ**（`todo.md` P0） | PH流入は「とりあえず会議で試す」層。初回体験でファンが回ったら「重い」がレビューの見出しになる |
| P0 | **初回オンボーディングの権限導線**（Accessibility 権限の許可 → 再起動）| macOSの権限ダイアログはPH最大の離脱点。**権限を1つ許可するたびに何が動き出すか**を画面で説明する |
| P1 | **マルチディスプレイ/Spaces でのノッチ追従**（`todo.md` P1） | 外部モニタ勢が多い。擬似ノッチのフォールバックが効いているかを実機で確認 |
| P1 | **DMGのGatekeeper実機確認** | 「壊れているため開けません」が出た瞬間に終わる。別のクリーンなMacでダウンロード→初回起動まで通す |
| P1 | **Stripe: 30日トライアルのクーポン/価格** | オファーが機能しないローンチは信用を落とす。当日朝にテスト購入で確認 |

### 1.3 日時

- **火曜 00:01 PT**。PHの1日は 00:01 PT 始まり。火〜木が票の総量が多く、月曜は大型が来やすい、金土日は薄い。
- **日本チームにとっての利点**: 00:01 PT ＝ **16:01 JST（PDT期間）**。日本の夕方に打ち上げて、その日の日本のゴールデンタイム（19-23時 JST）でJPコミュニティを拾い、米国が起きてくる 23時JST 以降に第2波が来る。**ローンチ当日は24時間走る前提でシフトを組む**（§5）。
- 候補日: **2026-09-15(火)** / 予備 **2026-09-22(火)**。§1.2 のP0が2週間で閉じなければ迷わず後ろ倒す。

---

## 2. PHでの言い方（ポジショニングの翻訳）

PHの読者は「新しいカテゴリの説明」を読まない。**既知の不満に接続してから、構造の話をする。**

- 入口（1行目）: 「AIに毎回いきさつを説明するのが仕事になっている」——これは全員が持っている痛み。
- 中核: **記憶（memory）と実行（execution）が同じ一つのモデルを共有している**こと。機能の束ではない（`positioning §5` アンチバンドル）。
- 信頼: **録らない**（スクリーンショットも録画も保存しない）、**ローカル**、**外に出るものは必ず承認（L3）**。実行系の訴求には必ず承認をセットで書く（`positioning §8` チェックリスト）。
- 抽象化の一句（競合名を出さずに位置を示す）:
  > The next layer after meeting recorders and lifeloggers.

### 使ってよい／だめな言葉（PH面）

| 使う | 使わない |
|---|---|
| memory layer / execution / world model / local-first / approval | AI-powered, revolutionary, game-changing, second brain |
| "on your Mac" / "nothing leaves without your approval" | "personal AGI"（PHではhype判定される。デッキ限定） |
| macOS accessibility layer, MCP（**first comment とコメント返信のみ**。技術的信頼の担保に必要） | tagline / description / gallery キャプチャに技術名を入れる |
| 絵文字なし（例外: ローンチ告知の **⚔** のみ） | 競合の名指し（相手が名前を出したらカテゴリで返す。§6） |

---

## 3. 提出アセット一式

### 3.1 基本情報（PHの入力欄にそのまま入る形）

**Name**
```
ShogunAI
```

**Tagline（上限60字）— 採用案**
```
Local-first memory for your Mac that finishes the work
```
予備案（A/Bで迷ったらこの順）:
```
Private AI memory for your Mac that does the next step        (54)
The memory layer for your Mac that acts on what it sees       (55)
Your Mac remembers your work — and finishes it                (48)
```
> 選定理由: PHのtaglineは気の利いた比喩ではなく**何であるか**を通す場所。`Local-first`＝差別化、`for your Mac`＝対象の限定を先に開示（macOS限定の不満を先回りで潰す）、`finishes the work`＝実行レイヤー。

**Description（上限260字）— 採用案（242字）**
```
ShogunAI turns your workday into one private memory on your Mac — screen text, meetings, mail, calendar — then acts on it: drafts, follow-ups, holds, a morning brief. No screenshots, no recordings, and nothing sends without your approval.
```

**Topics**（3つまで実効。優先順）
```
Productivity  /  Artificial Intelligence  /  Mac
```
（候補: Privacy, Note taking, Meetings, Developer Tools。Privacyは票が薄いので本命3つを優先）

**Links**
- Website: `https://syogun.com/?ref=producthunt`（UTM: `?utm_source=producthunt&utm_medium=launch&utm_campaign=ph_launch`）
- Direct download: LPのダウンロード導線へアンカー（PH流入をヒーロー直下のDLボタンに落とす）
- Pricing / Privacy & Security ページ（プライバシー質問はここへリンクして即答する）

**Pricing表記**: `Free trial` を選択 →「30-day full trial for Product Hunt, no card required. Then $49/mo (annual) or $62/mo.」

---

### 3.2 ギャラリー（1270×760、7枚。この順で並べる）

PHは**画像1枚目とタイトルで9割決まる**。1枚目に文章を詰めない。

| # | 内容 | 画面内キャプション（EN・短く） |
|---|---|---|
| 1 | ヒーロー: 黒背景(#080808)にノッチが開いた瞬間のMac上部クローズアップ。goldは1アクセントのみ | **Your Mac already knows. Now it acts.** |
| 2 | **デモ動画/GIF**（§3.3）。PHは2枚目に動画を置くと再生率が高い | — |
| 3 | Recall: 自然文の問いに、根拠（provenance）付きで答えている画面 | **Ask in plain language. Get answers with receipts.** |
| 4 | 実行: ノッチのボタン1つ → 下書きが立ち上がり、**送信前の承認プレビュー**が出ている | **Nothing sends until you say send.** |
| 5 | Morning Brief: 朝の1画面（昨夜動いたもの／今日開いているもの） | **Overnight it organizes. Morning it briefs you.** |
| 6 | プライバシー: 「保存するもの／しないもの」の対比図。*Text, on your Mac* vs *No screenshots. No recordings. No audio files.* | **We built it so we can't see your day.** |
| 7 | 連携グリッド＋オファー | **Works where the work is. 30-day full trial.** |

**Thumbnail（240×240, GIF可）**: ノッチが開いて閉じる 2秒ループ。文字は入れない。Kamonは入れるなら中央、余白は直径の1/6（ブランド規約）。

**画像の作り方**: 実スクショが無い箇所は `macos-mockup` skill のSVG手法で作る。ただし**7枚のうち最低4枚は実画面**にする。全部モックだとPH勢には見抜かれる。

---

### 3.3 デモ動画スクリプト（60〜75秒・音声なし・字幕のみ）

無音で成立させる（PHは音を切って見る）。1カット5〜8秒、テンポ最優先。

| # | 秒 | 画面 | 字幕 |
|---|---|---|---|
| 1 | 0-5 | 普通に仕事している画面（メール＋ドキュメント） | `You already did the work. Your AI just doesn't know about it.` |
| 2 | 5-13 | ノッチをクリック → 100ms で展開、文脈アクションが**もう並んでいる** | `Open the notch. The context is already there.` |
| 3 | 13-25 | 「Draft the follow-up」を押す → 相手の名前・前回の約束を織り込んだ下書きがストリーミングで出る | `It knows who they are and what you owe them.` |
| 4 | 25-33 | 送信前の**フルプレビュー＋承認**。承認しないと何も出ない | `Nothing leaves your Mac without approval.` |
| 5 | 33-43 | 会議が始まる → 自動検知 → 終了後にrecapとフォローアップ下書き | `Meetings end with the next step, not a transcript.` |
| 6 | 43-53 | 翌朝の Morning Brief | `It works overnight. You wake up briefed.` |
| 7 | 53-62 | 設定画面のプライバシー表記（保存されないもの一覧） | `No screenshots. No recordings. No audio files. Ever.` |
| 8 | 62-70 | ロゴ＋CTA | `ShogunAI — memory that acts. 30-day full trial.` |

> 撮影ルール: 実データはダミーアカウントで作る（`CLAUDE.md` テレメトリ規約と同じ精神で、実在の人名・社名・メール本文を映さない）。カーソルは大きめ、ノッチの展開は**等速で1回だけ**見せる（繰り返すと遅く見える）。

---

### 3.4 First comment（ローンチ直後に投稿する Maker's comment）— EN原稿

> そのまま貼れる。長さはPHの最適域（250〜400語）。段落は3〜5行で切る。

```
Hi Product Hunt ⚔

I built ShogunAI because of one stupid daily ritual: every time I open an AI tool, I have to explain my own week to it. Who this person is. What we agreed last Tuesday. What I already decided. The intelligence is free now — the context isn't.

ShogunAI is a macOS app with two layers.

**Memory.** It quietly builds one memory of your workday — the text on your screen, your meetings, and the tools you connect (mail, calendar, docs, chat). It is not a screen recorder: there are no screenshots, no video, and no audio files, ever. Capture runs through the macOS accessibility layer as text, and it stays on your Mac in an encrypted local database.

**Execution.** That memory is not a search box. ShogunAI keeps a live model of your work — people, projects, commitments, open loops — and every record carries where it came from and how confident it is. From the notch you get one button that finishes something: the follow-up drafted with the right history, the meeting recap that already knows the relationship, the calendar hold, the morning brief that tells you what moved overnight.

The rule I refuse to break: **nothing leaves your Mac without you approving it.** Reads are automatic, sends never are. You see the full body before anything goes out, and every outbound call is traceable in the app.

You bring your own model — your Anthropic/OpenAI key, or the Claude/ChatGPT/Gemini plan you're already paying for. We don't want to be your model reseller, and your memory shouldn't be hostage to ours.

**For Product Hunt: 30-day full trial, no card.** [link]

What I'd love your help on:
1. If you tried memory tools before and dropped them — what made you quit? I care more about that than about feature requests.
2. Which integration is missing for you to actually leave it on for a week?
3. Anyone on a notch-less Mac or heavy multi-monitor setup — tell me how the panel behaves. That's where I have the least data.

I'm here all day. Ask me anything, including the uncomfortable privacy questions — those are the ones I want on the record.
```

**ハウスキーピング**
- `[link]` は 30日トライアルの導線に置換。
- 絵文字は冒頭の **⚔** 1つのみ（ブランド例外）。他は入れない。
- 競合名はゼロ。`positioning §3.5` の「記録ツールのゴールは見つかった、SHOGUNはそこから始まる」を、名指しなしで body に織り込んである。

### 3.5 中盤に落とす Maker follow-up（票が伸びる時間帯に1〜2本）

**(a) 6時間後 — 技術的な深掘り（builder票を拾う）**
```
A few people asked how capture works without screenshots, so: ShogunAI walks the accessibility tree of the focused window and keeps text only — bounded walk, near-duplicate collapse, password managers and private browsing excluded by default, secure text fields skipped at the subtree level. Secrets get redacted before anything is written. The DB is encrypted on device; Hot 24h in RAM, Warm 30 days, Cold compressed. Deleting is a first-class action, not a support ticket.
```

**(b) 10〜12時間後 — 使い方の実例（滞在時間を伸ばす）**
```
The clearest "aha" from early users isn't recall — it's Monday morning. Overnight the app reprocesses the day into state: who you owe something to, what's gone stale, what got promised in a meeting and never landed anywhere. The brief is three lines long and every line links back to the evidence it came from. That's the part people say they can't unsee.
```

---

## 4. PH限定オファー

**採用: 30日フルトライアル（通常7日）、カード不要。値引きはしない。**

理由:
1. ShogunAIの価値は**溜まってから**出る。7日ではワールドモデルが育ちきらず、PH流入の大半が「よく分からないまま終了」になる。トライアル延長は**プロダクトの構造に対する正しいオファー**であり、値引きではない。
2. 年額$49の価格アンカーをローンチ初日に自分で割ると、以後の正価が「割引待ち」になる。
3. **ライフタイムディールは絶対にやらない。** クラウドのBatchレーン（Select KKキー）に継続コストがかかる構造なので、LTDは将来の首を絞める。

実装: Stripe の trial_period_days を 30 にした PH 専用チェックアウト（またはクーポン `PH30D`）。**当日朝にテスト購入で必ず検証する。**

サブ特典（任意・コストゼロ）: PH経由の最初の200人を「Founding」タグでDiscordに入れ、機能要望のルーティングを優先する。値引きより効くし、原価がかからない。

---

## 5. 当日タイムテーブル（PT / JST 併記・PDT基準）

| PT | JST | やること |
|---|---|---|
| 00:01 | 16:01 | 公開。**First comment を即投稿**（先に投稿しないと質問が野放しになる） |
| 00:05 | 16:05 | X / LinkedIn / Discord / 社内向けに同時告知（§7の原稿）。**「upvote してください」と書かない**（PHガイドライン違反） |
| 00:30-02:00 | 16:30-18:00 | 全コメントに**15分以内**返信を維持。この初動2時間がランキングの立ち上がり |
| 02:00-05:00 | 18:00-21:00 | JPゴールデンタイム。JA向け投稿（§7）。日本語コメントには日本語で返す |
| 06:00 | 22:00 | Maker follow-up (a) 技術編を投下 |
| 08:00-11:00 | 00:00-03:00 | **米国東海岸の朝＝票の本番**。ここで返信が止まると失速する。交代要員を必ず置く |
| 10:00 | 02:00 | Maker follow-up (b) 使い方編 |
| 14:00 | 06:00 | 中間報告をXに（順位ではなく**質問された内容**を共有すると伸びる） |
| 20:00 | 12:00 | 最後の追い込み。未返信ゼロを確認 |
| 23:59 | 15:59 | 締め。翌日に「学んだこと」スレッドをXに出す（順位に関係なく出す） |

**当日の禁止事項**: 票の依頼（DM含む）、複数アカウント、コメント欄での競合の名指し批判、ネガティブコメントへの反論バトル。PHは運営が見ている。

---

## 6. コメント返信テンプレ（EN。想定質問別）

> 原則: ①相手の懸念を認める ②構造で答える（機能自慢にしない） ③検証できるものへリンクする。

**Q1. 監視ツールでは？ 全部見られているのでは**
```
Fair concern — it's the first thing I'd ask. Three structural answers: capture is text only (no screenshots, no video, no audio files), it stays in an encrypted database on your Mac, and you can exclude any app or window title — password managers and private browsing are excluded by default. There's a full export and a delete-everything button in settings, not a support form.
```

**Q2. でもクラウドに送っているんでしょ？**
```
Only what a request needs, and only when you ask for it. Reading is local. When a model call happens, the relevant chunk goes to the provider you chose with your own key — and every outbound call is logged in the app so you can see what left and when. Sends to other people (mail, chat, calendar) are a separate category: those always stop for your explicit approval, with the full body shown first.
```

**Q3. なぜmacOSだけ？ Windows/Linuxは？**
```
Because the capture quality is the product. The macOS accessibility layer lets us read on-screen text cheaply and reliably without recording anything — that's what makes the "no screenshots" promise possible. Doing it badly on three platforms would have been the wrong trade. Apple Silicon, macOS 14+. Windows isn't a no, it's a not-until-we-can-do-it-at-this-quality.
```

**Q4. $49は高い**
```
Honest answer: it's priced for people whose hour is worth more than the subscription, and there's no free tier because the thing runs all day on real infrastructure. The 30-day trial is full-featured and needs no card — if a month of it doesn't visibly save you time, it isn't for you and I'd rather you say so.
```

**Q5. ◯◯（競合名）とどう違うの？**
```
I won't do a feature grid on someone else's product, but the categorical difference is this: recorders and lifeloggers end at "found it." ShogunAI treats memory as the fuel — it keeps a live model of your work (people, commitments, open loops, each with its source and a confidence level) and the output is a finished draft or a scheduled hold, not a search result.
```

**Q6. オープンソースにする予定は？**
```
Not the app today. What is open in practice: your data is a local database you can export in full, and the memory is reachable over MCP, so you can point your own agents at it instead of being locked into our UI.
```

**Q7. Accessibility APIって重くない？ / バッテリーは？**
```
It's bounded on purpose: we walk the focused window only, collapse near-duplicates, and accumulate dwell rather than re-reading. The target we hold ourselves to is under 5% CPU at idle averaged over a minute, and the panel has to open in under 100ms. If you see worse on your machine, that's a bug and I want the report.
```

**Q8. Appleが同じことをやったら？**
```
Then a lot of people get a better OS, which is fine by me. The part that isn't easy to copy is the execution loop across your other tools with an approval model on top — that's cross-app, cross-vendor work, and it's the part Apple has historically not wanted to own.
```

**Q9. BYOKが面倒**
```
You don't need an API key if you already pay for Claude, ChatGPT, or Gemini — ShogunAI can run inference through the plan you already have, with your explicit opt-in. Keys are supported as the fallback, and either way they live in the system Keychain, never in a config file.
```

**Q10. 日本語でのコメント（JP勢向け）**
```
日本語でも問題なく動きます。画面テキストの取得も会議の文字起こしも日英どちらも扱えて、UIは今のところ英語です（日本語UIは対応予定）。使ってみて詰まったところがあれば、この場で日本語で聞いてください。
```

**Q11. ネガティブ/辛辣なコメント**
```
That's a fair hit and I'm not going to argue it. [具体的に何を認めるか1文] Here's what I'll do about it: [期限つきの1文]. If you want, ping me when it lands and tell me if it actually fixed your case.
```
> 反論しない。事実で受けて、期限を切る。これがPHで最も票を集める返信の型。

**Q12. 会議の音声はどこへ行く？（正確に答える。ごまかさない）**
```
Live transcription for meetings runs through a cloud speech provider, opted out of any model training, and we never write audio to disk — not even a temp file. What gets stored is the text and where it came from. It's disclosed in the app before you turn meetings on, and you can leave meeting capture off entirely.
```

---

## 7. 外部拡散の原稿

> **共通ルール**: どこにも「upvoteして」と書かない。「出した／ここにいる／聞いてくれ」で通す。

### X（ローンチ告知・EN）
```
ShogunAI is live on Product Hunt ⚔

A macOS app that keeps one private memory of your workday — no screenshots, no recordings — and then actually finishes things: the follow-up, the recap, the morning brief.

Nothing leaves your Mac without your approval.

30-day full trial → [link]
```

### X（日本語・JP向け）
```
ShogunAI を Product Hunt に出しました ⚔

Macの中だけで一日の仕事を記憶して、そこから実行まで行くアプリです。スクショも録画も保存しません。会議のあとに残るのは議事録ではなく、次の一手の下書きです。

外に出るものは必ず承認を挟みます。

30日フルトライアル → [link]
```

### X（当日中盤・伸ばす用スレッド）
```
The question I got most in the first 3 hours wasn't about features. It was: "how do I know it isn't watching me?"

So here's the actual answer, in the order it matters: [4ツイートで技術規律を分解 → 最後にPHリンク]
```

### LinkedIn（少しフォーマルに）
```
We launched ShogunAI on Product Hunt today.

Most AI tools ask you to re-explain your own week before they can help. ShogunAI removes that step: it keeps one private memory of your workday on your Mac — text only, never screenshots or recordings — and turns it into finished work: drafted follow-ups, meeting recaps that know the relationship, a morning brief that tells you what moved overnight.

Anything that leaves the machine stops for your approval first.

macOS 14+, Apple Silicon. 30-day full trial, no card: [link]
```

### Discord / Slack コミュニティ（短く、押し付けない）
```
We're live on PH today ⚔ — ShogunAI, local-first work memory for Mac that drafts and executes instead of just recalling. I'm in the comments all day if you want to grill me on the privacy model: [link]
```

### Reddit（r/macapps 等。宣伝臭を消す／サブごとの規約を必ず確認）
```
Title: I built a Mac app that remembers my workday as text (no screenshots) and drafts the follow-ups

Body: 数段落で「なぜ作ったか」「何を保存しないか」「何ができないか」を書く。PHリンクは本文末に1回だけ。機能列挙は書かない。
```

### Hacker News（Show HN。PHと同日に出すか翌日に分けるかは体力で決める）
```
Show HN: ShogunAI – Local-first work memory for macOS that drafts and executes

本文: 技術的な事実だけで書く。accessibility tree のバウンド走査、暗号化ローカルDB、Hot/Warm/Cold、provenance＋confidence、外部送信の承認モデル、BYOK。マーケ語はゼロにする。HNはトーンを1つでも間違えると沈む。
```

---

## 8. ハンター／動員

- **セルフハントで問題ない。** 2026年のPHでは大物ハンターの効果は薄く、メイカー本人が終日コメントにいる方が効く。
- 事前にやってよいこと: フォロワーに「◯日に出す」と予告する、Discord/メーリングリストに当日アナウンスする、PH上で自分のフォロワーを増やしておく（ローンチ時に通知が飛ぶ）。
- **やってはいけないこと**: 票の直接依頼、インセンティブ付きの投票、投票用の新規アカウント作成の誘導。1件でも露見すると当日ランキングから外される。
- 事前に確保しておくもの: 実ユーザー **10〜20人**（トライアル中でよい）に「当日、正直な感想をコメントで書いてくれ」と頼む。**票ではなくコメントを頼む**——これは規約内で、かつランキングに効く。

---

## 9. KPIと事後の動線

| 指標 | 目標（現実的なライン） |
|---|---|
| PH順位 | Top 5 of the Day（Top 3は取れたら上出来） |
| PHコメント数 | 60+（メイカー返信を除く） |
| LP セッション（当日） | 3,000〜6,000 |
| DMGダウンロード | 400〜900 |
| 起動→権限許可完了（アクティベーション） | ダウンロードの55%以上 ← **ここが本当のKPI。順位ではない** |
| 7日後もキャプチャが動いている（D7 retention） | 30%以上 |
| トライアル→有料転換（30日後） | 8〜12% |

**ローンチ後の動線（PHの1日より大事）**
- D+1: 「初日に聞かれた質問トップ5と、その答え」をブログ＋Xに出す。PHページにも同じものを追記コメントで置く。
- D+3: ダウンロードして**権限を許可していない人**へのメール1通（何が動くようになるかだけを書く）。
- D+7: PH経由ユーザーだけを見たリテンションを実測して、離脱理由をヒアリング。**バッジ（Top◯）は取れた場合のみLPに貼る**（§10）。
- D+25: トライアル終了5日前のメール。使用実績（記憶した件数・実行した件数）を本人のローカル数値で提示。

---

## 10. LP側の必須修正（PH流入に耐えるために、ローンチ前に必ず）

PH勢はLPを疑いながら読む。**現状のLPには当日に燃える要素が3つある。**

1. **架空のテスティモニアル** — `apps/website/src/i18n/dictionaries.ts` の `testimonials.items` に実在しない人名・肩書き（`alex_builds` / `Maria Kowalski` / `Kenji Tanaka` 等）が入っており、`Testimonials.tsx` でそのまま表示されている。PHのトップページに載ると必ず突かれ、突かれたら**プライバシーの主張の信頼まで一緒に落ちる**。ローンチ前に、実ユーザーの実名許諾コメントに差し替えるか、セクションごと非表示にする。二択で、加工での回避はしない。
2. **`stats` の "4h saved per week, on average"** — 根拠のない平均値。同ファイル `stats.items`。実測がないなら数値を落として、検証可能な事実（「スクショを1枚も保存しない」「ノッチ展開100ms」等の自社SLO）に差し替える。
3. **Product Hunt バッジ表記** — 辞書に `badges.productHunt = '#1 Product of the Day'` が定義されている（現在レンダリングされているのは `authority` の "Coming soon on Product Hunt" のみで、実害は出ていない）。**取っていない実績の文字列がコードに残っているのは事故の元**なので、ローンチ前に削除し、当日以降に実際に取れた順位のバッジ（PH公式の埋め込み）だけを貼る。

その他の準備:
- ヒーロー直下に**ダウンロードボタン**（PH流入の第一動作は「落とす」）。要件（macOS 14+ / Apple Silicon）をボタンの直下に小さく明記。
- `?ref=producthunt` 判定で、ページ上部に「30-day full trial for Product Hunt」の細いバーを出す。
- Privacy & Security ページを**コメント返信から直リンクできる粒度**に整える（§6 のQ1/Q2/Q12の内容がそのページにあること）。
- ステータス/更新履歴ページ（あるいはChangelog）を1枚用意。「生きているプロダクト」の証明になる。

---

## 11. 出す前の最終チェック（ブランド＋規約）

- [ ] tagline / description / gallery に技術スタック名が出ていないか（技術名は first comment とコメント返信のみ）
- [ ] 絵文字は ⚔ のみか（他はゼロ）
- [ ] "AI-powered" / "revolutionary" / "game-changing" / "second brain" 不使用か
- [ ] 競合の固有名がこちらの原稿に一切ないか（相手が出したらカテゴリで返す）
- [ ] "personal AGI" を使っていないか（PH面では使わない）
- [ ] 実行の訴求すべてに**承認（approval）**が併記されているか
- [ ] gold の面積が画面の5%以下か（ギャラリー画像すべて）
- [ ] Kamon の周囲余白 1/6 が確保されているか
- [ ] 日本語版と英語版の**主張が一致**しているか（翻訳ではなくネイティブ表現）
- [ ] LPの架空テスティモニアル・未実証の数値・未取得バッジが消えているか（§10）
- [ ] DMGをクリーンなMacで落として起動できたか（Gatekeeper実機確認）
- [ ] 30日トライアルのチェックアウトをテスト購入で確認したか
