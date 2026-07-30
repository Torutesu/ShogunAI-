// visspike — FR-VIS verification-gate spike (docs/vis-capture-spike-runbook.md)
//
// Measures, on a real Mac, the three Go/No-Go questions for event-driven visual
// capture (requirements-v1.0 §6.21):
//   (1) CPU cost of keyframe capture + on-device OCR during normal work
//   (2) how much text OCR recovers that AX capture misses (precision proxy)
//   (3) storage growth rate at the intended downscale/compression settings
//
// Invariant alignment even in the spike: event-driven keyframes only (no video,
// no continuous stream), password-manager exclusion, nothing leaves the device.
// Frames are written unencrypted to a local out dir for inspection — delete the
// out dir after the run (runbook step).

import AppKit
import ApplicationServices
import ScreenCaptureKit
import Vision

// MARK: - config

let pollIntervalMs: UInt64 = 500        // focus/content poll cadence
let minCaptureGapS: TimeInterval = 2.0  // per-window content-change re-capture floor
let maxFrameWidth = 1600                // downscale target (FR-VIS-03)
let jpegQuality: CGFloat = 0.5
let dhashSkipThreshold = 6              // hamming distance ≤ this ⇒ duplicate, drop
let axWalkBudgetMs = 250.0              // mirror axcache.rs timebox
let axMaxElements = 300
let axMaxDepth = 8

// FR-VIS-04 / FR-CAP-05: exclusion list (password managers + SecurityAgent).
let excludedBundleIds: Set<String> = [
    "com.1password.1password", "com.agilebits.onepassword7",
    "com.bitwarden.desktop", "org.keepassxc.keepassxc",
    "com.dashlane.Dashlane", "com.sinopoli.enpass", "in.sinew.Enpass-Desktop",
    "com.apple.keychainaccess", "com.apple.SecurityAgent",
]

let axTextRoles: Set<String> = [
    "AXStaticText", "AXTextArea", "AXTextField", "AXHeading", "AXLink", "AXCell",
]

// MARK: - stop flag (SIGINT-safe)

var gStop: sig_atomic_t = 0
signal(SIGINT) { _ in gStop = 1 }

// MARK: - metrics

struct FrameRecord: Codable {
    let ts: Double
    let app: String
    let trigger: String        // "focus" | "content"
    let captureMs: Double
    let ocrMs: Double
    let bytes: Int
    let ocrChars: Int
    let axChars: Int
    let ocrOnlyTokens: Int
    let axTokens: Int
    let ocrTokens: Int
}

final class Metrics {
    var frames: [FrameRecord] = []
    var dedupDropped = 0
    var captureErrors = 0
    var startWall = Date()
    var cpuSamples: [(wall: Double, cpu: Double)] = []  // cumulative process CPU seconds

    func processCpuSeconds() -> Double {
        var ru = rusage()
        getrusage(RUSAGE_SELF, &ru)
        let u = Double(ru.ru_utime.tv_sec) + Double(ru.ru_utime.tv_usec) / 1e6
        let s = Double(ru.ru_stime.tv_sec) + Double(ru.ru_stime.tv_usec) / 1e6
        return u + s
    }

    func sampleCpu() {
        cpuSamples.append((Date().timeIntervalSince1970, processCpuSeconds()))
    }

    // Per-minute CPU% series from cumulative samples.
    func perMinuteCpuPercent() -> [Double] {
        guard cpuSamples.count > 2 else { return [] }
        var out: [Double] = []
        var windowStart = cpuSamples[0]
        for s in cpuSamples.dropFirst() {
            if s.wall - windowStart.wall >= 60 {
                let pct = (s.cpu - windowStart.cpu) / (s.wall - windowStart.wall) * 100
                out.append(pct)
                windowStart = s
            }
        }
        return out
    }
}

func percentile(_ sorted: [Double], _ p: Double) -> Double {
    guard !sorted.isEmpty else { return 0 }
    let idx = min(sorted.count - 1, Int(ceil(p / 100 * Double(sorted.count))) - 1)
    return sorted[max(0, idx)]
}

// MARK: - AX text walk (bounded, mirrors axcache.rs)

