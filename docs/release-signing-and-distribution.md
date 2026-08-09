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

Gatekeeper の要件（macOS 14+）: Web からダウンロードされたアプリは **Developer ID 署名 + notarization（公証）+ ticket staple** の3点が揃っていないと「壊れているため開けません」ダイアログになる。ad-hoc 署名やただの codesign では配布不可。この3点はすべて Tauri v2 バンドラが環境変数から自動実行する。

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
   - `SHOGUN-macOS-arm64-0.1.0.dmg`（バージョン付き）
   - `SHOGUN-macOS-arm64.dmg`（LP 用固定名）

**なぜ Tauri の DMG 生成を使わないか**: Tauri の `dmg` ターゲットは create-dmg 由来の `bundle_dmg.sh` を呼び、AppleScript で Finder にウィンドウ配置をさせる。ヘッドレスのランナーではここが無限にブロックしうる（本ワークフローの初回実行が実際に1時間以上停止した）。`hdiutil` なら window server を介さず同じ成果物が作れるため、DMG 生成は自前ステップにしている。あわせて **`.app` と DMG の両方**を公証・staple している（DMG だけを staple すると、アプリを Applications にドラッグした後の初回起動でオンライン確認が必要になる）。

最後に人間が Release ページで **Publish release** を押した時点で公開される（LP のリンクが新版を指すのはこの瞬間）。

タグを打たずに試したいときは Actions → release → **Run workflow**（DMG はワークフローの Artifacts に上がるだけで Release は作られない）。

## 3. LP からの配布

公開後、以下の URL が常に最新版を指す（リダイレクト）:

```
https://github.com/torutesu/ShogunAI-/releases/latest/download/SHOGUN-macOS-arm64.dmg
```

LP のダウンロードボタンはこの固定 URL に張るだけでよい。将来 CDN（Cloudflare R2 等）に載せ替える場合も、この URL から DMG を取得して置くだけで署名・公証はそのまま有効（署名はファイルに内包されており、配布経路に依存しない）。

ユーザー側の体験: DMG を開く → アプリを Applications へドラッグ → 初回起動時に「"SHOGUN Spike" は Apple により悪質なソフトウェアが含まれていないか確認されました」の確認ダイアログ（公証済みの正常フロー）→ 起動。

## 4. 手元での最終確認（任意）

配布前に自分の Mac で:

```bash
spctl --assess --type open --context context:primary-signature -v SHOGUN-macOS-arm64.dmg
# → accepted / source=Notarized Developer ID なら合格
xcrun stapler validate SHOGUN-macOS-arm64.dmg   # → The validate action worked!
```

ブラウザで実際にダウンロードして開くのが最終テスト（quarantine 属性が付いた状態での Gatekeeper 挙動を踏む）。

---

## 5. 既知の注意点・今後

- **製品名/識別子**: 現状は Phase 0 スパイクのため `SHOGUN Spike` / `dev.shogun.spike`。一般公開前に `tauri.conf.json` の `productName` / `identifier` を製品名へ変更する（`entitlements.plist` の keychain-access-groups も同時に変更。識別子を変えると Keychain・TCC 許可（Accessibility / Screen Recording / Mic）がリセットされるため、**トライアル開始前に一度だけ**行うこと）
- **TCC 許可の持続**: 安定した Developer ID 署名になることで、これまで dev ビルドで問題だった「リビルドごとの許可やり直し」は配布ビルドでは起きない
- **自動更新（Tauri updater）**: 未設定。導入時は updater 署名鍵ペアの生成・`createUpdaterArtifacts` 有効化・update manifest 配信先が必要（別トラック。Gatekeeper 署名とは独立）
- **Intel Mac**: 対応環境は Apple Silicon のみ（CLAUDE.md）。x86_64 ビルドは作らない
- **証明書の期限**: Developer ID Application 証明書は5年有効。失効しても**公証済みの既配布物は動き続ける**（新規ビルドに新証明書が必要になるだけ）
