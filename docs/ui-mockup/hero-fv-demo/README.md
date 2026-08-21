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
| `dock.png` | **任意。置けば実物の Dock に差し替わる**（後述）。無ければ下記の混成版が使われる |
| `logos/` | 実ブランドマーク。App Store の公開アートワーク20個＋Commons の SVG 3個。出典と再取得は `logos/SOURCES.md` / `logos/fetch_appstore.py` |
| `shogun-hero-mac.mp4` | **FV用の完成品**。閉じた状態から開く（32.5s / 1700×1280 / 4.6MB） |
| `shogun-hero-mac-1200.mp4` | 同・幅1200版（1.7MB）。**LP 埋め込みはこれ** |

Artifact は単一ファイル・外部ホスト禁止（Google Fonts のみ可）なので、写真は
sibling asset ではなく data URI で入れている。`index.html` は生成物なので、
文言や演出を直すときは必ず `index.src.html` を直して `build.py` を回す。

## MacBook Pro の筐体

画面だけを浮かせると「モック画像」に見えるので、**MacBook Pro 14" の筐体ごと**描いている。
`Mac frame` ボタン（キー `F`、URL は `?frame=0`）で画面だけの表示にも戻せる。

**ループは閉じた状態から始まる。** 開く → 仕事する → 閉じる → 開く。継ぎ目が
「何も動いていない閉じた筐体」の中に落ちるので、フェードを突き合わせる従来のやり方より
はるかに切りやすく、絵としても始まりと終わりがある。

寸法は装置の実寸から出している。ディスプレイは 1512×982pt で、これはパネルの
3024×1964 / 254ppi＝**302.4 × 196.4mm** ちょうど。つまり**このデモの 5px = 1mm**。
公表されている筐体サイズ 312.6 × 221.4mm を同じ縮尺に置くと蓋は 1564 × 1108px、
side bezel は自動的に 5.2mm になる。上ベゼルは左右と同じ幅にしてある（ノッチが
上ベゼルを"切り上がる"形なので実機もそう見える）。その結果、顎に残るのが約20mm。

**物理ノッチ（185 × 32px）はこのプロジェクトの実測値**。`docs/phase0-findings.md` に
残っている実機ログ `notch_w=220 notch_h=38 @ screen=1800x1169` を 1512 幅の
スケーリングに直すと `220 × 1512/1800 = 185`、`38 × 1512/1800 = 32` になる。
後者はアプリの `--notch-dead-h:32px` そのもの。ノッチは 1512×982 の**内側**に置いている
（macOS がその座標系で報告するため。メニューバーの上ではなく中にある）。
SHOGUN の chin はこれを必ず覆うよう `min-width:185px` を入れた。

目分量なのは2つだけ: **角丸**（同心を保って inner + bezel = outer にしてある）と、
**天板を蓋よりわずかに広く**描いていること。実機は同幅だが、蓋が後ろに倒れている以上
真正面の完全同幅は額縁に見える。

## 蓋の開閉

蓋は自分の下辺（＝天板が始まる線）を軸に `rotateX` で回している。ヒンジの位置を別途
でっち上げる必要がない。**閉が +90deg**（奥へ倒れる）で、手前へ倒す -90deg ではない:
-90 だと閉じた蓋が手前に張り出して画面（壁紙）が上向きに大写しになる。+90 なら少し上から
見下ろした天板として収まる。`perspective-origin` を 84% と低めに置いて、その天板が
薄く見えるようにしてある。

蓋が真横を向いているあいだは面積がゼロなので、**閉じた筐体の厚み（約6mm）は `.shut` が
描いている**。開き始めの一拍で本物の蓋に受け渡す。ディスプレイは開きながら点く
（`.scroff` が蓋の途中で抜ける）。実機がそう振る舞うため。

実装上つまずいた点を3つ残しておく:

1. **状態クラスに `.shut` を使ってはいけない。** `<body class="shut">` は要素側の
   `.shut{position:absolute;height:30px}` に**自分がマッチしてしまい**、body ごと
   30px の絶対配置になって筐体が画面外へ飛ぶ。状態は `body.lidshut`
2. **CSS custom property 経由の transition は使わない。** `--lidx` と `--lidms` を同時に
   変える書き方だと、Chromium で約250ms止まってから動き出す