func axAttr(_ el: AXUIElement, _ name: String) -> CFTypeRef? {
    var v: CFTypeRef?
    let err = AXUIElementCopyAttributeValue(el, name as CFString, &v)
    return err == .success ? v : nil
}

func axString(_ el: AXUIElement, _ name: String) -> String? {
    axAttr(el, name) as? String
}

func axFocusedWindow(pid: pid_t) -> (el: AXUIElement, title: String)? {
    let app = AXUIElementCreateApplication(pid)
    guard let win = axAttr(app, kAXFocusedWindowAttribute as String) else { return nil }
    let winEl = win as! AXUIElement
    let title = axString(winEl, kAXTitleAttribute as String) ?? ""
    return (winEl, title)
}

func axVisibleText(root: AXUIElement) -> String {
    var pieces: [String] = []
    var queue: [(AXUIElement, Int)] = [(root, 0)]
    var visited = 0
    let deadline = Date().addingTimeInterval(axWalkBudgetMs / 1000.0)
    while !queue.isEmpty, visited < axMaxElements, Date() < deadline {
        let (el, depth) = queue.removeFirst()
        visited += 1
        let role = axString(el, kAXRoleAttribute as String) ?? ""
        if role == "AXSecureTextField" { continue }  // never read, never descend
        if axTextRoles.contains(role) {
            if let v = axString(el, kAXValueAttribute as String), !v.isEmpty {
                pieces.append(v)
            } else if let t = axString(el, kAXTitleAttribute as String), !t.isEmpty {
                pieces.append(t)
            }
        }
        if depth < axMaxDepth,
           let children = axAttr(el, kAXChildrenAttribute as String) as? [AXUIElement] {
            for c in children.prefix(40) { queue.append((c, depth + 1)) }
        }
    }
    return pieces.joined(separator: "\n")
}

// MARK: - tokenization (coverage proxy)

func tokens(_ s: String) -> Set<String> {
    let lowered = s.lowercased()
    var toks: Set<String> = []
    var current = ""
    for ch in lowered {
        if ch.isLetter || ch.isNumber {
            current.append(ch)
        } else {
            if current.count >= 2 { toks.insert(current) }
            current = ""
        }
    }
    if current.count >= 2 { toks.insert(current) }
    return toks
}

// MARK: - dHash (9x8 difference hash)

func dhash(_ image: CGImage) -> UInt64 {
    let w = 9, h = 8
    var pixels = [UInt8](repeating: 0, count: w * h)
    guard let ctx = CGContext(
        data: &pixels, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w,
        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue
    ) else { return 0 }
    ctx.interpolationQuality = .low
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
    var hash: UInt64 = 0
    for row in 0..<h {
        for col in 0..<(w - 1) {
            hash <<= 1
            if pixels[row * w + col] > pixels[row * w + col + 1] { hash |= 1 }
        }
    }
    return hash
}

func hamming(_ a: UInt64, _ b: UInt64) -> Int { (a ^ b).nonzeroBitCount }

// MARK: - capture + OCR

func captureWindow(pid: pid_t, title: String) async throws -> CGImage? {
    let content = try await SCShareableContent.excludingDesktopWindows(
        false, onScreenWindowsOnly: true)
    let candidates = content.windows.filter {
        $0.owningApplication?.processID == pid && $0.isOnScreen && $0.windowLayer == 0
    }
    let win = candidates.first { ($0.title ?? "") == title }
        ?? candidates.max { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }
    guard let win else { return nil }
    let filter = SCContentFilter(desktopIndependentWindow: win)
    let cfg = SCStreamConfiguration()
    let scale = min(1.0, CGFloat(maxFrameWidth) / max(1, win.frame.width))
    cfg.width = Int(win.frame.width * scale)
    cfg.height = Int(win.frame.height * scale)
    cfg.showsCursor = false
    return try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: cfg)
}

func ocr(_ image: CGImage) throws -> String {
    let req = VNRecognizeTextRequest()
    req.recognitionLevel = .accurate
    req.recognitionLanguages = ["en-US", "ja-JP"]
    req.usesLanguageCorrection = false
    try VNImageRequestHandler(cgImage: image, options: [:]).perform([req])
    return (req.results ?? [])
        .compactMap { $0.topCandidates(1).first?.string }
        .joined(separator: "\n")
}

