# Hero FV デモ — 触れる／録画できる1枚

LP のファーストビューに貼る動画素材を作るための、**操作できる**デモ。
Figma の `11 · Hero / FV` フレームを HTML に起こしたもので、静止画ではなく
「ノッチが開いて、押したら送信が終わる」までを実際に触れる。

- 公開 Artifact: <https://claude.ai/code/artifact/cfe0b7e5-24ca-453c-87fd-3794e616f15e>
- ローカルで開く: `python3 build.py` → `index.html` をブラウザで開く
- 対応 Figma: `Product Flow — in a Mac` ページの `11 · Hero / FV`

## ファイル

| | |
|---|---|
| `index.src.html` | **正本。** 編集はここだけ。壁紙は `__WALL__` プレースホルダ |
| `build.py` | 壁紙を data URI で埋め込んで `index.html` を吐く |
| `index.html` | 生成物（自己完結・外部リクエストなし）。直接編集しない |
| `wallpaper.jpg` | Sand Harbor, Lake Tahoe。Wikimedia Commons (CC BY-SA 4.0) を FV 比率にクロップ・縮小 |

Artifact は単一ファイル・外部ホスト禁止（Google Fonts のみ可）なので、写真は
sibling asset ではなく data URI で入れている。`index.html` は生成物なので、
文言や演出を直すときは必ず `index.src.html` を直して `build.py` を回す。

## 5つのビート

シーケンスはそのまま製品の主張になっている。レール中央のビート名をクリックすると
その瞬間に飛べる（録画で特定カットだけ撮り直すとき用）。

| # | ビート | 何を見せているか |
|---|---|---|
| 01 | Idle | ノッチは黒いチン。`reading Mail` と件数だけ。通知ではない |
| 02 | Brief lands | 兜マークとブランドブルーのグロー。**赤丸を出さない**（`docs/daily-summaries-design.md` §3.1） |
| 03 | Opens | パネルがノッチから注がれる。夜間レビューの既了分が L1 で並び、`1,204` はカウントアップ |
| 04 | You confirm | L3 が「送信中」へ。**押すまで何も出ていない**ことがこの1拍で分かる |
| 05 | Sent | 行が緑に変わり、`1 message left this device · logged in Traceability` が降りてくる |

タイミングは実装の実値を使っている（展開 100ms / 収納 140ms / 内容 72ms、
`styles.css` の `--notch-open` 系）。演出のために速くも遅くもしていない。

## 録画のしかた

1. Artifact を開く（既定で **Auto-loop** が走っている）
2. `C` を押す → **Clean mode**。操作レールとヒントが消え、暗い部屋にMacだけが残る
3. `100%` を押すと 1512×982 の等倍になる。等倍で撮るとピクセルが眠くならない
4. 画面収録の範囲は **ステージの矩形だけ**にする。レールはステージの外にあるので、
   矩形さえ合わせれば操作UIは絶対に写り込まない
5. 1ループは約 14 秒。`Space` で頭から採り直せる
6. FV が 16:9 の場合は **16:9 guide** を出して、切れる上下を確認してから撮る

| キー | |
|---|---|
| `Space` | 頭から再生 |
| `C` / `Esc` | Clean mode の出入り |
| `←` `→` | ビートを1つずつ送る |
| `A` | Auto-loop の切り替え |

手で触ると Auto-loop は自動で外れる（勝手に進んで撮り損ねないため）。
ノッチのホバーで peek、クリックで展開、`Confirm & send` で送信、L2 行のクリックで
prep 完了 — 触れる箇所は実装と同じものだけにしてある。

## 直してよいもの／だめなもの

- ✅ シナリオのダミーデータ（Aiko / 12k / 14日）、ビートの尺、壁紙
- ❌ **L1 / L2 / L3 のタグと色** — 権限表示はトーンや尺で消してよいものではない
- ❌ `nothing sent` / `1 message left this device` の2行 — 「勝手に動くが外には出ない」が
  このデモの主張そのもので、これを外すと別プロダクトの宣伝になる
- ❌ パネル寸法 560px と展開 100ms — SLO と実装に連動（`CLAUDE.md`）

文言は `apps/desktop/src/strings.ts` に対応するものがある限りそちらを正とする。
`WHILE YOU SLEPT` / `READY FOR YOU` の2見出しだけは LP 向けの追加コピーで、
実装には存在しない。
