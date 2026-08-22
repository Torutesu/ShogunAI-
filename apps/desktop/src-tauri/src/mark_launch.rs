//! The Dock and menu-bar icons folding themselves together, once, as the app comes up.
//!
//! The app's windows fold the mark in CSS. These two do not: AppKit draws them from images, so
//! the same arrival has to be handed over frame by frame. `shogun-mark` holds the geometry, the
//! timing and the rasteriser — all of it platform-free and tested on Linux CI, which this crate
//! never gets.
//!
//! Deliberately bounded to the launch. An icon that keeps animating is a timer that keeps firing,
//! and the shell holds itself to 5% CPU while idle; this is ~25 frames on one worker thread inside
//! the first second, and then nothing. The Dock hands back to the real bundle icon rather than
//! ending on a frame of ours, so the last thing the animation does is stop existing.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use shogun_mark::{Placement, DURATION_MS};
    use tauri::AppHandle;

    /// The Dock draws up to 128pt; 256px covers the 2x case without paying for 512.
    const DOCK_PX: u32 = 256;
    /// The menu bar draws 22pt, and the shipped tray asset is 44px for the 2x case.
    const TRAY_PX: u32 = 44;
    /// ~31ms a frame. Enough to read as motion, few enough that the whole arrival is 25 draws.
    const FRAMES: u32 = 24;

    /// Measured off icons/icon-512.png: the mark spans 338 of the plate's 512 pixels, centred.
    /// `shogun-mark`'s own test redraws that icon from this number and compares it to the file.
    const DOCK_PLACEMENT: Placement = Placement::new(338.0 / 512.0);
    /// And off icons/tray-icon@2x.png: 38 of 44.
    const TRAY_PLACEMENT: Placement = Placement::new(38.0 / 44.0);

    /// The plate the Dock icon's mark sits on — one flat fill, sampled from the shipped artwork.
    const PLATE: [u8; 3] = [215, 215, 215];
    /// Brand blue, the value Logo.tsx and the artwork share.
    const MARK: [u8; 3] = [0x00, 0x4c, 0xfc];

    /// The shipped icon, for its alpha: the plate's squircle, including how it is anti-aliased.
    /// Redrawing that silhouette by hand would mean guessing at a corner curve Apple does not
    /// document; borrowing it costs one decode and is exact.
    const ICON_PNG: &[u8] = include_bytes!("../icons/icon-256.png");
    /// What the menu bar goes back to.
    const TRAY_PNG: &[u8] = include_bytes!("../icons/tray-icon@2x.png");

    static PLAYED: AtomicBool = AtomicBool::new(false);

    /// Fold both icons in. Returns immediately; the frames are drawn on a worker thread.
    pub fn play(app: &AppHandle) {
        if PLAYED.swap(true, Ordering::SeqCst) {
            return; // one launch, one arrival
        }
        if reduce_motion() {
            eprintln!("[mark] reduced motion — icons stay still");
            return;
        }

        // No Dock icon under the Accessory policy, so there is nothing to fold there.
        let dock = crate::dock_visibility::load_settings(app).visible;
        let plate = if dock { plate_alpha() } else { None };
        if dock && plate.is_none() {
            eprintln!("[mark] icon-256.png unreadable — the Dock keeps its still icon");
        }

        let app = app.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            for frame in 0..=FRAMES {
                let ms = DURATION_MS * frame as f32 / FRAMES as f32;

                if let Some(plate) = plate.as_ref() {
                    if let Some(png) = dock_frame(ms, plate) {
                        set_dock_icon(&app, Some(png));
                    }
                }
                set_tray_icon(&app, tray_frame(ms), TRAY_PX, TRAY_PX);

                // An absolute schedule, not a fixed sleep: a slow frame costs itself, not the
                // whole arrival, so the icons and the window that opens behind them stay in step.
                let due = Duration::from_secs_f32(DURATION_MS / 1000.0 * (frame + 1) as f32
                    / FRAMES as f32);
                if let Some(left) = due.checked_sub(start.elapsed()) {
                    std::thread::sleep(left);
                }
            }

            // Hand the real icons back. The Dock's is the bundle's own, which is the one place
            // this cannot end on a picture of ours that is a pixel off the one that ships.
            set_dock_icon(&app, None);
            set_tray_icon(&app, TRAY_PNG.to_vec(), 0, 0);
        });
    }

    /// Does this Mac's owner want less movement? The same question the stylesheet asks, asked of
    /// the OS instead of the webview.
    fn reduce_motion() -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }

    /// The shipped icon's alpha channel at [`DOCK_PX`], or None if it will not decode.
    fn plate_alpha() -> Option<Vec<u8>> {
        let icon = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
            .ok()?
            .to_rgba8();
        if icon.dimensions() != (DOCK_PX, DOCK_PX) {
            eprintln!("[mark] icon-256.png is {:?}, not {DOCK_PX}² — skipping", icon.dimensions());
            return None;
        }
        Some(icon.pixels().map(|p| p.0[3]).collect())
    }

    /// One Dock frame: the plate, with the mark painted into it in brand blue.
    fn dock_frame(ms: f32, plate: &[u8]) -> Option<Vec<u8>> {
        let mark = shogun_mark::unfold_alpha(ms, DOCK_PX, DOCK_PX, DOCK_PLACEMENT);
        let mut rgba = Vec::with_capacity(plate.len() * 4);
        for (plate_a, mark_a) in plate.iter().zip(mark.iter()) {
            let k = *mark_a as u16;
            for channel in 0..3 {
                let under = PLATE[channel] as u16 * (255 - k);
                let over = MARK[channel] as u16 * k;
                rgba.push(((under + over) / 255) as u8);
            }
            rgba.push(*plate_a);
        }
        to_png(rgba, DOCK_PX, DOCK_PX)
    }

    /// One menu-bar frame. The tray is a template image, so only the alpha carries — macOS tints
    /// the silhouette itself, light menu bar or dark.
    fn tray_frame(ms: f32) -> Vec<u8> {
        shogun_mark::unfold_alpha(ms, TRAY_PX, TRAY_PX, TRAY_PLACEMENT)
            .into_iter()
            .flat_map(|a| [0, 0, 0, a])
            .collect()
    }

    fn to_png(rgba: Vec<u8>, width: u32, height: u32) -> Option<Vec<u8>> {
        let image = image::RgbaImage::from_raw(width, height, rgba)?;
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .ok()?;
        Some(out.into_inner())
    }

    /// `None` restores the bundle's own icon.
    fn set_dock_icon(app: &AppHandle, png: Option<Vec<u8>>) {
        let _ = app.run_on_main_thread(move || {
            // `AnyThread` is what puts `alloc()` in scope, the same way sound.rs reaches NSSound.
            use objc2::AnyThread;
            use objc2_app_kit::{NSApplication, NSImage};
            use objc2_foundation::NSData;

            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return;
            };
            let image = png.and_then(|bytes| {
                let data = NSData::with_bytes(&bytes);
                NSImage::initWithData(NSImage::alloc(), &data)
            });
            // SAFETY: the shared NSApplication on the main thread; AppKit documents nil here as
            // "go back to the bundle icon", which is exactly what the end of the fold wants.
            unsafe {
                NSApplication::sharedApplication(mtm).setApplicationIconImage(image.as_deref());
            }
        });
    }

    /// Raw RGBA when `width` and `height` are given, an encoded PNG when they are zero.
    ///
    /// A missing tray is not an error here: bootstrap treats "the menu-bar icon failed to install"
    /// as a survivable degradation, so an arrival with nowhere to play is just skipped.
    fn set_tray_icon(app: &AppHandle, bytes: Vec<u8>, width: u32, height: u32) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(tray) = handle.tray_by_id("shogun-tray") else {
                return;
            };
            let image = if width == 0 {
                tauri::image::Image::from_bytes(&bytes).ok()
            } else {
                Some(tauri::image::Image::new_owned(bytes, width, height))
            };
            if let Some(image) = image {
                let _ = tray.set_icon(Some(image));
            }
        });
    }

    /// Wired from `setup_macos`, after the tray exists.
    pub fn init(app: &tauri::App) {
        play(app.handle());
    }
}

#[cfg(not(target_os = "macos"))]
pub mod mac {
    pub fn init(_app: &tauri::App) {}
}