3. **Web Animations でも「今から回せ」では遅い。** 90deg の蓋は画面上の面積がゼロで、
   コンポジタがラスタライズを済ませていないため約190ms食う。`delay` 付きで**前もって
   宣言**し（`fill:'backwards'`）、同期させたいものは `animation.ready` にぶら下げる。
   `setTimeout` で並べると、蓋が動く前に `.shut` だけが消える

## Dock のアイコン

**34枚中23枚が本物**。内訳と取得方法は `logos/SOURCES.md`、再取得は `logos/fetch_appstore.py`。

- **App Store の公開アートワーク（20）** — Safari / Messages / Mail / Maps / Photos / FaceTime /
  Calendar / Contacts / Reminders / Notes / Music / Freeform / Keynote / Numbers / Pages /
  Slack / LINE / Notion / Discord / Raycast。`itunes.apple.com/search` から 512px を取り、
  128px に落として **macOS の squircle（superellipse n=5）でマスク**して埋めている
- **Wikimedia Commons の SVG（3）** — Chrome / Gemini / Figma

**まだ描画版なのは 11 枚**: Finder / Launchpad / App Store / System Settings / ゴミ箱
（macOS にバンドルされていてどのストアにも無い）、Arc / Warp / Terminal、および Dock 内の
未同定の数枚。形は寄せてあるが本物ではない。

Calendar だけは注意: App Store のアートワークは日付が焼き込み（`Tue 1`）で、実機の
macOS は当日の日付を描く。デモでは静止画として扱っている。

全部を本物にしたい場合は次項でスクショごと差し替える。

## Dock を実物のスクショに差し替える

アイコンを手で描くと必ずどこか嘘になるので、**自分の Dock を撮った PNG を置けばそれが使われる**。

1. macOS で **⇧⌘4 → Space → Dock をクリック**
   （ウィンドウキャプチャ。これだと**背景が透明**の PNG になる。範囲選択 ⇧⌘4 のドラッグだと
   Dock の裏の壁紙が四角く焼き込まれるので**使わない**）
2. 撮れた PNG を `dock.png` としてこのフォルダに置く
3. `python3 build.py` を回す

`build.py` が透明マージン（キャプチャに入る影の余白）を自動でトリムして data URI で埋め込み、
描画版の Dock は自動的に隠れる。幅は CSS 変数 `--dockw`（既定 1180px）で調整できる。

`dock.png` はリポジトリに入れていない（各自の Mac のもので、内容も人によって違うため）。
差し替えたまま録画したいときだけ置けばよい。

## 11シーン × 32.5秒ループ（2026-08-21 改訂・Product Hunt 版）

5層構造（取得 → ワールドモデル → 実行 → 24時間実行 → 自己改善）を背骨に、
実行の一手を**受信メールから送信まで通しで**見せる。機能の羅列ではなく、
1通のメールが片づくまでの30秒。

| # | シーン | 層 | 尺 | 画で言っていること |
|---|---|---|---|---|
| 01 | Open | — | 1.6s | 閉じた MacBook が開き、開きながら画面が点く |
| 02 | Acquire | L1 | 2.6s | Mail / Calendar / Slack / Meeting が灯り「1,204 events today · all on this Mac」 |
| 03 | World model | L2 | 3.1s | 生イベント→ people 24 / projects 6 / commitments 9 / open loops 5。confidence と `Why?`、低確度は事実に混ぜない |
| 04 | Recall | L2 | 2.4s | `/vendor renewal` に Mail / Meeting / Slack 横断で即答 |
| 05 | Read | — | 3.0s | **Aiko からの実際の受信メール全文**。12k・Friday 17:00・14日がハイライトされる |
| 06 | Draft (⌥) | L3 | 4.2s | ⌥ を押すとスレッド内に返信ブロックが開き、本文がタイプされる。根拠チップ3つ付き |
| 07 | Approve | L3 | 2.8s | 「Leaves this Mac — nothing sends without you」→ Confirm & send → ✓ Sent + Traceability |
| 08 | Meeting | — | 4.0s | 会議検知カード → Take Notes → EN→JA ライブ翻訳 |
| 09 | Overnight | L4 | 4.4s | 夕方のラップ →**画面が夜になり** Dream Cycle → 夜明けに Morning Brief |
| 10 | Learn | L5 | 2.8s | 同方向の修正3回から1文のルールが蒸留され、以後の全ドラフトに効く |
| 11 | Mark | — | 1.6s | 暗転＋兜＋"Personal AGI — scoped to your work. On your Mac." **のまま蓋が閉じる**（継ぎ目はこの閉じた状態の中） |

