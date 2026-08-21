# 実ロゴの出典

Dock のアイコンは **23個が本物**、残りが描画版。本物は2系統から取っている。
いずれも各社の商標であり、製品デモ内で当該アプリを指す目的でのみ使用している。

## 1. App Store の公開アートワーク（20個・PNG）

`https://itunes.apple.com/search`（APIキー不要）から `artworkUrl512` を取得している。
取得は **`logos/fetch_appstore.py` が正本**で、下表はその `SPEC` の写し。検索の先頭ヒットではなく
**`trackName` と `sellerName` の完全一致**で固定しているので、再実行すると同じ絵が返るか、
さもなくば落ちる — 別の publisher の似たアイコンに黙って差し替わることがない。

- `logos/appstore/app_<key>.png` … 512×512 の原本
- `logos/appstore/128/<key>.png` … 実際に埋め込む 128px 版

128px 版は **macOS の squircle（superellipse n=5）でマスク**している。角丸矩形ではない。
原本が既にアルファで自分の輪郭を持っている場合（LINE / Slack）はマスクせずアルファの bbox で
クロップする — 二重に角を丸めないため。

| key | entity | trackName | sellerName |
|---|---|---|---|
| safari / messages / mail / maps / photos | software | Safari / Messages / Mail / Apple Maps / Photos | Apple Inc. |
| facetime / calendar / contacts / reminders / notes | software | FaceTime / Calendar / Contacts / Reminders / Notes | Apple Inc. |
| music | software | Apple Music | Apple Inc. |
| freeform | software | Freeform | Apple Inc. |
| keynote / numbers / pages | macSoftware | Keynote: Design Presentations 他 | Apple Inc. |
| slack | macSoftware | Slack for Desktop | SLACK TECHNOLOGIES L.L.C. |
| line | macSoftware | LINE | LY Corporation |
| notion | software | Notion: Notes, Tasks, AI | Notion Labs, Incorporated |
| discord | software | Discord - Talk, Play, Hang Out | Discord Inc. |
| raycast | software | Raycast: AI, Notes and more | Raycast Technologies Inc |

Mac 版がストアにあるものは `macSoftware` を優先している。無いもの（Apple 純正の Safari /
Mail / Photos など、および Discord・Raycast）は iOS 版のアートワーク。Big Sur 以降で両者の
絵柄は統合されているので実物と一致するが、**Calendar だけは日付が焼き込み**（macOS は実日付を描く）。

## 2. Wikimedia Commons の自由ライセンス版（3個・SVG）

App Store に無いもの、あるいはベクタのほうが良いもの。

| ファイル | Commons のファイル名 |
|---|---|
| `chrome.svg` | Google Chrome icon (February 2022).svg |
| `gemini.svg` | Google Gemini icon 2025.svg |
| `figma.svg` | Figma-logo.svg |

## まだ描画版のもの

**Finder / Launchpad / App Store / System Settings / ゴミ箱** — macOS にバンドルされていて
どのストアにも無い。**Arc / Warp / Terminal** と Dock 内の未同定の数枚も同様。
形は寄せてあるが本物ではない。

全部を本物にしたい場合は README の「Dock を実物のスクショに差し替える」を使う
（自分の Dock を撮った `dock.png` を置けば Dock ごと実物に差し替わる）。
