# コンテキスト層 機能監査と統合設計計画

「ボタン一つで最適な返信」「『あの件どうなってる?』に正しく答える」——この2つのコア体験に対して、
現状のバックエンドが何をどこまでやれているかを実装ベースで監査し、統合コンテキスト層の設計と
実装計画をまとめる。

- 作成日: 2026-07-25
- 対象: `crates/shogun-memory` / `shogun-core` / `shogun-fusion` / `shogun-integrations` / `apps/desktop`
- 監査方法: 実コードの読解（推測ではなく配線の有無を確認）

---

## 0. 結論（先に要点）

**土台は良い。ただし「検索」がチャットに繋がっておらず、意味検索は動く状態にない。**

いま `ああああ` と聞いても、`あの件どうなってる?` と聞いても、モデルに渡っているコンテキストは
**commitments と open_loops のリストだけ**（`daemon.rs:248 inline_memory`）。質問文に応じた
**イベントログの検索は一切行われていない**。つまり過去のメール・Slack・画面キャプチャは
DBに入っているのに、回答には使われていない。

さらに:
- **埋め込みモデルが存在しない**（`ort`/ONNX依存がCargo.tomlに無く、`MockEmbedder`のみ）→ 意味検索は不可
- **名寄せ（identity）・Dream Cycle・Cold層が未配線**（コードはあるが誰も呼んでいない）
- **抽出ルールが英語のみ**（"I'll send…"等）→ 日本語ユーザーでは抽出がほぼ効かない
- **DBは平文SQLite**（暗号化なし）

逆に言えば、**検索を繋ぐだけで体験は一段跳ね上がる**。FTS5 trigram は日本語でも機能するので、
埋め込みを待たずに今日から効く。

---

## 1. 現状のデータフロー（実測）

```
[画面] AXテキスト
   └─ capture_source::spawn_capture_poller  (lib.rs:308 で起動)
        └─ Db::ingest_capture               (daemon.rs:139)
             ├─ capture_collapsed  → event_log へ append（content_hash で重複統合、dwell加算）
             └─ extract::extract   → commitments / open_loops の「候補」を低信頼度で作成

[Gmail/Calendar/Drive/…] 公式リモートMCP
   └─ ConnectorRuntime::poll_tick (15分)     (lib.rs:317 で起動)
        └─ event_log へ source=gmail/gcal/... で append

[チャット]
   └─ shogun_chat → chat_blocking            (inline_source.rs:508)
        ├─ db.inline_memory(8)  ←★ commitments + open_loops のみ。検索なし
        └─ BYOKのLLMへ prompt を送信
```

**event_log には全部入っているが、質問時に引き出す経路が無い。** これが最大のギャップ。

---

## 2. ギャップ表

| 領域 | 状態 | 根拠 |
|---|---|---|
| イベントログ（append-only, 重複統合, spatial-ready列） | ✅ 実装・稼働 | `V1__init.sql`, `daemon.rs:139` |
| キャプチャ除外（パスワードマネージャ/プライベートブラウジング） | ✅ 実装・稼働 | `capture/exclusion.rs` |
| state tables（people/projects/commitments/open_loops + confidence + provenance） | ✅ 実装・稼働 | `V1__init.sql`, `state.rs` |
| トレーサビリティ（digestのみ記録） | ✅ 実装・稼働 | `V1__init.sql`, `traceability.rs` |
| 第1層コネクタ読み取り同期 | ✅ 実装・稼働 | `runtime.rs`, `lib.rs:317` |
| L3承認 → 送信実行 | ✅ 実装・稼働 | `approvals.rs`, `send_exec.rs` |
| **FTS5全文検索 + RRFハイブリッド** | ⚠️ **実装済みだがチャット未接続** | `search.rs:105`。呼び出しは Memory API と「件数を数えるだけ」の notch action のみ |
| **埋め込み（意味検索）** | ❌ **モデル無し**（trait と Mock のみ、ONNX依存が無い） | `embed.rs`、`Cargo.toml` に `ort` 不在 |
| **名寄せ（cross-channel identity）** | ⚠️ 純ロジック実装済み・**未配線** | `identity.rs`、呼び出し元なし |
| **Dream Cycle / Batch分類 / Cold層** | ⚠️ 部品あり・**スケジューラ無し** | `jobs.rs`/`maintenance.rs`/`cold.rs`、desktopで未起動 |
| Context Fusion | ⚠️ `assemble` はあるがチャット経路では未使用 | `assemble.rs:126`、`daemon.rs:465` |
| 会話スレッド構造 | ❌ 未実装（event_logはフラット） | スキーマに thread 概念なし |
| DB暗号化 | ❌ 平文 | `memory_db` (lib.rs:1256)、SQLCipher無し |

