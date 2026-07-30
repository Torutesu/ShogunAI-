// swift-tools-version: 5.9
// Vis-capture spike — measurement-only throwaway (FR-VIS verification gate).
// The product implementation stays Rust/objc2 per CLAUDE.md tech stack; this
// package exists only to answer the Go/No-Go questions on-device quickly.
// See docs/vis-capture-spike-runbook.md.
import PackageDescription

let package = Package(
    name: "visspike",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "visspike", path: "Sources/visspike")
    ]
)