### メール本文は「読める文章」で書く

デモの説得力はここで決まるので、受信・返信とも実務の文章として書いてある。
受信（Aiko）が **12k / 14.4k からの値下げ / 座席数維持 / 金曜17時の締切 / 14日への延期 /
デッキ差し替え依頼 / ベンダースレッドは Aiko が持つ** を提示し、
返信はその**全部に具体的に答える**（承認・procurement への連絡・書面確認・デッキの条件）。
Lorem や「〜について承知しました」のような中身のない文にしない。返信が受信の固有名詞を
拾っていない動画は、記憶を持つ製品の証明にならない。

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

**完成品が同梱してある**: `shogun-hero-mac.mp4`（32.5s / 1700×1280、継ぎ目なし）と
幅1200の軽量版。そのまま `<video autoplay muted loop playsinline>` で貼れる。

自分で撮り直す場合:

1. Artifact を開く（既定で **Auto-loop**）。`C` → Clean mode、`100%` → 等倍
2. 収録範囲は**ステージの矩形だけ**（レールは外にあるので写り込まない）
3. 1ループ 32.5 秒。**切るのは蓋が閉じているあいだ**（同梱 mp4 はその中心どうしで
   切ってある）。静止しているので数フレームずれても継ぎ目が出ない
4. ヘッドレス再録するなら `?capture=1`。ビューポートは**筐体ありなら 1700×1280、
   `&frame=0` の画面だけなら 1512×982**（`window.__capSize` に装置の実寸が出る）。
   **1周目は捨てる**: 蓋のレイヤを初めてラスタライズする分、開くのが約0.3s遅れる
   **注意**: Playwright の recordVideo は負荷でタイムラインが約1.1倍に伸びる。
   同梱 mp4 は `setpts` で意図した尺へ戻してある（今回は 1.17841）。
   **係数は撮るたびに変わるので毎回出し直すこと。** ページ側の実測ループ長は
   `window.__loops`（32.52s）。録画側のループ長は、フレームを 64×48 のグレーに落として
   **動きのある区間で自己相関の最小点**を探す。閉じている区間をアンカーにすると、そこは
   全フレームが同一なのでどのオフセットでも同じスコアになり谷が出ない。
   `setpts = 32.52 ÷ 録画側ループ長`、切り出しは相関で出たフレーム数ちょうどで
5. FV が 16:9 なら **16:9 guide** で切れる上下を先に確認

| キー | |
|---|---|
| `Space` | 頭から再生 |
| `C` / `Esc` | Clean mode の出入り |
| `←` `→` | シーンを1つずつ送る |
| `A` | Auto-loop の切り替え |
| `F` | MacBook の筐体あり／画面だけ |

手で触ると Auto-loop は自動で外れる（勝手に進んで撮り損ねないため）。
ノッチをクリックすると頭出し、`Confirm & send` で送信 — 触れる箇所は実装と同じものだけ。

## 直してよいもの／だめなもの

- ✅ シナリオのダミーデータ（Aiko / 12k / 14日）、シーンの尺、壁紙、筐体の色
- ❌ **L1 / L2 / L3 のタグと色** — 権限表示はトーンや尺で消してよいものではない
- ❌ `nothing sent` / `1 message left this device` の2行 — 「勝手に動くが外には出ない」が
  このデモの主張そのもので、これを外すと別プロダクトの宣伝になる
- ❌ パネル寸法 560px と展開 100ms — SLO と実装に連動（`CLAUDE.md`）
- ❌ **画面内に L1〜L5 の層番号を出すこと** — 権限レベルの L1/L2/L3 と衝突する（上記）
- ❌ **物理ノッチ 185×32 と画面 1512×982** — 実測値と実寸。ここを動かすと筐体が別機種になる
- ❌ 蓋の状態クラスを `body.shut` に戻すこと — 要素側の `.shut` に自分がマッチして筐体が飛ぶ
- ❌ 「パーソナルAGI」を定義なしで単独使用すること（§6 禁じ手）

文言は `apps/desktop/src/strings.ts` に対応するものがある限りそちらを正とする。
`WHILE YOU SLEPT` / `READY FOR YOU` の2見出しだけは LP 向けの追加コピーで、
実装には存在しない。