---

## 3. セキュリティ監査

**守れている**
- secrets は Keychain のみ（不変条件7）。`check-secret-exposure.py` がCIで強制
- HTTPクライアントは `shogun-core` に限定（`check-http-egress.py`）
- 外部送信は digest のみ記録、本文は残さない
- スクリーンショットを保存しない（AXテキストのみ、不変条件2）
- L1に外部送信を含めない構造（送信はL3承認キュー経由のみ、不変条件4）

**リスク（要対応）**

1. **DBが平文** — `~/Library/Application Support/<id>/memory.db` に、業務の画面テキストが全文で残る。
   ホームディレクトリを読める何か（他アプリ、バックアップ、マルウェア、共有Mac）に全部見える。
   FileVault頼み。→ **SQLCipher + Keychain保管の鍵**を推奨。
2. **キャプチャ内容の無差別性** — 画面に出ているAPIキー・パスワード・他人の個人情報も
   そのままevent_logに入る。除外はバンドルID単位のみで、**内容ベースのリダクションが無い**。
   → 書き込み前に secret パターン（`sk-`, `ghp_`, JWT, クレカ番号等）をマスクする段を入れる。
3. **除外設定のUIが無い** — 除外ポリシーはコードのデフォルトのみ。ユーザーが
   「このアプリは見ないで」を設定できない。
4. **「ユーザーごとのセキュリティ」について** — 現状はローカルファースト**単一ユーザー**設計で、
   マルチテナントの概念自体が無い（分離＝macOSユーザーアカウント）。もしチームでの共有や
   サーバ側保管を将来やるなら、それは別アーキテクチャで、要件から設計し直す必要がある。
   **v1の前提は「データは端末から出ない」**なので、そこを変えるかは製品判断。

---

## 4. 精度監査

| 問題 | 影響 | 原因 |
|---|---|---|
| **質問に応じた検索をしていない** | 「あの件どうなってる?」に事実で答えられない | `inline_memory` が状態テーブル固定 |
| **抽出ルールが英語のみ** | 日本語の約束・依頼をほぼ拾えない | `extract.rs` が英語のpromise cue文字列マッチ |
| ローカル抽出は全て低信頼度 → Fusionが除外 | inline_memory が空になりがち | 設計通り（`LOCAL_RULE_MAX_CONFIDENCE` < 0.5）だが第2段（Batch分類）が未稼働なので**昇格されない** |
| 名寄せ未稼働 | 同一人物がGmail/Slack/GitHubで別人扱い | `identity.rs` 未配線 |
| スレッド構造なし | 「あの件」が指す対象を特定できない | イベントがフラット |
| 意味検索なし | 言い換え・同義語に弱い（FTSは字面一致） | 埋め込みモデル未搭載 |

> 補足: FTS5 の **trigram トークナイザは日本語でも機能する**（文字単位）。
> つまり埋め込みが無くても、検索を繋げば日本語の字面一致検索は今日から効く。

---

## 5. 統合コンテキスト層の設計

### 5-1. 全体像

