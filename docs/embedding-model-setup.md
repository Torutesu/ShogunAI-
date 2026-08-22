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
- 配布時: **`.app` に同梱済み**。`pnpm bundle:assets`（`tauri.conf.json` の `beforeBuildCommand`
  が呼ぶ）が `scripts/fetch-onnxruntime.sh` でダウンロードし、`bundle.resources` が
  `Contents/Resources/libonnxruntime.dylib` に置く

> **バージョンは選べない。** `ort 2.0.0-rc.10` は `ORT_API_VERSION = 22`（`ort-sys/src/lib.rs`）
> でコンパイルされており、これは ONNX Runtime **1.22.x** を指す。ヘッダより古いランタイムは
> `OrtApi` に null を返し、セッション構築が失敗する。`ort` を上げるときは
> `scripts/fetch-onnxruntime.sh` の `VERSION` も一緒に上げること。

**場所は自動で探す**ので、通常は環境変数を設定する必要はない。探索順:

1. `ORT_DYLIB_PATH`（明示指定が常に最優先。ただし指定先が存在しなければエラーになる）
2. `.app` 内（`Contents/Frameworks/` → `Contents/Resources/`）※実行ファイルが `Contents/MacOS/`
   にあるときだけ。同梱物がシステムより先に来るので、ユーザーが無関係に `brew install onnxruntime`
   していても、配布ビルドが検証済みバージョン以外を掴むことはない
3. `/opt/homebrew/lib/`（Apple Silicon の Homebrew）
4. `/usr/local/lib/` → `/opt/local/lib/` → `/usr/lib/`

すべて `OnnxEmbedder::load` の中で探すので、**アプリでもテストでも同じように効く**。3以降は
開発機のパスしかないので、2 が無かった頃の配布ビルドは意味検索が必ずOFFだった。

> Apple Silicon の注意: `dlopen` はライブラリ名だけ渡されると `/usr/local/lib` と `/usr/lib` しか
> 探さず、Homebrew の `/opt/homebrew/lib` は見に行かない。`brew install onnxruntime` しただけでは
> 見つからないため、上記の探索を入れてある。
>
> さらに `ort` は**ライブラリが見つからないとpanicする**（Resultを返さない）。そのままだと
> 「モデルが無ければ字面検索に落ちる」という設計が成立せず、配布版で起動時クラッシュになる。
> ort に触る前に存在確認して `Err` を返すようにしてある。

見つからない場合のエラー文言:

```
libonnxruntime.dylib not found (looked in /opt/homebrew/lib, /usr/local/lib, /opt/local/lib, /usr/lib)
 — install the ONNX Runtime (`brew install onnxruntime`) or set ORT_DYLIB_PATH
```

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
