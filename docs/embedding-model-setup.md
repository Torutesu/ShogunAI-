# ローカル埋め込みモデルのセットアップ

意味検索（言い換え・同義語に効く検索）を有効にするための手順。**モデルが無くても製品は動く** —
検索は字面一致（FTS）のみになる。

- 対象: multilingual-e5-small（ADR-001）/ 384次元 / JA+EN / 完全オンデバイス
- 前提: モデル本体（約470MB）は **gitに入れない**。ビルド時に取得し、.appに同梱する

---

## 1. モデルの取得

```bash
./scripts/fetch-embedding-model.sh
```

`models/multilingual-e5-small/` に `model.onnx` と `tokenizer.json` を配置する（`models/` は
gitignore 済み）。既にあるファイルはスキップするので、再実行は安全。

## 2. ONNX Runtime 本体

`ort` は **実行時ロード**（`load-dynamic`）で使っている。ビルド時にダウンロードしないので、
ビルドがネットワークに依存しない代わりに、`libonnxruntime.dylib` が実行時に必要。

- 開発時: `brew install onnxruntime` が最も手軽
- 配布時: `.app` の `Frameworks/` に同梱し、`ORT_DYLIB_PATH` で指す

## 3. 開発中の起動

```bash
export SHOGUN_EMBED_MODEL=$PWD/models/multilingual-e5-small/model.onnx
export SHOGUN_EMBED_TOKENIZER=$PWD/models/multilingual-e5-small/tokenizer.json
cd apps/desktop && cargo tauri dev
```

起動ログで確認:

| ログ | 意味 |
|---|---|
| `[embed] local model loaded — hybrid search enabled` | 意味検索が有効 |
| `[embed] no local model — search stays lexical` | モデル未配置（正常。字面検索のみ） |
| `[embed] model present but failed to load (…)` | **異常**。ファイルはあるが使えない |
| `[embed] embedded N event(s)` | バックログを埋め込み中 |

## 4. モデルが正しく動いているかの確認

```bash
SHOGUN_EMBED_MODEL=$PWD/models/multilingual-e5-small/model.onnx \
SHOGUN_EMBED_TOKENIZER=$PWD/models/multilingual-e5-small/tokenizer.json \
cargo test -p shogun-memory --features onnx -- --ignored --nocapture
```

「質問に対して、答えを含む文が無関係な文より高い類似度になる」ことを検証する。ここが通らなければ
モデルかトークナイザの不整合。

## 5. 設計上の注意（静かに精度が落ちる箇所）

実装で押さえている3点。いずれも**エラーにならず精度だけ落ちる**ので、変更時は注意:

1. **e5のロール接頭辞** — `query:` / `passage:` を付けないと精度が明確に落ちる
2. **attention maskを使った平均プーリング** — パディングを平均に混ぜると短文がパディング
   ベクトルに引きずられる
3. **L2正規化** — コサイン類似度とベクトルストアの距離が単位ベクトルを前提にしている

## 6. 埋め込みの動作

- 書き込み経路では**やらない**（FR-MEM-22）。バックグラウンドジョブが少しずつ処理する
- 遅れても問題ない — 未埋め込みのイベントも**FTSでは見つかる**。言い換えで引けないだけ
- 推論スレッドは1本に固定（アイドルCPU 5%のSLOがあるため、キャプチャやUIとコアを奪い合わない）