```
                 ┌─────────────── Sources ───────────────┐
  画面AX ────────┤                                        │
  第1層MCP ──────┤   正規化 → redaction → event_log       │
  AIツール履歴 ──┤   (source, kind, thread_key, ts)       │
  （新規）       └────────────────┬───────────────────────┘
                                  │
                     ┌────────────┴────────────┐
                     │  Enrichment（非同期）    │
                     │  ・embed job (ONNX)      │
                     │  ・thread grouping       │
                     │  ・identity 名寄せ       │
                     │  ・Batch分類で信頼度昇格 │
                     └────────────┬────────────┘
                                  │
        ┌─────────────────────────┴──────────────────────────┐
        │  Retrieval: search_hybrid(FTS + vector) + RRF       │
        │  + state facts + 現在の画面 + 時間減衰・スレッド優先 │
        └─────────────────────────┬──────────────────────────┘
                                  │
              ┌───────────────────┴───────────────────┐
              │  Context Pack（トークン予算で切る）     │
              └───────────────────┬───────────────────┘
                   ┌──────────────┴──────────────┐
        ボタン一つで返信                「あの件どうなってる?」
```

### 5-2. ChatGPT / Codex など「AIツールとのやりとり」の取り込み

ここは**AXスクレイピングより、ローカルのセッションファイルを読む方が圧倒的に高精度**。
画面キャプチャは「見えている部分だけ・話者構造なし・重複だらけ」になるが、セッションファイルは
**役割（user/assistant）・時刻・スレッドIDが構造化済み**。

| ツール | 推奨取り込み方法 | 備考 |
|---|---|---|
| Claude Code | `~/.claude/projects/**/*.jsonl` を読む | 役割・時刻・セッションID付き。最も高精度 |
| Codex CLI | 同様にローカルのセッション/ログを読む | 形式は要調査 |
| ChatGPT (desktop/web) | ローカル履歴が無いのでAXキャプチャ or 公式エクスポート取り込み | 会話UI形状を認識するセグメンタが必要 |
| Claude/ChatGPTのブラウザ版 | AXキャプチャ + 会話セグメンタ | 同上 |

新しい `source` 値（`ai_session`）と `thread_key`（セッションID）で event_log に入れれば、
**既存の検索・Fusion・state抽出がそのまま効く**。スキーマ追加は additive で済む。

> プライバシー: これも端末内処理のみ。除外設定（このプロジェクトは取り込まない等）を必ず用意する。

### 5-3. スレッド（「あの件」を解決する単位）

追加マイグレーション（additive）:

```sql
ALTER TABLE event_log ADD COLUMN thread_key TEXT;   -- 会話/スレッド識別子
CREATE INDEX idx_event_log_thread ON event_log (thread_key, ts);

CREATE TABLE threads (
  id INTEGER PRIMARY KEY,
  thread_key TEXT NOT NULL UNIQUE,
  source TEXT NOT NULL,
  title TEXT,
  summary TEXT,                -- Dream Cycleが要約
  participants TEXT,           -- JSON: people.id[]
  project_id INTEGER REFERENCES projects(id),
  last_activity_at INTEGER NOT NULL,
  salience REAL NOT NULL,      -- 「あの件」候補の順位付け
  confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
```

`thread_key` の作り方: Gmail=threadId / Slack=channel+thread_ts / GitHub=issue URL /
AIセッション=session id / 画面キャプチャ=アプリ+ウィンドウタイトルの正規化。

---

## 6. コア体験の設計

### 6-1. ボタン一つで最適な返信

SLOは「提示150ms / 初トークン1s」。**押してから集める設計は不可**（CLAUDE.md）。

```
フォーカス変化イベント（既存のcontext cache更新経路）
  → 現在のスレッドを特定（thread_key）
  → Reply Context Pack を事前組み立て:
       ・スレッド直近N発言（全文）
       ・相手（people、名寄せ済み）と過去の口調サンプル
       ・関連するcommitments/open_loops（このスレッド/相手に紐づくもの）
       ・関連する過去スレッド（search_hybridで上位k）
  → キャッシュに常駐（押下時はストリーミング開始だけ）
押下
  → ドラフト生成（ストリーミング）→ L3承認キューへ（送信は必ず明示確認、不変条件4）
```

すでに `producer::propose` → ApprovalQueue → `RoutedSendTransport` の実行経路は通っているので、
**足りないのは「Reply Context Pack の事前組み立て」だけ**。

### 6-2. 「あの件どうなってる?」（参照解決）

これは検索より **参照解決（referent resolution）** が本体。

