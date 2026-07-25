# 会議コンテキスト と Context Dashboard — 設計と乖離の棚卸し

対象: (a) 会議を検知して裏側でコンテキストを取り、要約して state に落とす経路、(b) このアプリ自身の
コンテキストを見る「ダッシュボード」、(c) それを設計するにあたって判明した**プロダクト趣旨からの乖離**。

- 作成日: 2026-07-25
- 上位文書: `docs/requirements-v1.0.md`（正本）/ `CLAUDE.md`（不変条件）
- 先行文書: `docs/context-architecture-design.md`（取得〜文脈化の全体設計）/ `docs/context-layer-audit-and-plan.md`（検索・抽出の監査）
- 本書は実コードの読解に基づく**設計・判断記録**であり、実装の完了報告ではない

---

## 0. 結論（先に要点）

1. **「裏側で録画」は設計から恒久的に外す。** CLAUDE.md 不変条件2（画像を一切保存しない）に真正面から
   抵触する。回避策ではなく**製品の差別化そのもの**なので、譲らない方が強い。
2. **「裏側で録音」は v1 では書けない**（NFR-PRV-01: 音声を取得・保存するコードを書かない、v1.5）。
   ただし **録音なしでも会議コンテキストの大半は今日取れる** — カレンダーの予定・参加者・会議アプリの
   AXテキスト（ライブキャプション/チャット/共有ノート）・会議中に触っていた資料。ここが v1 の正解。
   音声レーンは v1.5 に「後から差せる形」で器だけ用意する（§3.6）。
3. **ダッシュボードは新規プロダクトではない。要件にあるのに存在しない Full UI がそれ。**
   `OpenFullUi` は状態機械の Effect として存在するだけで**開く窓が無い**
   （`notch/statemachine.rs:264`、`integrate.rs:766`）。結果として要件が Full UI に置いた機能
   （トレーサビリティ閲覧・Dream Cycle結果・実行履歴・APIトークン・名寄せ修正・メトリクス）が
   **全部行き場を失い**、設定だけが 560px のパネルに押し込まれている。
4. **「ちゃんと取れているか」に答える指標が1つも無い。** SLO はレイテンシしか見ていない（`metrics.rs:131`）。
   取得の網羅性（Coverage）・歩留まり（Yield）・接地率（Grounding）は**計測すらしていない**。
   これが「コンテキストが取れているか分からない」の正体で、ダッシュボードの中核はここ（§4.3）。
5. **最大の乖離は会議でもダッシュボードでもない。「ボタンを押しても仕事が終わらない」こと。**
   `notch_exec.rs` の実効果は、検索＝ヒット数を数えて `eprintln!`、ドラフト＝`"draft (reply)"` という
   **中身の無い文字列**をノートに追記、通知＝`eprintln!` のみ。さらに7つのプリセットエージェント
   （Meeting Prep を含む、`presets.rs:144`）は**どこからも呼ばれていない**（参照ゼロ）。
   会議コンテキストを完璧に取っても、この環が欠けている限り「記録ツール」にしかならない。

---

## 1. 乖離の棚卸し（重大度順・実コード根拠つき）

> 「取得 → 評価 → 蓄積 → 文脈化 → **提示 → 実行**」のうち、後ろ2つが薄い。前半（取得・蓄積・検索）は
> 監査済みで土台がある（`docs/context-layer-audit-and-plan.md`）。以下は**その先**の乖離。

### D1【致命】実行が空洞 — 「ボタンを押して仕事が終わる」の"終わる"が無い

| 押されたアクション | 実際に起きること | 根拠 |
|---|---|---|
| Search memory: X | ヒット件数を数えて標準エラーに出力。**ユーザーには何も出ない** | `notch_exec.rs:44-48` |
| Draft reply | `append_note("draft (reply)")` — **本文が無い**。生成すらしていない | `notch_exec.rs:49-52` |
| Remind: … | `eprintln!("[exec] show notification")` | `notch_exec.rs:53-56` |

プリセット（Reply Drafter / **Meeting Prep** / Task Extractor / Follow-up Sentinel / Calendar Scheduler /
Issue Triage / Note Capture）は**権限レベルの宣言表として存在するだけ**で、実行する runtime が無い。
`PresetId::` の参照は `presets.rs` 自身とそのテスト以外に存在しない。

→ **アクションは「意図」を運べるが「生成物」を運べない。** `ActionCandidate` は
`{action, level, rationale}` のみで payload の席が無い（`assemble.rs:71-79`）。
`LocalAction::SaveDraft { target: &'static str }` は静的文字列しか持てない（`permission.rs:37`）。
これが構造的な原因。

