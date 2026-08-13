# 会議テキストを検索の背骨に載せるか（FR-MT-14 設計判断）

- **起票**: 2026-08-13（監査 Phase 5-4）
- **状態**: **オーナー判断待ち。実装していない**
- **関連**: FR-MT-14 / FR-MTUX-02 / FR-MEM-23 / 不変条件3・5 / `docs/audits/2026-08-13.md` §8

取り込まなかった `shogunai-core-features` ブランチに `meeting_index.rs` があり、会議テキストを
`event_log` に流し込む設計が実装済みだった。**これは「検索を1本足す」話ではなく、会議の全文が
Dream Cycle 経由でクラウドへ出るかどうかの判断**なので、移植せずここに論点を書く。

---

## 1. 現状（main）

会議テキストは専用の検索経路を持つ。

```
meeting_recaps / transcript_segments  ──→  search::search_meetings()  ──→  会議検索UI
event_log ──→ event_fts ──→ search_hybrid / Fusion / extraction / context pack / Dream Cycle
```

`search_meetings` は語彙検索としては動く（テスト5本あり、recap 優先・transcript フォールバック、
最新ではなくクエリ関連のセッションを返す）。**が、会議テキストは以下のどこにも乗っていない**:

| 経路 | 会議テキストが効くか |
|---|---|
| `search_hybrid`（FTS＋ベクトル） | ❌ |
| Context Fusion の候補生成 | ❌ |
| Dream Cycle の抽出（commitments / people / open_loops） | ❌ |
| `memory.get_context_pack`（FR-API-08） | ❌ |
| Warm/Cold の階層化・量子化 | ❌（会議テーブルは階層外） |

つまり「先週の会議で誰が何を約束したか」は**state tables に入らない**。会議で生まれた
コミットメントは、Slack や Gmail で同じことを言った場合には拾われるのに、会議で言った場合は
拾われない。FR-MT-14 が「既存の検索・抽出・Fusion がそのまま効く」と書いているのは
この非対称を潰せという要求であり、現状は満たしていない。

## 2. ブランチの設計（案A: 背骨に載せる）

セッション終了時（Wrapping → Recap）に一度だけ、transcript と user note を
`event_log` へ `source = 'meeting'` で書く。

```
transcript_segments ─┐
                     ├─→ index_session() ─→ event_log(source='meeting') ─→ 既存の全経路
session_notes ───────┘
```

- 話者は判明時のみ `Me: ` / `Them: ` を前置。`Unknown` は NULL のままで、
  「Someone:」のような**推測を書かない**（FR-MT-15）
- `insert_or_touch` なので同じテキストでの再実行は dedup タッチ。
  再文字起こし（テキストが変わる）は前の行を消してから、という制約付き

**これで §1 の表が全部 ✅ になる。** connector lane が Gmail を
`ingest_integration` で正規化して log に載せるのと同じ動き — 背骨は1本、供給源は複数。

## 3. 代償（ここが判断ポイント）

`event_log` に入るということは、**`event_log` を読む全経路に入る**ということ。
そのうち1本はローカルではない。

```rust
// crates/shogun-core/src/dreamcycle/jobs.rs
fn consolidate(&self, from_ts: i64, to_ts: i64) -> Result<(), String> {
    let events = self.db.events_in_range(from_ts, to_ts);   // ← source で絞っていない
    for (event_id, cands) in self.classifier.classify(&events) { ... }
```

`events_in_range` は `SELECT id, content FROM event_log WHERE ts >= ?1 AND ts < ?2` で、
**source フィルタが無い**。classify は Batch レーン（Select KK キー／中継経由）へ
チャンクを送る。したがって案Aをそのまま入れると:

> **会議の全文文字起こしが、毎晩 Dream Cycle で運営の中継サーバー経由で Anthropic へ送られる。**

これは不変条件3（生データはデバイス外に出さない）に対する**新しい**egress であり、
2026-08-05 に受容した Deepgram の例外とは別物である:

| | Deepgram 例外（受容済み） | 案Aが増やすもの |
|---|---|---|
| 何を送るか | 音声（ライブ STT のため） | **確定した全文テキスト** |
| いつ | 会議中、その場限り | 毎晩、蓄積分すべて |
| 用途 | 文字起こし（process-only、`mip_opt_out=true`） | 分類・抽出 |
| 開示 | UI に明示済み | **未開示** |

Deepgram の同意を取ったユーザーが、これにも同意したことにはならない。

## 4. 選択肢

### 案A-1: そのまま載せる（ブランチのまま）
- ✅ FR-MT-14 を完全に満たす。実装は移植のみ
- ❌ 上記の未開示 egress が発生する。**このままの実装は入れられない**

### 案A-2: 載せるが、Batch レーンから会議を除外する（推奨）
`events_in_range` に source 除外を足し、`source = 'meeting'` の行は
Dream Cycle の classify に渡さない。抽出はローカル抽出（`extract.rs`）に限定する。

- ✅ 検索・Fusion・context pack・階層化は全部効く
- ✅ 新しい egress ゼロ。開示の追加不要
- ⚠️ 会議からの commitments 抽出精度はローカル抽出の水準に落ちる
- ⚠️ **除外は構造で守る必要がある。** `events_in_range` を「Batch に渡す用」と
  「ローカル用」に型で分けないと、将来 source を足した誰かが黙って穴を開ける。
  `check-http-egress.py` と同じ発想のガードを1本足すのが望ましい

### 案A-3: 載せるうえで、Batch へ送るかを opt-in にする
Composio / Deepgram と同じ3開示 opt-in を会議の Batch 分類にも付ける。

- ✅ 精度を取りたいユーザーは取れる
- ❌ 同意面がまた1つ増える。Wave 1 の複雑さに見合うか疑問

### 案B: 載せない（現状維持）
- ✅ 判断不要
- ❌ FR-MT-14 未達のまま。「会議で決めたことが SHOGUN の状態に入らない」という
  プロダクトの穴が残る。**記録ツールではなく状態推定のプロダクト**という定義と正面から衝突する

## 5. 推奨

**案A-2。** FR-MT-14 の要求は「検索・抽出・Fusion が効くこと」であって「クラウドで分類すること」
ではない。背骨に載せる価値のほとんど（検索・Fusion・context pack・state 抽出の入口）は
ローカルだけで取れる。クラウド分類の精度分だけを、開示のない egress と引き換えにするのは
割に合わない。

案A-3 は、A-2 を出して「会議からの抽出が弱い」という実使用のフィードバックが出てから
検討すればよい。順序を逆にすると、必要か分からない同意面を先に作ることになる。

## 6. 実装するときの作業（案A-2）

1. `crates/shogun-memory/src/meeting_index.rs` を移植（ブランチの
   `1f364cc` / `4314562`）。`SOURCE = "meeting"`、`transcript_body` の話者前置ルールはそのまま
2. **`events_in_range` を分割する**: `events_in_range_for_local` と
   `events_in_range_for_batch`（後者は `source NOT IN ('meeting')`）。
   呼び出し側が選ぶのではなく、型/関数名で選べないようにする
3. Dream Cycle の `consolidate` / `morning_brief` の summarize 経路を batch 版に差し替え
4. セッション終了（Wrapping → Recap）から `index_session` を1回だけ呼ぶ
5. `search_meetings` の扱いを決める: 背骨に載れば `search_hybrid` で引ける。
   会議UI専用の絞り込みとして残すか、削るか
6. ガード: `scripts/` に「Batch レーンへ渡る event 取得経路が source を絞っているか」を
   見る check を追加。除外を人間の記憶に頼らせない（Phase 2 と同じ方針）
7. 再文字起こし（WS9）が来たとき、`index_session` の再実行は前の行を消してから、を
   呼び出し側の契約として明記

**2 と 6 を省略して 1 だけ入れてはならない。** それは案A-1 であり、
未開示の egress をそのまま出荷することになる。
