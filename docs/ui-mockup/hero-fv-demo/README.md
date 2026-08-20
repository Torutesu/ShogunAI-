# Hero FV デモ — 触れる／録画できる1枚

LP のファーストビューに貼る動画素材を作るための、**操作できる**デモ。
Figma の `11 · Hero / FV` フレームを HTML に起こしたもので、静止画ではなく
⌥ドラフト送信・朝夕ブリーフ・会議翻訳・検索を15秒ループで流し、実際に触れる。

- 公開 Artifact: <https://claude.ai/code/artifact/cfe0b7e5-24ca-453c-87fd-3794e616f15e>
- ローカルで開く: `python3 build.py` → `index.html` をブラウザで開く
- 対応 Figma: `Product Flow — in a Mac` ページの `11 · Hero / FV`

## ファイル

| | |
|---|---|
| `index.src.html` | **正本。** 編集はここだけ。壁紙は `__WALL__` プレースホルダ |
| `build.py` | 壁紙を data URI で埋め込んで `index.html` を吐く |
| `index.html` | 生成物（自己完結・外部リクエストなし）。直接編集しない |
| `wallpaper.jpg` | 本物の macOS Sequoia Light 標準壁紙（6K 原本を FV 比率にクロップ・縮小） |
| `shogun-fv-loop.mp4` | **完成品のループ動画**（16.5s / 1512×982 / H.264）。LP にそのまま貼れる |

Artifact は単一ファイル・外部ホスト禁止（Google Fonts のみ可）なので、写真は
sibling asset ではなく data URI で入れている。`index.html` は生成物なので、
文言や演出を直すときは必ず `index.src.html` を直して `build.py` を回す。

## 6シーン・15秒ループ（2026-08-20 改訂）

FV 用に「機能をテンポよく」へ再構成。壁紙は**本物の macOS Sequoia 標準壁紙**、
Dock はオーナーの実際の並び（スクショから再現、実行中インジケータ・Settings のバッジ含む）。

| # | シーン | 尺 | 何を見せているか |
|---|---|---|---|
| 01 | Morning brief | 3.0s | idle → 兜グロー → パネルが開き夜間レビューの既了分（`1,204` カウントアップ） |
| 02 | ⌥ Draft → Send | 4.0s | Mail の返信に ⌥ キーキャップが光り、ドラフトがタイプされ、L3 確認 → ✓ Sent + Traceability トースト |
| 03 | Live translation | 2.5s | 会議キャプション EN→JA がリアルタイムに流れる（黒カプセル＋波形） |
| 04 | Memory search | 2.0s | `/` 検索に `vendor renewal`、Mail/Meeting/Slack 横断の結果が即答 |
| 05 | Evening wrap | 2.5s | Good evening、3 done · 2 loops · 1/2 adopted のカウント |
| 06 | Mark | 1.0s | 暗転＋兜＋"Your AI has memory. Now it acts." — **ループの継ぎ目はこの暗転の中** |

パネルの展開 100ms / 収納 140ms は `styles.css` の実値のまま。

## 録画のしかた

**完成品が同梱してある**: `shogun-fv-loop.mp4`（16.5s / 1512×982 / H.264 / 約240KB、
継ぎ目なしループ）。LP にはこれをそのまま `<video autoplay muted loop playsinline>` で貼れる。

自分で撮り直す場合:

1. Artifact を開く（既定で **Auto-loop**）。`C` → Clean mode、`100%` → 等倍
2. 収録範囲は**ステージの矩形だけ**（レールは外にあるので写り込まない）
3. 1ループ 15 秒。継ぎ目を隠すなら **Mark の暗転中に開始・終了**する
4. ヘッドレス再録するなら `?capture=1` を付けると 1512×982 ぴったりの
   ステージのみ表示になる（Playwright の recordVideo で撮ったのが同梱 mp4）
5. FV が 16:9 なら **16:9 guide** で切れる上下を先に確認

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