### D2【致命】「時間の区間」という概念が無い

`event_log` は**点イベント**のみ。会議・通話・集中作業は**区間**であり、
「この30分の間に見ていたもの・話したこと・決まったこと」をまとめる器が存在しない。
`thread_key`（`thread.rs`）は会話の同一性であって区間ではない。

→ 会議コンテキストは「器が無いから作れない」のであって、キャプチャ手段の問題ではない。

### D3【致命】未来の予定を置く場所が無い

カレンダーは `read_sync` で `event_log` に**過去ログとして** append される。さらに正規化器は
RFC3339 の開始時刻を解釈できず **`ts=0` に潰す**（`result.rs:100-118` — 数値/数値文字列のみ受理）。
参加者・開始/終了・会議URLは `body` の中に文字列として溶ける。

→ 「15分後に会議がある」を**検知できない**。US-03（会議前の自動準備）と FR-AG-11（Meeting Prep）は
現状のデータモデルの上では成立しない。

### D4【重大】取得の網羅性を誰も測っていない

- 除外（`capture/exclusion.rs`）で落ちた回数を**数えていない** → 除外が効きすぎていても気づけない
- フォーカス時間と取り込み件数の比を取っていない → 「よく使うアプリなのにイベントが無い」盲点が見えない
- `extract` の歩留まり（何件のイベントから何件の候補が出たか）を記録していない
- チャット回答に provenance が付いた割合（接地率）を記録していない

計測されている6項目はすべてレイテンシ/CPU（`metrics.rs:147`）。**質と網羅性のメトリクスがゼロ。**

### D5【重大】Full UI が無いため、要件の行き場が無い

FR-TR-02（トレーサビリティ閲覧）/ FR-DC-06（Dream Cycle 結果）/ FR-AG-18（実行履歴）/
FR-API-03（APIトークン発行）/ FR-ST-10（名寄せ誤統合の修正）/ FR-SET-01（設定）/
NFR-SLO-00（メトリクス閲覧）— **すべて「Full UIで」と書かれていて、その Full UI が存在しない。**

### D6【中】ソース種別ごとのセグメンタが無い

AI セッションだけは構造化リーダーがある（`ai_session.rs` — 役割・時刻・セッションIDが取れる）。
これは正しい方向。しかし会議アプリ・メールクライアント・チャットの画面テキストは、
参加者名リストやボタン名まで含めて**平らな文字列**として入る。
関連度判定はいまだに**タイトル語の部分一致**（`daemon.rs:1282-1295`、ヒットで1.0 / 外れて0.4 の二値）。

### D7【中】confidence が構造的に High 帯に到達しない

ローカル抽出は上限 0.4、`corroborate` の上限は 0.75（High 未満、意図的）。High への昇格は
Batch 分類（Select KK）だけが担うが、**キーが未整備で未稼働**。
→ Fusion は永遠に「possibly」しか言えず、ボタンの説得力の源が無い。会議の Recap を作っても同じ壁に当たる。

### D8【小】ダッシュボードは「記録ツール化」の入口でもある

CLAUDE.md の一行目は「記録ツールではない。**状態の推定と実行**のプロダクト」。
眺めるための画面を足すことは、その線を踏み越えるリスクそのもの。
→ 設計上の防波堤を §4.2 に置く（**すべての指標に「直す導線」を必須にする**）。

---

## 2. 設計の前提: 不変条件との突き合わせ（会議まわり）

| やりたいこと | 不変条件・要件 | 判定 |
|---|---|---|
| 画面/会議の**録画** | 不変条件2「スクリーンショット・画像データを一切保存しない」 | ❌ **恒久的に不採用**。回避策も設計しない |
| **音声の録音・保存** | NFR-PRV-01「音声を取得・保存するコードを書かない」 | ❌ v1 では書かない |
| オンデバイス ASR（波形は RAM のみ、**テキストだけ保存**） | 要件では音声=v1.5 | ⚠️ **v1.5**。不変条件2の文言精緻化＋同意設計が前提（§3.6） |
| カレンダー（予定・参加者・時刻） | 第1層 MCP 読み取り（FR-INT-04） | ✅ v1。ただし D3 の修正が要る |
| 会議アプリの **AXテキスト**（キャプション/チャット/参加者名） | 不変条件2 の範囲内（テキストのみ） | ✅ v1 |
| 会議中に開いていた資料・スレッド | 既存キャプチャ | ✅ v1（区間に紐づける器が要る = D2） |