func writeJpeg(_ image: CGImage, to url: URL) -> Int {
    let rep = NSBitmapImageRep(cgImage: image)
    guard let data = rep.representation(
        using: .jpeg, properties: [.compressionFactor: jpegQuality]) else { return 0 }
    try? data.write(to: url)
    return data.count
}

// MARK: - summary

func writeSummary(_ m: Metrics, outDir: URL) {
    let wall = Date().timeIntervalSince(m.startWall)
    let cpu = m.processCpuSeconds()
    let captureMs = m.frames.map(\.captureMs).sorted()
    let ocrMs = m.frames.map(\.ocrMs).sorted()
    let totalMs = zip(m.frames.map(\.captureMs), m.frames.map(\.ocrMs)).map(+).sorted()
    let bytes = m.frames.map(\.bytes).reduce(0, +)
    let perMin = m.perMinuteCpuPercent().sorted()

    // Per-app aggregation: is OCR finding text AX cannot see?
    var byApp: [String: (frames: Int, ocrOnly: Int, union: Int, axChars: Int, ocrChars: Int)] = [:]
    for f in m.frames {
        var e = byApp[f.app] ?? (0, 0, 0, 0, 0)
        e.frames += 1
        e.ocrOnly += f.ocrOnlyTokens
        e.union += max(1, f.axTokens + f.ocrOnlyTokens)
        e.axChars += f.axChars
        e.ocrChars += f.ocrChars
        byApp[f.app] = e
    }

    var md = "# visspike summary\n\n"
    md += "| metric | value | gate |\n|---|---|---|\n"
    md += String(format: "| wall clock | %.1f min | — |\n", wall / 60)
    md += String(format: "| process CPU (avg) | %.2f%% | ≤ 5%% |\n", cpu / wall * 100)
    md += String(format: "| per-minute CPU p95 | %.2f%% | ≤ 10%% |\n", percentile(perMin, 95))
    md += "| frames kept | \(m.frames.count) | — |\n"
    md += "| frames deduped (dHash) | \(m.dedupDropped) | — |\n"
    md += "| capture errors | \(m.captureErrors) | — |\n"
    md += String(format: "| capture+OCR p95 | %.0f ms | ≤ 1500 ms |\n", percentile(totalMs, 95))
    md += String(format: "| capture p95 / OCR p95 | %.0f / %.0f ms | — |\n",
                 percentile(captureMs, 95), percentile(ocrMs, 95))
    md += String(format: "| storage | %.1f MB | — |\n", Double(bytes) / 1e6)
    md += String(format: "| storage projection | %.0f MB/day (8h active) | ≤ 200 MB/day |\n",
                 Double(bytes) / 1e6 / max(wall / 3600, 0.01) * 8)
    md += "\n## per app (OCR-only token ratio = text AX missed)\n\n"
    md += "| app | frames | OCR-only ratio | AX chars | OCR chars |\n|---|---|---|---|---|\n"
    for (app, e) in byApp.sorted(by: { $0.value.frames > $1.value.frames }) {
        let ratio = Double(e.ocrOnly) / Double(max(1, e.union)) * 100
        md += String(format: "| %@ | %d | %.0f%% | %d | %d |\n",
                     app, e.frames, ratio, e.axChars, e.ocrChars)
    }
    try? md.write(to: outDir.appendingPathComponent("summary.md"),
                  atomically: true, encoding: .utf8)
    if let json = try? JSONEncoder().encode(m.frames) {
        try? json.write(to: outDir.appendingPathComponent("frames.json"))
    }
    print("\n" + md)
    print("out dir: \(outDir.path)")
}

// MARK: - main loop

guard AXIsProcessTrusted() else {
    print("visspike: Accessibility permission missing for this terminal.")
    print("System Settings → Privacy & Security → Accessibility → add your terminal, then rerun.")
    exit(1)
}
// Screen Recording permission is prompted on first SCShareableContent call.