```
発話/入力 "あの件どうなってる?"
  1) 指示詞の検出（あの件/例の/さっきの/これ）
  2) 候補スレッドのランキング:
        salience = w1*直近性 + w2*未処理度(open_loop有) + w3*現在画面との一致
                 + w4*発話内の手がかり語とのFTS/ベクトル一致
  3) 候補が1つに絞れる → そのスレッドのContext Packで回答
     絞れない → 「Aの件? Bの件?」と2択で聞き返す（誤答より聞き返す）
  4) 回答は必ず根拠付き（provenance: どのメール/メッセージが根拠か）
```

**信頼度ゲートを回答にも適用**: 低信頼の状態は「〜の可能性」として弱く出す（既存の
`confidence::treat_fact` をそのまま使う）。

音声は CLAUDE.md 上 **v1.5スコープ**。テキスト入力で先に参照解決を作り込み、
ASRは後から前段に差すのが正しい順序（参照解決は音声/テキスト共通の資産）。

---

## 7. 実装計画

### Phase R1 — 検索をチャットに繋ぐ（最優先・最小コスト）
- [ ] `Db::assemble_context(query, budget)` を追加: `search_hybrid` + state facts + 現在画面 を
      RRFで統合し、トークン予算で切る
- [ ] `chat_blocking` を `inline_memory` から `assemble_context` に差し替え
- [ ] プロンプトに根拠（event id）を含め、回答に出典を付ける
- **効果**: 日本語FTSが効くので即座に「事実で答える」に変わる。埋め込み不要

### Phase R2 — 抽出の言語分離 + スレッド
- [ ] `extract.rs` の cue を言語別データに分離（英語が正典）。言語判定 → 該当セット適用
- [ ] 英語の cue/評価を先に強化（precision/recall を計測できる状態に）
- [ ] 日本語セットを**英語の指標を落とさないこと**を条件に追加
- [ ] `thread_key` マイグレーション + 各コネクタでのthread_key付与
- [ ] `threads` テーブル + salience 計算

### Phase R3 — 埋め込み（意味検索）
- [x] `search_hybrid` に query embedding を渡す配線（`Db::with_embedder` / `Db::embed_pending`）
      — モデル無しでは字面検索に degrade、モデルを載せた瞬間にハイブリッドになる
- [ ] **残: `ort` + `tokenizers` 依存追加と multilingual-e5-small の実搭載**
      モデル本体（数百MB）は**gitに入れない**。ビルド/パッケージ時に取得して .app に同梱する
      （CLAUDE.md「ローカルONNXモデル同梱」）。`Embedder` を実装して `with_embedder` に渡せば
      他の変更は不要

### Phase R4 — AIツール履歴の取り込み
- [ ] `ai_session` ソース + Claude Codeセッション(jsonl)リーダー
- [ ] Codex / ChatGPT の取り込み（形式調査 → リーダー実装）
- [ ] 取り込み対象のオプトイン/除外UI

### Phase R5 — コア体験
- [x] 参照解決（「あの件」）— `thread::is_referring` / `Db::resolve_referent`。
      曖昧なら**推測せず聞き返す**（チャットが候補を提示）
- [x] スレッド永続化（`threads` を書き込み経路で自動更新）+ 画面/語句/未処理/直近での順位付け
- [x] AIセッション取り込みのデスクトップ配線（オプトイン設定UI + 5分ポーリング）
- [x] 根拠を回答に渡す（`event_id`・出典付きで prompt に含む）
- [x] Reply Context Pack の事前組み立て — `Db::build_reply_context` をフォーカス変化時に
      キャプチャポーラが実行し `ReplyContextCache` に常駐。押下時は読むだけ（組み立てない）。
      組み立て所要時間を `build_ms` としてデータに同梱＝SLOを推測でなく計測できる
- [x] 根拠のUI表示 — 回答の下に出典チップ（`shogun_chat` が citations を返す）
- [x] SLO計測ハーネス（`crates/shogun-core/tests/context_slo.rs`）+ 初回実測

#### 実測値

**実機（Apple Silicon, MacBook Pro）40,000イベント / 400スレッド — 修正前**