**結論**: v1 の会議機能は「録らない会議コンテキスト」。音声が無くても、
*誰と・何について・何が決まり・誰が何を負ったか* は上記4つでかなり埋まる。
音声は「解像度を上げる後付けの1ソース」として v1.5 に差す（§3.6）。

---

## 3. 設計① — Meeting Context（録らずに会議を文脈化する）

### 3.1 中核概念: `session`（区間）を第一級にする

D2 への回答。**点の event_log の上に、区間の session を1枚重ねる。**

```
sessions            区間: [started_at, ended_at)  kind = meeting | call | focus
  ├─ calendar_occurrence_id?   予定に紐づくか（飛び込み会議は NULL）
  ├─ participants              people.id[]（identity.rs で名寄せ済み）
  ├─ thread_key?               関連する会話スレッド
  ├─ summary / decisions       Recap が書く（Dream Cycle または即時）
  └─ confidence + provenance   検知も推定である以上、断定しない（state と同じ規律）

event_log.session_id  ← additive。区間中のキャプチャ・チャット・メールが自動的にぶら下がる
```

これが入ると、**会議に限らず**「さっきの30分、何をしていたか」が答えられるようになる
（`kind=focus`）。会議機能は session の最初の応用にすぎない、という位置づけが正しい。

### 3.2 検知 — 単一信号に頼らず、confidence で扱う

| 信号 | 取得元 | 単独の強さ |
|---|---|---|
| ① 予定がある | `calendar_occurrences`（§3.3 の新テーブル） | 強（ただし予定=出席ではない） |
| ② 会議アプリが前面 / マイクが使用中 | NSWorkspace + bundle id 表 + マイク使用中フラグ（**音声は読まない**） | 中 |
| ③ 会議UIの痕跡 | AX に参加者リスト・Leave/Mute 等のコントロールが見える | 中 |

**②③ が立てば session を開く。① と一致すれば confidence を上げ、予定に結び付ける。**
①だけ（前面に来ない）は「予定はあったが出席の証拠なし」として区間を開かない。
誤検知は state tables と同じ扱い — 断定せず、Recap で「possibly」として出し、
ユーザーの1タップで確定する（それが唯一の正直な昇格経路）。

> マイク使用中の検知は「使用中か」の真偽だけを見る。**音声ストリームには触れない。**
> ここを曖昧にすると不変条件2の精神を破るので、実装時もこの境界をコメントで明示すること。

### 3.3 予定を持つ — `calendar_occurrences`（D3 の修正）

未来の予定を append-only の過去ログに入れるのは設計として誤り。**予定は state であって event ではない。**

```sql
CREATE TABLE calendar_occurrences (
  id INTEGER PRIMARY KEY,
  source TEXT NOT NULL,              -- 'gcal'
  external_id TEXT NOT NULL,
  starts_at INTEGER NOT NULL,        -- epoch ms（RFC3339 をここで解釈する）
  ends_at INTEGER,
  title TEXT,
  location TEXT,                     -- 会議URLを含む（第三者に出さない）
  attendees TEXT,                    -- JSON: {email, name, response}[]
  updated_at INTEGER NOT NULL,
  UNIQUE (source, external_id)
) STRICT;
CREATE INDEX idx_cal_occ_starts ON calendar_occurrences (starts_at);
```

- `result.rs` の正規化に **RFC3339 パーサ**と、カレンダー用の構造化フィールド抽出を足す
  （現状は `ts=0` に潰れる）。ここは会議機能の前提であり、Morning Brief の Today セクション
  （FR-MB-01）の前提でもある。
- attendees は **identity.rs の供給元**になる。名寄せは「仕組みは完成、供給元が無い」状態
  （audit §Phase I）だったので、これが最初の実供給になる。

### 3.4 3フェーズ（Prep / Live / Recap）

**Prep（T-15分、L1 = ローカル集約のみ・外部送信ゼロ）**
US-03 / FR-AG-11 のそのまま。**押してから集めない**（CLAUDE.md）。
既存の `Db::build_reply_context` と同型の `build_meeting_context(occurrence_id)` を
**予定の15分前に**組み立て、RAM に常駐させる。中身:
- 参加者ごとの直近のやり取り（Gmail/Slack、名寄せ済み）
- その相手/プロジェクトに紐づく未解決 commitments・open_loops
- 関連する過去スレッド（`search_hybrid` 上位k）
- 前回の同一予定（繰り返し会議）の Recap

