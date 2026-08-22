use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=tests");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(reference_path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={reference_path}");
        }
    }

    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_default();
    let dirty = git(&["status", "--porcelain"])
        .map(|status| (!status.is_empty()).to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MEMORY_BENCH_BUILD_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=MEMORY_BENCH_BUILD_GIT_DIRTY={dirty}");
}
