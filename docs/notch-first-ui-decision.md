# 判断記録 — ノッチファーストUI(ウィンドウUIの廃止)

- 決定日: 2026-08-07(オーナー判断)
- 状態: 採用。ワイヤーフレーム正本に反映済み(`docs/wireframes/shogun-notch-full.html`)
- 影響範囲: UIサーフェスのみ。**機能要件は削らない**(各機能の露出先がノッチパネル内に変わるだけ)

## 決定

**ノッチ以外の画面UI(別ウィンドウ)は作らない。** 設定・ブリーフ・メモリ・ヘルス・トレーサビリティ・承認はすべて**ノッチパネル内のステート**として提供する。パネル寸法は `docs/wireframe-spec.md` §0 の既存値に従う(展開300、設定・ビュー時は H_SETTINGS=460)。

## 何が置き換わるか

| 旧サーフェス(ウィンドウ) | 新しい置き場所(パネル内ステート) |
|---|---|
| Full UI — Today(`shogun-fullui*.html`) | `today` ビュー(ブリーフ+スケジュール要約+suggested actions) |
| Full UI — Memory | `memory` ビュー(検索+フィルタ+confidence付きリスト+merge review) |
| Full UI — Context Health / Traceability | `status` ビュー(coverage / freshness / egress / SLOの要約)+ Settings「Privacy & Data」内のegress台帳 |
| Full UI — Activity / Sources | Settings「Connections」(接続と鮮度)/ 承認は「Approvals」タブ。実行履歴の全量表示はv1ではパネル内スクロールで提供(要検討: 下記) |
| Settings ウィンドウ(`shogun-settings.html` 全10タブ) | Settings 5タブに圧縮: General(外観+ショートカット+Nightly)/ Privacy & Data(権限+egress+danger)/ Connections / Model & Key / Approvals |
| Onboarding / Plans ウィンドウ | **維持**(初回セットアップと課金はパネル外でよい。常用UIではないため本決定の対象外) |

- **会議ノートUI**: デザイナーが着手済みのため本決定の対象外。機能(検知+オンデバイスASR+Recap、§6.16)はそのまま維持
- 旧ワイヤー(`shogun-fullui.html` / `shogun-fullui-standard.html` / `shogun-settings.html`)は参照用として残すが、**v1サーフェスとしては廃止**

## 理由

- プロダクトの一言定義は「ノッチから仕事が終わる」。別ウィンドウのFull UIは記録ツール的な重心をつくり、定義と競合する
- 設定・状態確認のためにウィンドウを増やすと、常駐オーバーレイとしての性格(押してから1秒以内に価値)が薄まる
- 不変条件6(人間UIとAI APIの対称性)には影響しない — 同じ機能をMCP/CLIからも呼べる形は維持

## 実装への影響(フロントは後で差し替え予定)

- `apps/desktop` の `fullui.html` 系ウィンドウはv1から除外予定。設定はNotchパネルのステートとして実装(webview1枚のまま)
- プラン判定・L1/L2/L3・トレーサビリティ記録などRustコア側の責務は**変更なし**(不変条件1)
- SLOは従来どおり: パネル展開100ms・アクション提示150ms。設定ステートへの遷移もパネル内トランジション(260ms以内)で収める

## 未解決(次の判断が必要)

1. **実行履歴(Activity)の全量表示**: 620×460パネルにテーブルは窮屈。v1は直近N件+スクロールで足りるか、`shogun` CLI / Memory APIに全量を寄せるか
2. ノッチ非搭載Mac・外部ディスプレイの擬似ノッチパネルでも同ステート構成をそのまま使う(想定どおりだが実測はPhase 0の枠組みで)
3. Figmaファイル側: 旧Full UI / Settingsページ(03〜05)は「Deprecated」ラベルを付けて残すか削除するか(デザイナー判断)