Notch は**ゴールドのインジケータ＋Hover に1行**だけ。自動で展開しない（割り込まない原則、FR-MB-03 と同じ規律）。

**Live**
session を開き、区間中のイベントに `session_id` を付ける。パネルは "in meeting" 表示のみで、
提案を出さない（会議中に話しかけてくるプロダクトにはしない）。
キャプチャ対象は既存のまま — 会議アプリのキャプション/チャットは `AXStaticText` として既に拾える。
**キャプションを ON にしろとは言わない。取れたら取る。**

**Recap（session クローズ時 + その夜の Dream Cycle）**
1. 区間の要約（3〜5行）→ `sessions.summary`
2. **誰が何を負ったか**の抽出 → commitments / open_loops の**候補**（`extract` の cue に会議文脈を追加）
3. 提示は「possibly」帯。ユーザーが1タップで確定 = `UpdateState`（L2）→ confidence 1.0 + provenance=編集イベント
   （FR-ST の「ユーザー明示編集」経路、要件 §6.4 の表）
4. 外部送信は一切なし。議事録の共有は L3（v1 スコープ外に留める）

### 3.5 会議コンテキストの「取れているか」を測る

会議は成功/失敗が見えやすいので、Context Health（§4.3）の最初の実装対象にする。

| 指標 | 意味 |
|---|---|
| 予定に対する session 検知率 | 予定があった会議のうち、区間を開けた割合 |
| 区間あたりのキャプチャ密度 | 会議中に取れたテキスト量（0 なら会議アプリのAXが取れていない＝盲点） |
| Recap 採用率 | 提示した候補のうちユーザーが確定した割合 = **抽出品質の唯一の正直な指標** |

### 3.6 音声レーン（v1.5）— 今作らないが、後から差せる形にする

差込口を**1点**に絞っておけば、v1.5 の作業は ASR 実装だけになる。

```
（v1.5）音声 → オンデバイスASR → transcript_segments{session_id, speaker?, ts, text}
                                        └→ 既存の抽出・検索・Recap がそのまま効く
```

v1.5 に持ち越す**未決事項**（先に書いておくべきこと）:

1. **不変条件2の文言精緻化が必要**。現行「スクリーンショット・画像データは一切保存しない」に対し、
   音声を扱うなら「**音声データもディスクに書かない。波形はRAM内でのみ処理し、永続化するのはテキストのみ**」
   と明文化する。曖昧なまま実装させない。
2. **同意**。一方当事者同意で足りない法域がある。録音でなく ASR でも会話の記録である以上、
   (a) 会議アプリ別の既定 OFF、(b) 参加者への開示文言、(c) provenance に
   「キャプション由来 / 音声由来」を残す、の3点は設計必須。
3. **技術調査**: macOS 14.4+ の Core Audio tap（他アプリ音声）と TCC マイク権限、
   オンデバイス ASR モデルのサイズとアイドル CPU（SLO 5%）への影響。
4. 参照解決（「あの件」）は**音声/テキスト共通の資産**なので、テキストで先に作り込む方針は変えない
   （audit §6-2 の判断を踏襲）。

---

## 4. 設計② — Context Dashboard（= 未実装の Full UI ＋ Context Health）

### 4.1 位置づけ

**新しいアプリを作るのではなく、要件にある Full UI を実装し、そこに Context Health を足す。**
Notch は「今」のためのサーフェス（100ms で開き、4つのボタンを出す）。Full UI は
「振り返り・診断・修正」のためのサーフェス。役割が違うので、Notch に足すのは誤り
（現に設定を 560px パネルに押し込んで無理が出ている）。

### 4.2 防波堤 — 「眺める画面」を作らない

> **すべての指標に「直す導線」を必ず1つ付ける。直せない指標は載せない。**

| 見えるもの | 直す導線 |
|---|---|
| このアプリのイベントがゼロ | 除外設定を開く / 権限を再付与する |
| 接続が amber | 再認証する |
| state が Medium 止まり | Batch 分類が未稼働である旨と、その設定導線 |
| 抽出がゼロのソース | 言語設定 / セグメンタ未対応として報告する |

これが「記録ツールではない」を UI レベルで守る規律（D8）。ダッシュボードは**ワールドモデルの整備場**であって、
ログビューアではない。

### 4.3 Context Health — 「ちゃんと取れているか」に答える画面 ★本題

