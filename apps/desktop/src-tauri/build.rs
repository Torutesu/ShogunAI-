fn main() {
    // `whisper-rs` builds Objective-C Metal sources that use `@available`.
    // Clang implements that runtime check in its compiler-rt archive, but Rust
    // does not add the archive when it performs the final executable link.
    // Resolve the active Xcode resource directory instead of hard-coding a
    // Clang version so fresh macOS builds keep working after Xcode upgrades.
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
