# リリース署名・配布ランブック（Developer ID / App Store 不使用）

CLAUDE.md「課金/配布」どおり、SHOGUN は **Developer ID 署名 + notarization** で LP から直接配布する。App Store には載せないため、Apple のマージン（15〜30%）は一切かからない。かかる費用は Apple Developer Program の年会費のみ。個人（Individual）アカウントで問題なく完結する。

ビルド・署名・公証は `.github/workflows/release.yml`（macOS arm64 ランナー）が全自動で行う。ローカル Mac での作業は**証明書と公証クレデンシャルの発行（初回のみ）**だけ。

---

## 全体像

```
[初回のみ] Developer ID 証明書 + 公証クレデンシャル発行 → GitHub Secrets に登録
[毎リリース] git tag v0.1.0 → push → CI が署名済み・公証済み DMG を Draft Release に添付
            → 人間が Release を publish → LP の固定リンクが新版を指す
```

Gatekeeper の要件（macOS 14+）: Web からダウンロードされたアプリは **Developer ID 署名 + notarization（公証）+ ticket staple** の3点が揃っていないと「壊れているため開けません」ダイアログになる。ad-hoc 署名やただの codesign では配布不可。署名は Tauri v2 バンドラが環境変数から行い、公証と staple はワークフローの明示ステップが行う（理由は §2 末尾）。

---

## 1. 初回セットアップ（ローカル Mac、約30分）

### 1-a. Developer ID Application 証明書を作る

1. Mac の **キーチェーンアクセス** → メニュー「証明書アシスタント」→「認証局に証明書を要求」
   - メール: Apple ID のメール / 通称: 自分の名前 / 「ディスクに保存」を選択 → `CertificateSigningRequest.certSigningRequest` を保存