var durationMin: Double? = nil
if let i = CommandLine.arguments.firstIndex(of: "--duration"),
   i + 1 < CommandLine.arguments.count {
    durationMin = Double(CommandLine.arguments[i + 1])
}

let outDir = URL(fileURLWithPath: "vis-spike-out")
    .appendingPathComponent(ISO8601DateFormatter().string(from: Date()))
let framesDir = outDir.appendingPathComponent("frames")
try FileManager.default.createDirectory(at: framesDir, withIntermediateDirectories: true)

let metrics = Metrics()
var lastKey = ""
var lastAxHash = 0
var lastCaptureAt: [String: Date] = [:]
var lastHash: [String: UInt64] = [:]
var frameSeq = 0
var lastSummaryAt = Date()

print("visspike: running. Ctrl-C to stop and write summary. out: \(outDir.path)")

let task = Task {
    while gStop == 0 {
        if let d = durationMin, Date().timeIntervalSince(metrics.startWall) > d * 60 { break }
        try? await Task.sleep(nanoseconds: pollIntervalMs * 1_000_000)
        metrics.sampleCpu()
        if Date().timeIntervalSince(lastSummaryAt) > 300 {  // periodic flush
            writeSummary(metrics, outDir: outDir)
            lastSummaryAt = Date()
        }

        guard let app = NSWorkspace.shared.frontmostApplication,
              let bundleId = app.bundleIdentifier,
              !excludedBundleIds.contains(bundleId),
              bundleId != Bundle.main.bundleIdentifier
        else { continue }
        let pid = app.processIdentifier
        guard let (winEl, title) = axFocusedWindow(pid: pid) else { continue }
        // Private-browsing heuristic (mirror pipeline.rs): skip if title says so.
        let lowT = title.lowercased()
        if lowT.contains("private browsing") || lowT.contains("incognito")
            || lowT.contains("inprivate") { continue }

        let axText = axVisibleText(root: winEl)
        let key = "\(bundleId)|\(title)"
        let axHash = axText.hashValue

        var trigger: String? = nil
        if key != lastKey {
            trigger = "focus"
        } else if axHash != lastAxHash,
                  Date().timeIntervalSince(lastCaptureAt[key] ?? .distantPast) >= minCaptureGapS {
            trigger = "content"
        }
        lastKey = key
        lastAxHash = axHash
        guard let trigger else { continue }

        let t0 = Date()
        let image: CGImage?
        do { image = try await captureWindow(pid: pid, title: title) }
        catch { metrics.captureErrors += 1; continue }
        guard let image else { metrics.captureErrors += 1; continue }
        let captureMs = Date().timeIntervalSince(t0) * 1000
        lastCaptureAt[key] = Date()

        let h = dhash(image)
        if let prev = lastHash[key], hamming(prev, h) <= dhashSkipThreshold {
            metrics.dedupDropped += 1
            continue
        }
        lastHash[key] = h

        let t1 = Date()
        let ocrText = (try? ocr(image)) ?? ""
        let ocrMs = Date().timeIntervalSince(t1) * 1000

        frameSeq += 1
        let bytes = writeJpeg(
            image, to: framesDir.appendingPathComponent(String(format: "%05d.jpg", frameSeq)))

        let axToks = tokens(axText)
        let ocrToks = tokens(ocrText)
        let ocrOnly = ocrToks.subtracting(axToks)
        metrics.frames.append(FrameRecord(
            ts: Date().timeIntervalSince1970, app: bundleId, trigger: trigger,
            captureMs: captureMs, ocrMs: ocrMs, bytes: bytes,
            ocrChars: ocrText.count, axChars: axText.count,
            ocrOnlyTokens: ocrOnly.count, axTokens: axToks.count, ocrTokens: ocrToks.count))
    }
}

// Keep the main runloop alive while the task runs (AppKit APIs want a runloop).
while gStop == 0, !task.isCancelled {
    RunLoop.main.run(until: Date().addingTimeInterval(0.25))
    if let d = durationMin, Date().timeIntervalSince(metrics.startWall) > d * 60 { break }
}
writeSummary(metrics, outDir: outDir)
