# プロジェクトボード再レビュー・監査 サマリ（2026-07-30）

対象: https://github.com/users/Torutesu/projects/3 の In Review / Done 相当アイテム。
Projects v2 の Status / Priority フィールドは API 経由で読めないため、リポジトリ実データから再構成した:
**In Review ≈ オープン PR**（#92 / #90 / #79 / #78 / #74 / design-system スタック #14・#15・#17・#18）、
**Done ≈ クローズ済み Issue + マージ済み PR**（#47/#69, #61(#91), #56, #46(#76), #20(#71), #21(#66), #63系 #73/#75/#77/#83）、
**P0 ≈ 実装トリオ #80/#81/#82**（[P1] 明記の #84/#85/#70 より上位のスプリント本柱と判断）。

詳細レポート:
- Done 監査: `2026-07-30-audit-done-items.md`
- コード PR レビュー: `2026-07-30-review-open-code-prs.md`
- docs / design-system PR レビュー: `2026-07-30-review-docs-design-prs.md`

P0 仕様たたき（作成済み）:
- `docs/specs/issue-80-wave1-live-verification-spec.md`
- `docs/specs/issue-81-llm-mcp-wiring-spec.md`
- `docs/specs/issue-82-onboarding-connections-ui-spec.md`

---

## 最優先アクション（severity 順）

| # | 深刻度 | 内容 | 対象 |
|---|---|---|---|
| 1 | **High/セキュリティ** | waitlist: 既存メールで再 signup すると**他人の statusToken/statusUrl が返る**（ステータス閲覧・profile 改竄可能）。origin allowlist はフェイルオープン | Done #47/#69（website） |
| 2 | **High/プロセス** | **Issue #21（ピルドラッグ）は Done クローズ済みだが main に未統合**。PR #66 の base が `design-system/documentation-node` だった。main への移植が必要（Castle Position との位置優先規則の衝突解決込み） | Done #21/#66 |
| 3 | **High/ブロッカー** | PR #90（オンボーディング）は **main と merge base が無い別系統ブランチ**。このままマージすると会議ノート・音声スタック・analytics・CLAUDE.md の決定事項まで巻き戻る。中身は良いので**現 main 上に作り直して移植** | In Review #90 |
| 4 | **High/整合性** | PR #78（プライバシー）と マージ済み #91（PostHog）で**アナリティクスの既定が矛盾**（#91: 既定送信 / #78: 既定OFF表記）。#78 のゲートは dead_code で実配線されておらず「Off by default」コピーが虚偽になる。オーナー判断（opt-in か opt-out か）が必要 | In Review #78 |
| 5 | **High/docs** | PR #79（docs/mcp）は旧設計「Gmail 読み取り=公式MCP直結」を現行として記述し、**2026-07 の Gmail 全面 Composio 化決定と正面矛盾**。§5 のプライバシー説明はこのままだとユーザー向け虚偽になる。3開示同意・読み取り egress トレーサビリティ・draft-stop の追記必須 | In Review #79 |
| 6 | Med | PostHog キーが実行時 env のみ → 配布ビルドで計測サイレント無効。オプトアウト UI がオンボーディングにしか無い。トグルの日本語直書きは UI 英語規約違反 | Done #61/#91 |
| 7 | Med | design-system スタックは Free プラン矛盾こそブランチ上で解消済みだが、main の glass 刷新でトークン値が旧値のまま。**rebase-and-fold**（#14+#15+#17 を squash 1本 + #18 後続）を推奨。main の LP には `Free/$0` が4箇所残存 → マージ時に併せて修正 | In Review #14-#18 |

## In Review PR の判定一覧

| PR | 判定 | 一言 |
|---|---|---|
| #92 push-to-talk | **approve-with-nits** | 不変条件2（音声を RAM から出さない）をライン単位で確認、適合。#91 と analytics.rs が add/add 競合 → rebase 必須 |
| #74 Shougun.md | **approve-with-nits** | 指示文がシステム前文より前に注入される問題の修正・サイズ上限・atomic-save 対応・UI 文言の strings.ts 化 |
| #78 privacy/security | **request-changes** | 上記 #4。削除トランザクション・Keychain 運用自体は堅牢 |
| #90 onboarding | **request-changes** | 上記 #3。現 main 上での再構築が必須 |
| #79 docs/mcp | **request-changes** | 上記 #5 |
| #14/#17 tokens | **request-changes** / #15/#18 approve-with-nits | 上記 #7 |

**推奨マージ順: #92 → #74 → #78（方針決定後）→ #90（再構築後）**

## Done 監査の判定一覧

| 項目 | 判定 |
|---|---|
| #56 CI/CD 最適化 | fully done（apps/website の CI 不在のみ Med） |
| #46/#76 AX オンボーディング | fully done |
| #20/#71 Castle Position | fully done（SLO p50/p95 計測未添付は Low） |
| #47/#69 waitlist Supabase | done-with-gaps（上記 High #1） |
| #61/#91 PostHog | done-with-gaps（上記 Med #6） |
| #63 圧縮 #73/#75/#77/#83 | done-with-gaps（主張は全て実証済み。`SHOGUN_COMPRESSION=1` 既定 OFF の休眠状態、Issue open 継続は正しい） |
| #21/#66 ピルドラッグ | **not-actually-on-main**（上記 High #2） |

CLAUDE.md 絶対不変条件の明確な違反は **ゼロ**（マージ済みコード上）。

## 横断的な推奨

1. `git branch -r --no-merged origin/main` で「design 系ブランチに取り残された Done」の棚卸しを定期実施（#66 と同種の事故防止）
2. PR の base ブランチが main 以外のものはマージ前に必ず base を確認する運用（#66 / #90 の再発防止）
3. アナリティクスの opt-in / opt-out 方針をオーナーが1回決めて、#78・#91・オンボーディングの3箇所を同時に揃える
