#!/usr/bin/env bash
set -euo pipefail

# Local onboarding QA only. Reset once before a manual test launch. An onboarding-triggered app
# restart bypasses this wrapper, so newly granted Screen Recording access can survive that restart.
readonly BUNDLE_ID="com.syogun.shogunai"

echo "[onboarding-qa] resetting Accessibility, Microphone, and Screen Recording for ${BUNDLE_ID}"
/usr/bin/tccutil reset Accessibility "${BUNDLE_ID}"
/usr/bin/tccutil reset Microphone "${BUNDLE_ID}"
/usr/bin/tccutil reset ScreenCapture "${BUNDLE_ID}"

export SHOGUN_FORCE_ONBOARDING=1
exec pnpm --dir apps/desktop tauri dev