| 経路 | p50 | p95 | max | 予算 | 判定 |
|---|---|---|---|---|---|
| `build_reply_context` | 0.1ms | 0.2ms | 0.7ms | 150ms | 余裕 |
| `resolve_referent` | 0.1ms | 0.1ms | 0.2ms | 150ms | 余裕 |
| `assemble_context` | 112ms | 286ms | **506ms** | 500ms | ❌ **max が予算超過** |

40k件の時点で最悪値が予算を超えていた。原因は `ORDER BY bm25(...)` が**一致した全行を
スコアリング**すること（返す件数ではなく、一致した量にコストが比例する）。

**対策: Warm窓（直近30日）を先に検索し、結果が薄い時だけ全履歴へ拡大**（3層メモリ設計通り）。

実装上の要点: `event_log` を JOIN して `ts` で絞っても**まったく速くならない**（計測で確認）。
SQLiteはMATCHを解決し全ヒットをスコアリングしてからでないとJOINで捨てられないため。
FTS5のpostingsはdocid順なので、**`rowid >=` 制約なら実際にスキップできる**。時刻の下限を
docidの下限に翻訳している（挿入順≒時刻順。バックフィルでズレるが、余分に古い行が入る側に
倒れるだけで、新しい行が落ちることはない）。

**開発機での効果（同一条件）**: p95 201ms → **132ms**、max 234ms → **144ms**

- [ ] **残: 修正後の実機再計測** — 上記の実機数値は修正前。Warm窓導入後の再測定が必要

### Phase S — セキュリティ強化（R1と並行可）
- [ ] SQLCipher 導入（鍵はKeychain）+ 既存DBのマイグレーション手順
- [ ] 書き込み前のsecretリダクション
- [ ] キャプチャ除外設定UI

### Phase Q — 精度の可視化
- [ ] 評価セット（質問→期待される根拠イベント）を作り recall@k を計測
- [ ] Dream Cycle（Batch分類）で低信頼候補を昇格・破棄、名寄せを実行

---

## 8. スコープ判断（確定 2026-07-25）

| 論点 | 決定 |
|---|---|
| 言語方針 | **英語で精度を出すのが最優先。日本語特化にはしない。** 多言語は英語品質を落とさない形で対応する |
| DB暗号化 | **v1でやる**（SQLCipher + Keychain鍵 + 既存DB移行） |
| AIツール履歴の取り込み | **v1でやる**（`ai_session` ソース） |
| 「ユーザーごと」 | **端末ごと**でよい。マルチテナントは作らない |
| 音声 | CLAUDE.md通り v1.5。参照解決をテキストで先に作り、ASRを後段で前置きする |

### 言語方針の実装への落とし方（重要）

「日本語も動くが、英語が主」。したがって:

- **抽出**: cue（手がかり語）は言語別の**データ**として持ち、言語判定して適用する。
  英語セットが正典で、チューニングと評価の主対象。日本語セットの追加が
  英語の precision/recall を動かさないこと（評価セットで担保）
- **検索**: FTS5 trigram は言語非依存でそのまま両対応
- **埋め込み**: multilingual-e5-small を使う（英語性能を保ちつつ多言語をカバー）。
  英語専用モデルに切り替えない
- **評価**: recall@k は**英語セットを主指標**、日本語セットは回帰チェックとして併走

---

## 付録: 主要な参照先

- 検索: `crates/shogun-memory/src/search.rs:105` (`search_hybrid`)
- チャット文脈: `crates/shogun-core/src/daemon.rs:248` (`inline_memory`)
- チャット実行: `apps/desktop/src-tauri/src/inline_source.rs:508` (`chat_blocking`)
- 取り込み: `crates/shogun-core/src/daemon.rs:139` (`ingest_capture`)
- 抽出: `crates/shogun-memory/src/extract.rs`
- 名寄せ: `crates/shogun-memory/src/identity.rs`（未配線）
- 埋め込み: `crates/shogun-memory/src/embed.rs`（モデル未搭載）
- 信頼度ゲート: `crates/shogun-fusion/src/confidence.rs`
