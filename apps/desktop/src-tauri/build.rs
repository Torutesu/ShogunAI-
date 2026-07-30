fn main() {
    // analytics.rs が option_env!("SHOGUN_POSTHOG_KEY") でビルド時埋め込みを行うため、
    // env の変更をインクリメンタルビルドでも確実に反映させる（stale キー混入の防止）。
    println!("cargo:rerun-if-env-changed=SHOGUN_POSTHOG_KEY");
    tauri_build::build();
}
