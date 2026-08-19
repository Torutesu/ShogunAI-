#!/usr/bin/env bash
set -euo pipefail

# Local onboarding QA only. Reset once before a manual test launch. An onboarding-triggered app
# restart bypasses this wrapper, so newly granted Screen Recording access can survive that restart.
readonly BUNDLE_ID="com.syogun.shogunai"
readonly ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly DESKTOP_DIR="${ROOT_DIR}/apps/desktop"
readonly APP_PATH="${ROOT_DIR}/target/debug/bundle/macos/ShogunAI.app"
readonly APP_BINARY="${APP_PATH}/Contents/MacOS/shogun-desktop-spike"

# TCC identifies an app bundle, not the raw `cargo run` executable. Build the real debug `.app`
# first. Run Vite directly because QA source gates are executed separately and the local pnpm shim
# may be offline; disable Tauri's duplicate frontend build for this bundle pass.
echo "[onboarding-qa] building bundled debug app"
(
  cd "${DESKTOP_DIR}"
  node_modules/.bin/vite build
  node_modules/.bin/tauri build --debug --bundles app --config '{"build":{"beforeBuildCommand":""}}'
)

if [[ ! -x "${APP_BINARY}" ]]; then
  echo "[onboarding-qa] missing bundled executable: ${APP_BINARY}" >&2
  exit 1
fi

# Debug bundles are linker-signed with the executable name, which makes TCC ignore a reset for
# the product bundle id. Give the disposable QA bundle one coherent ad-hoc identity first.
/usr/bin/codesign --force --deep --sign - --identifier "${BUNDLE_ID}" "${APP_PATH}"

echo "[onboarding-qa] resetting Accessibility, Microphone, and Screen Recording for ${BUNDLE_ID}"
/usr/bin/tccutil reset Accessibility "${BUNDLE_ID}"
/usr/bin/tccutil reset Microphone "${BUNDLE_ID}"
/usr/bin/tccutil reset ScreenCapture "${BUNDLE_ID}"

# LaunchServices must start the bundle. Directly exec'ing Contents/MacOS makes TCC attribute the
# process to the shell instead of this app, leaving old grants visible after a successful reset.
exec /usr/bin/open -n -W \
  --env SHOGUN_FORCE_ONBOARDING=1 \
  "${APP_PATH}"