指標は**すべて Rust コア側で集計**し、UI はその投影にする（不変条件1）。

| 指標 | 定義 | 実装コスト |
|---|---|---|
| **Coverage** | 直近24hのアクティブ時間のうちキャプチャが有効だった割合。ソース別の到達状況 | 除外/失敗のカウンタ追加（**現在は数えていない**） |
| **Blind spots** | フォーカス時間は長いのにイベントがほぼ無いアプリの検出 | フォーカス時間の集計を追加 |
| **Freshness** | ソース別の最終同期からの経過 | `runtime.rs` に既にある。出すだけ |
| **Yield** | 1000イベントあたりの state 候補生成数／確定数 | `extract` の戻り値を計上 |
| **Confidence mix** | state の High/Medium/Low 構成比の推移 | state tables の集計のみ |
| **Grounding** | 回答・ドラフトのうち provenance 付きで出せた割合 | `shogun_chat` の citations を計上 |
| **SLO** | 既存6項目の p50/p95（NFR-SLO-00） | `metrics.rs` を出すだけ |
| **Egress** | 直近の外部送信件数・バイト数・route別（第三者経由を強調） | `traceability_log` の集計 |

**対称性（不変条件6）**: 上記は先に `GET /v1/health/context`（REST）と MCP tool `context_health` として
定義し、Full UI はその投影として実装する。**webview 側に集計ロジックを書かない。**
外部 AI も「SHOGUN のコンテキストが健全か」を同じ数字で問える。

### 4.4 画面構成

| 画面 | 中身 | 対応要件 |
|---|---|---|
| **Today** | Morning Brief / 今日の予定（`calendar_occurrences`）/ 各予定の Meeting Prep 導線 | FR-MB-01, US-03 |
| **Context Health** | §4.3 | 新規（NFR-SLO-00 を内包） |
| **Memory** | 検索 / スレッド / state 4テーブル（confidence 帯つき）/ provenance / 名寄せの分割修正 | FR-ST-10, US-04 |
| **Sources** | 接続・鮮度・スコープ・第三者バッジ・**キャプチャ除外設定** | FR-INT-06/07, Phase S |
| **Activity** | 実行履歴 / Dream Cycle 結果 / L3 承認キュー | FR-AG-18, FR-DC-06 |
| **Traceability** | 時系列・route/purpose フィルタ・第三者バッジ | FR-TR-02 |
| **Settings** | 既存パネル設定の移設 + APIトークン | FR-SET-01, FR-API-03 |

Notch との関係: Notch の Hover / amber・赤インジケータからは、**必ず対応する Full UI の画面に着地する**
（FR-NU-07 の「エラー詳細は Full UI のログ画面へ導線」）。

---

## 5. 設計③ — 提示と実行の環を閉じる（D1 への回答）

会議もダッシュボードも、ここが埋まらないと価値にならない。

1. **`ActionCandidate` に payload を持たせる**
   `{action, level, rationale, payload}` — payload は**事前生成された生成物**か、その生成ジョブID。
   Fusion は候補を作るだけでなく、**生成物への参照**を運べるようにする。
2. **`LocalAction` を「仕事が終わる」語彙にする**
   `SaveDraft { target, body }`（本文を持つ）/ `CreateNote { body }` /
   `OpenThread { thread_key }` / `ShowResult { … }`。現在の静的文字列だけの表現をやめる。
3. **プリセット runtime を作る**
   `PresetId → operations → ExecutionEngine` の経路。7つの宣言が初めて動く。
   **Meeting Prep を最初の実例にする**（L1・外部送信ゼロなので、権限面のリスクが最も低い）。
4. **事前生成を守る**
   ドラフト本文は押下前に組み立て済み or ストリーミング即開始（SLO: 提示150ms / 初トークン1s）。
   「押してから集める」は禁止（CLAUDE.md）。
5. **効果の可視化**
   実行結果は Activity に残り、`ShowResult` は Notch に返る。`eprintln!` で終わるアクションを1つも残さない。

---

## 6. スキーマ変更（すべて additive）

```sql
-- V5: sessions（区間）+ calendar_occurrences（予定）+ event_log の紐づけ
CREATE TABLE sessions (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,                    -- 'meeting' | 'call' | 'focus'
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  calendar_occurrence_id INTEGER REFERENCES calendar_occurrences(id),
  thread_key TEXT,
  participants TEXT,                     -- JSON: people.id[]
  summary TEXT,
  confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_sessions_started ON sessions (started_at);

ALTER TABLE event_log ADD COLUMN session_id INTEGER;   -- NULL 可（既存行は NULL のまま）
CREATE INDEX idx_event_log_session ON event_log (session_id, ts);

-- calendar_occurrences は §3.3
-- （v1.5）transcript_segments {session_id, speaker, ts, text} — v1 では作らない
```

