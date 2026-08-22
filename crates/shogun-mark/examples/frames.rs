//! Dump fold frames as base64 alpha, for eyeballing the rasteriser outside a Mac.
//! `cargo run -p shogun-mark --example frames -- 512 0.66015625 9`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let size: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(256);
    let fraction: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.66);
    let count: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(9);
    let placement = shogun_mark::Placement::new(fraction);

    println!("{size}");
    for i in 0..count {
        let ms = shogun_mark::DURATION_MS * i as f32 / (count - 1).max(1) as f32;
        let alpha = shogun_mark::unfold_alpha(ms, size, size, placement);
        println!("{ms:.0} {}", b64(&alpha));
    }
}

fn b64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(A[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
