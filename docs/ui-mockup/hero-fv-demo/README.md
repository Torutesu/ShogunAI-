# Hero FV デモ — 触れる／録画できる1枚

Product Hunt ローンチ と LP ファーストビューのための、**操作できる**デモ兼動画素材。
機能の羅列ではなく、`docs/positioning-category-messaging.md` §1.4 の**5層構造**
（取得 → ワールドモデル → 実行 → 24時間実行 → 自己改善）を 15 秒で一周させる。
「単能アプリの束ではなく一つのループである」＝パーソナルAGIの主張が、そのまま画になる。

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
| `shogun-agi-loop.mp4` | **完成品のループ動画**（15.7s / 1512×982 / H.264 / 2.8MB） |
| `shogun-agi-loop-1200.mp4` | 同・幅1200版（1.2MB）。LP 埋め込みはこちら推奨 |

Artifact は単一ファイル・外部ホスト禁止（Google Fonts のみ可）なので、写真は
sibling asset ではなく data URI で入れている。`index.html` は生成物なので、
文言や演出を直すときは必ず `index.src.html` を直して `build.py` を回す。

## 5層 × 15.7秒ループ（2026-08-21 改訂・Product Hunt 版）

各シーンは製品の1機能ではなく、**アーキテクチャの1層**。順に見ると1本のループになる。

| # | シーン | 層 | 尺 | 画で言っていること |
|---|---|---|---|---|
| 01 | Acquire | L1 取得 | 2.2s | Mail / Calendar / Slack / Meeting が順に灯り「1,204 events today · all on this Mac」 |
| 02 | World model | L2 ハーネス | 2.6s | 生イベントが people 24 / projects 6 / commitments 9 / open loops 5 へ。行には confidence と `Why?`、低確度は事実に混ぜない |
| 03 | Act | L3 実行 | 3.8s | ⌥ キーでドラフト生成 → **L3 承認** → ✓ Sent ＋ Traceability |
| 04 | Overnight | L4 24時間 | 3.4s | 夕方のラップ → **画面が夜になり** Dream Cycle が走る → 夜が明けて Morning Brief |
| 05 | Learn | L5 自己改善 | 2.4s | 同方向の修正3回から1文のルールが蒸留され、以後の全ドラフトに効く |
| 06 | Mark | — | 1.3s | 暗転＋兜＋"Personal AGI — scoped to your work. On your Mac."（**継ぎ目はこの暗転の中**） |

### 画面に「L1〜L5」と出していない理由

製品UIの `L1 / L2 / L3` は**権限レベル**（自動実行 / ワンタップ / 明示承認）であり、
`CLAUDE.md` 不変条件4 の根幹。ここに5層構造の L1〜L5 を重ねると1つの記号に2つの意味が乗り、
**送信を統べる言語が壊れる**。よってシーン名は Acquire / World model / Act / Overnight / Learn
とし、画面内のタグは権限レベルのみ。レール（操作UI）にだけ層名が出る。

### L5 の中身は実装どおり

`crates/shogun-memory/src/lessons.rs` を典拠にしている:
同方向の修正が **3回**（`MIN_RULE_OCCURRENCES`）で候補になり、confidence は Medium から始まり
corroboration 上限 0.75 で **High には決して届かない**。デモの一文
"Keep drafts significantly shorter; …" は同ファイルの実テンプレート。
「offered, never asserted」はコピーの飾りではなく実装の性質。

### 「パーソナルAGI」の扱い（§6 の規律）

`docs/positioning-category-messaging.md` §6 では、この語は
**ローンチ告知では可・断定形・ただし定義とセット**、LPヒーローでは使わない、と決まっている。
エンドカードは既定で Product Hunt 版（`Personal AGI — scoped to your work. On your Mac.`＝
定義そのものを併記した形）。**LP ヒーローに同じ動画を使うときは `?end=lp`** を付けて撮り直すと
`Your AI has memory. Now it acts.` に差し替わる。禁じ手（"AGI is here" 型の到達宣言、定義なしの
単独使用）はどちらにも入れていない。

## 録画のしかた

**完成品が同梱してある**: `shogun-agi-loop.mp4`（15.7s / 1512×982、継ぎ目なし）と
幅1200の軽量版。そのまま `<video autoplay muted loop playsinline>` で貼れる。

自分で撮り直す場合:

1. Artifact を開く（既定で **Auto-loop**）。`C` → Clean mode、`100%` → 等倍
2. 収録範囲は**ステージの矩形だけ**（レールは外にあるので写り込まない）
3. 1ループ 15.7 秒。継ぎ目を隠すなら **Mark の暗転中に開始・終了**する
   （同梱 mp4 は暗転の中心どうしで切ってある）
4. ヘッドレス再録するなら `?capture=1` で 1512×982 ぴったりのステージのみになる。
   **注意**: Playwright の recordVideo は負荷でタイムラインが約1.1倍に伸びる。
   同梱 mp4 は `setpts=0.907*PTS` で意図した尺へ戻してある（伸びたまま出さないこと）
5. FV が 16:9 なら **16:9 guide** で切れる上下を先に確認

| キー | |
|---|---|
| `Space` | 頭から再生 |
| `C` / `Esc` | Clean mode の出入り |
| `←` `→` | シーンを1つずつ送る |
| `A` | Auto-loop の切り替え |

手で触ると Auto-loop は自動で外れる（勝手に進んで撮り損ねないため）。
ノッチをクリックすると頭出し、`Confirm & send` で送信 — 触れる箇所は実装と同じものだけ。

## 直してよいもの／だめなもの

- ✅ シナリオのダミーデータ（Aiko / 12k / 14日）、シーンの尺、壁紙
- ❌ **L1 / L2 / L3 のタグと色** — 権限表示はトーンや尺で消してよいものではない
- ❌ `nothing sent` / `1 message left this device` の2行 — 「勝手に動くが外には出ない」が
  このデモの主張そのもので、これを外すと別プロダクトの宣伝になる
- ❌ パネル寸法 560px と展開 100ms — SLO と実装に連動（`CLAUDE.md`）
- ❌ **画面内に L1〜L5 の層番号を出すこと** — 権限レベルの L1/L2/L3 と衝突する（上記）
- ❌ 「パーソナルAGI」を定義なしで単独使用すること（§6 禁じ手）

文言は `apps/desktop/src/strings.ts` に対応するものがある限りそちらを正とする。
`WHILE YOU SLEPT` / `READY FOR YOU` の2見出しだけは LP 向けの追加コピーで、
実装には存在しない。
