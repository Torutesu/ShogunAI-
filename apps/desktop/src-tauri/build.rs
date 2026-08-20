fn main() {
    // analytics.rs が option_env!("SHOGUN_POSTHOG_KEY") でビルド時埋め込みを行うため、
    // env の変更をインクリメンタルビルドでも確実に反映させる（stale キー混入の防止）。
    println!("cargo:rerun-if-env-changed=SHOGUN_POSTHOG_KEY");

    // whisper-rs builds Objective-C Metal sources that use `@available`. Clang supplies the
    // runtime check in compiler-rt, but Rust does not add that archive during the final link.
    // Ask the active Xcode toolchain for its resource directory so Xcode upgrades cannot leave a
    // stale, hard-coded Clang version here.
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("xcrun")
            .args(["clang", "-print-resource-dir"])
            .output()
        {
            if output.status.success() {
                if let Ok(resource_dir) = String::from_utf8(output.stdout) {
                    let darwin_runtime_dir = resource_dir.trim().to_owned() + "/lib/darwin";
                    println!("cargo:rustc-link-search=native={darwin_runtime_dir}");
                    println!("cargo:rustc-link-lib=static=clang_rt.osx");
                }
            }
        }
    }

    tauri_build::build();
}