- **後方互換を破らない**: 追加列は NULL 許容、既存行は無変更。既存クエリは影響を受けない。
- **ロールバック**: `DROP TABLE sessions / calendar_occurrences` + `event_log.session_id` は
  SQLite の列削除制約に合わせ、ロールバック手順は「列を残したまま未使用にする」を正とする
  （マイグレーションファイルに明記。**メモリは年単位で生きるデータ**）。

---

## 7. 段階計画

| 段 | 内容 | 依存 | 価値 |
|---|---|---|---|
| **E1** | **実行の環を閉じる** — payload / LocalAction 拡張 / プリセット runtime | 無し | ★最大。これ無しでは他が飾り |
| **E2** | Full UI シェル + Traceability + Activity（**既存 backend の投影だけ**） | 無し | 要件の行き場ができる |
| **E3** | Context Health API（REST/MCP）+ 画面。カウンタ追加が実作業 | E2 | 「取れているか」に答えられる |
| **E4** | `calendar_occurrences` + RFC3339 正規化 + Today + **Meeting Prep（プリセット第1号）** | E1, gcal 接続 | US-03 が成立 |
| **E5** | `sessions` + 会議検知 + Live + Recap 候補 | E4 | 会議コンテキストの本体 |
| **E6** | *(v1.5)* 音声レーン — 不変条件の文言改訂・同意設計・ASR | E5 + 製品判断 | 解像度向上 |

E1 と E2 は独立に着手できる。E3 のカウンタは E1/E2 の実装中に同時に埋めるのが安い。

**受け入れ基準**
- E1: 4つのボタンすべてが**ユーザーに見える結果**を返す（`eprintln!` で終わる経路がゼロ）
- E3: `context_health` が REST・MCP・UI の3面から同一の値を返す（対称性テスト）
- E4: 予定15分前に Prep が RAM に載っていることを計測（組み立て時間を `build_ms` で同梱）
- E5: 予定のある会議の検知率と Recap 採用率が Context Health に出る
- 全段: レイテンシに触る変更は p50/p95 を測ってからマージ（SLO ゲート）

---

## 8. 不変条件チェック

- ✅ **1. データ重心は Rust コア** — Context Health の集計はコア側。webview は投影のみ
- ✅ **2. 画像を保存しない** — 録画は恒久的に不採用。会議は AX テキストのみ。マイクは「使用中か」の真偽のみ読み、ストリームに触れない
- ✅ **3. 生データはデバイス外に出さない** — 会議 Recap の生成は Batch（処理チャンクのみ）＋トレース。会議URL・参加者を第三者に渡さない
- ✅ **4. L1 に外部送信を含めない** — Meeting Prep はローカル集約のみ。議事録共有は v1 スコープ外
- ✅ **5. 鍵の分離** — Recap/要約/Brief = Select KK（Batch）。チャット・ドラフト = BYOK
- ✅ **6. 人間UIとAI APIの対称** — Context Health は REST/MCP を先に定義し UI をその投影にする
- ✅ **7. secrets は Keychain のみ** — 変更なし

---

## 9. 要確認（プロダクト判断が要るもの）

1. **録画は恒久的に不採用でよいか。** 推奨: はい。不変条件2は差別化そのもので、ここを緩めると
   競合と同じカテゴリに落ちる。
2. **音声を v1.5 のままにするか、前倒しするか。** 前倒しするなら
   不変条件2の文言改訂・同意設計・要件（NFR-PRV-01）の改訂がセットで必要。
   本書は「v1.5 のまま・器だけ用意」を前提に書いている。
3. **Full UI は別窓か。** 要件（§6.1 の状態機械）は別窓。本書もそれに従っている。
4. **Context Health をユーザーに見せるか、Advanced に隠すか。** 推奨: 見せる。
   ただし §4.2 の「直す導線を必ず付ける」を条件にする。

---

*本書は実コード（`notch_exec.rs` / `notch_actions.rs` / `presets.rs` / `assemble.rs` / `permission.rs` /
`result.rs` / `daemon.rs` / `metrics.rs` / `integrate.rs` / `statemachine.rs`）の読解に基づく。
実装は §7 の E1 から段階的に進める。*