2. [developer.apple.com/account/resources/certificates](https://developer.apple.com/account/resources/certificates/list) → 「+」→ **Developer ID Application** を選択（"Apple Development" や "Apple Distribution" ではない。Developer ID G2 Sub-CA が既定）→ 上の CSR をアップロード → `.cer` をダウンロード
3. `.cer` をダブルクリックしてキーチェーン（ログイン）に取り込む
4. キーチェーンアクセスで該当証明書（秘密鍵ごと）を右クリック → 書き出す → **.p12** 形式、パスワードを設定して保存

### 1-b. 署名アイデンティティ名を控える

```bash
security find-identity -v -p codesigning
# → "Developer ID Application: Your Name (XXXXXXXXXX)" の行の引用符内全体をコピー
# Developer ID 証明書では括弧内の10桁 = Team ID
```

注意: `Apple Development: ...` の行を使わないこと（開発用。これで署名した配布物は Gatekeeper に弾かれる）。また **Apple Development 証明書の括弧内は Team ID ではない**（証明書固有 ID）。Team ID は [Membership details](https://developer.apple.com/account#MembershipDetailsCard) の Team ID、または証明書詳細の「組織単位(OU)」で確認する。

### 1-c. 公証（notarization)クレデンシャル

どちらか一方でよい。**個人アカウントなら App-specific password が最短**。

| 方式 | 手順 |
|---|---|
| **Apple ID + App-specific password**（推奨・最短） | [account.apple.com](https://account.apple.com) → サインインとセキュリティ → アプリ用パスワード → 生成（`xxxx-xxxx-xxxx-xxxx` 形式） |
| App Store Connect API キー | [App Store Connect → Users and Access → Integrations](https://appstoreconnect.apple.com/access/integrations/api) → キー生成（ロール: Developer 以上）→ `.p8` ダウンロード（一度きり）+ Key ID + Issuer ID を控える |

### 1-d. GitHub Secrets に登録

リポジトリ → Settings → Secrets and variables → Actions:

| Secret | 値 |
|---|---|
| `APPLE_CERTIFICATE` | `base64 -i cert.p12 \| pbcopy` の出力（.p12 の base64） |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 書き出し時に付けたパスワード |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (XXXXXXXXXX)` |
| `APPLE_TEAM_ID` | 10桁 Team ID |
| `APPLE_ID` | Apple ID メールアドレス（App-specific password 方式のみ） |
| `APPLE_PASSWORD` | アプリ用パスワード（同上） |
| `APPLE_API_ISSUER` / `APPLE_API_KEY` / `APPLE_API_KEY_CONTENT` | API キー方式のみ: Issuer ID / Key ID / `base64 -i AuthKey_XXX.p8` |

登録後、ローカルの .p12・.p8・CSR は安全な場所（1Password 等）に退避しディスクから削除。**リポジトリには絶対にコミットしない**（不変条件7と同じ扱い）。

---

## 2. リリース手順（毎回）

```bash
git tag v0.1.0
git push origin v0.1.0
```

→ `release` ワークフローが走り、以下を自動実行:

1. ジョブ専用キーチェーンに証明書を取り込み、署名アイデンティティが解決することを先に検証
2. arm64 でフルビルド（`tauri build`、バージョンはタグから注入）＋ Developer ID 署名（hardened runtime + `entitlements.plist`）
3. `.app` を notarytool へ提出（`--wait --timeout 30m`）→ staple
4. staple 済み `.app` から `hdiutil` で DMG を作成 → DMG も署名・公証・staple
5. `codesign --verify` / `spctl --assess` / `stapler validate` を `.app` と DMG の両方で検証（ここが赤ければ配布物は壊れている）
6. **Draft** の GitHub Release に DMG を2つ添付:
   - `ShogunAI-macOS-arm64-0.1.0.dmg`（バージョン付き）
   - `ShogunAI-macOS-arm64.dmg`（LP 用固定名）

**なぜ公証と DMG 生成を Tauri に任せないか**: Tauri のバンドラは `notarytool ... --wait` をタイムアウトなしで呼ぶ。本ワークフローの初回実行は `Notarizing .../SHOGUN Spike.app` を出力したまま **56分間無反応**となり、キャンセル時の後片付けで孤児プロセス `notarytool` が回収された（ビルドと署名自体は6分で正常完了していた）。そのため公証は自前ステップで `--wait --timeout 30m` を付けて実行し、失敗時は Apple 側のログを出力する。公証を自前で回すなら DMG も `hdiutil` で作るほうが扱いやすく（バンドラの create-dmg / AppleScript 経路を通らない）、**`.app` と DMG の両方**を署名・公証・staple できる。DMG だけを staple すると、アプリを Applications にドラッグした後の初回起動でオンライン確認が必要になる。

**実測（2026-08-09 初回グリーン、キャッシュ無し）**: 全体11分（ビルド＋署名9分 / 公証2回で54秒 / 検証2秒）。

最後に人間が Release ページで **Publish release** を押した時点で公開される（LP のリンクが新版を指すのはこの瞬間）。

タグを打たずに試したいときは Actions → release → **Run workflow**（DMG はワークフローの Artifacts に上がるだけで Release は作られない）。

## 3. LP からの配布

公開後、以下の URL が常に最新版を指す（リダイレクト）:

```
https://github.com/torutesu/ShogunAI-/releases/latest/download/ShogunAI-macOS-arm64.dmg
```

LP のダウンロードボタンはこの固定 URL に張るだけでよい。将来 CDN（Cloudflare R2 等）に載せ替える場合も、この URL から DMG を取得して置くだけで署名・公証はそのまま有効（署名はファイルに内包されており、配布経路に依存しない）。

ユーザー側の体験: DMG を開く → アプリを Applications へドラッグ → 初回起動時に「"ShogunAI" は Apple により悪質なソフトウェアが含まれていないか確認されました」の確認ダイアログ（公証済みの正常フロー）→ 起動。

## 4. 手元での最終確認（任意）

配布前に自分の Mac で:

```bash
spctl --assess --type open --context context:primary-signature -v ShogunAI-macOS-arm64.dmg
# → accepted / source=Notarized Developer ID なら合格
xcrun stapler validate ShogunAI-macOS-arm64.dmg   # → The validate action worked!
```

ブラウザで実際にダウンロードして開くのが最終テスト（quarantine 属性が付いた状態での Gatekeeper 挙動を踏む）。

---

## 5. 既知の注意点・今後

- **製品名/識別子（2026-08-09 変更済み）**: `ShogunAI` / `com.syogun.shogunai`（旧: `SHOGUN Spike` / `dev.shogun.spike`）。識別子は所有ドメイン syogun.com の逆DNS。**これ以上変更しないこと** — 変えると Keychain・TCC 許可（Accessibility / Screen Recording / Mic）・アプリデータ（`~/Library/Application Support/<identifier>/` の memory.db と onboarding.json）が全部リセットされる。識別子は `tauri.conf.json` / `entitlements.plist` の keychain-access-groups / `crates/shogun-mcp/src/plan_source.rs` の `DESKTOP_IDENTIFIER` / `apps/desktop/src-tauri/src/meeting.rs` の `SHOGUN_BUNDLE_IDS` が lockstep。開発機に旧 `dev.shogun.spike` のデータが残っている場合、そのまま引き継がれないので必要なら手で移す
- **バイナリ名**: `.app` 内の実行ファイルは Cargo のパッケージ名 `shogun-desktop-spike` のまま（アクティビティモニタにこの名前で出る）。改名するには Cargo.toml のパッケージ名と `ci.yml` / `phase0-smoke.yml` / `scripts/codesign-desktop-dev.sh` の参照を揃える必要があり、製品名変更とは独立した作業
- **TCC 許可の持続**: 安定した Developer ID 署名になることで、これまで dev ビルドで問題だった「リビルドごとの許可やり直し」は配布ビルドでは起きない
- **自動更新（Tauri updater）**: 未設定。導入時は updater 署名鍵ペアの生成・`createUpdaterArtifacts` 有効化・update manifest 配信先が必要（別トラック。Gatekeeper 署名とは独立）
- **Intel Mac**: 対応環境は Apple Silicon のみ（CLAUDE.md）。x86_64 ビルドは作らない
- **証明書の期限**: Developer ID Application 証明書は5年有効。失効しても**公証済みの既配布物は動き続ける**（新規ビルドに新証明書が必要になるだけ）
