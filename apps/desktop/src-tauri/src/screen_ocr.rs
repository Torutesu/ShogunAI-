//! On-device screen OCR (issue #106/#107, decision B: OCR-then-discard).
//!
//! Capture + OCR path mirrored from Screenpipe (`screenpipe-screen` + `screenpipe-capture`):
//! CGWindow capture → RAM `DynamicImage` → text-region gate → Apple Vision on crop → text only.
//! **No JPEG/PNG timeline storage** (CLAUDE.md invariant 2). Screenpipe Commercial License
//! prevents vendoring their sources; architecture and thresholds match their public crates.
//! Reference: https://github.com/screenpipe/screenpipe

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::CGRectNull;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::image::CGImage;
use core_graphics::window::{
    kCGNullWindowID, kCGWindowImageBoundsIgnoreFraming, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionIncludingWindow, kCGWindowListOptionOnScreenOnly, CGWindowListCopyWindowInfo,
    CGWindowListCreateImage,
};
use foreign_types::ForeignType;
use image::{DynamicImage, ImageBuffer};
use objc2::AnyThread;
use objc2::runtime::AnyObject;
use objc2_core_graphics::CGImage as VisionImage;
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizedTextObservation, VNRecognizeTextRequest, VNRequest,
    VNRequestTextRecognitionLevel,
};

use crate::visual_recall::pipeline::{self, RecallPipeline};

/// Minimum dwell between OCR passes on the same focus (respect idle CPU SLO spirit).
pub const MIN_OCR_INTERVAL_MS: u64 = 10_000;

/// CGWindow id for the frontmost normal window owned by `pid`, if any.
pub fn focused_window_id(pid: i32) -> Option<u32> {
    // SAFETY: standard window-list query; result is +1 CFArray.
    let list = unsafe {
        CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        )
    };
    if list.is_null() {
        return None;
    }
    // SAFETY: list is a valid CFArray from the create rule above.
    let array = unsafe {
        CFArray::<CFDictionary<CFString, *const std::ffi::c_void>>::wrap_under_create_rule(list as _)
    };
    for i in 0..array.len() {
        let dict = array.get(i)?;
        let owner = dict
            .find(kCGWindowOwnerPID())
            .and_then(|v| cf_number_i32(&*v))
            .unwrap_or(0);
        if owner != pid {
            continue;
        }
        let layer = dict
            .find(kCGWindowLayer())
            .and_then(|v| cf_number_i32(&*v))
            .unwrap_or(-1);
        if layer != 0 {
            continue;
        }
        let wid = dict.find(kCGWindowNumber()).and_then(|v| cf_number_u32(&*v))?;
        return Some(wid);
    }
    None
}

/// RAM capture of the focused window: CGImage (for Vision crop) + DynamicImage (for text-region gate).
pub fn capture_focused_window(pid: i32) -> Option<(CGImage, DynamicImage)> {
    let window_id = focused_window_id(pid)?;
    capture_window(window_id)
}

/// Capture one on-screen window by CGWindow id (Screenpipe uses sck-rs/xcap; we use CGWindow
/// for a single focused window — same RAM-only contract, smaller scope for SHOGUN).
pub fn capture_window(window_id: u32) -> Option<(CGImage, DynamicImage)> {
    // SAFETY: Screen Recording permission required; NULL on denial.
    let raw = unsafe {
        CGWindowListCreateImage(
            CGRectNull,
            kCGWindowListOptionIncludingWindow,
            window_id,
            kCGWindowImageBoundsIgnoreFraming,
        )
    };
    if raw.is_null() {
        return None;
    }
    // SAFETY: wrap the +1 CGImage from CreateImage; owned by `cg` until dropped.
    let cg = unsafe { CGImage::from_ptr(raw) };
    let frame = cg_image_to_dynamic(&cg)?;
    Some((cg, frame))
}

/// Gated OCR on the focused window — Screenpipe `paired_capture` OCR path without storage.
pub fn ocr_focused_window_gated(
    pipeline: &mut RecallPipeline,
    pid: i32,
    app_key: &str,
    bundle_or_app: &str,
    window_title: Option<&str>,
    ax_empty: bool,
    ax_text_len: usize,
    meeting_active: bool,
) -> pipeline::OcrOutcome {
    let Some((cg, frame)) = capture_focused_window(pid) else {
        return pipeline::OcrOutcome::Skipped;
    };
    let trigger =
        pipeline::wants_ocr(bundle_or_app, window_title, ax_empty, ax_text_len, meeting_active);
    pipeline.ocr_gated_window(&frame, app_key, trigger, |_, crop| {
        let rect = CGRect::new(
            &CGPoint::new(f64::from(crop.x), f64::from(crop.y)),
            &CGSize::new(f64::from(crop.width), f64::from(crop.height)),
        );
        cg.cropped(rect).and_then(|sub| ocr_cg_image(&sub))
    })
}

/// Apple Vision OCR on a CGImage crop (Screenpipe `perform_ocr_apple` semantics, flat text).
fn ocr_cg_image(image: &CGImage) -> Option<String> {
    // SAFETY: both CGImage types are transparent refs to the same CoreGraphics object.
    let vision_image = unsafe { &*(image.as_ptr() as *const VisionImage) };
    let options = NSDictionary::<objc2_vision::VNImageOption, AnyObject>::new();
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            vision_image,
            &options,
        )
    };
    let request = unsafe {
        let req = VNRecognizeTextRequest::init(VNRecognizeTextRequest::alloc());
        req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        req.setUsesLanguageCorrection(true);
        req
    };
    let requests = NSArray::from_slice(&[&*request as &VNRequest]);
    handler.performRequests_error(&requests).ok()?;
    let observations = request.results()?;
    let mut lines: Vec<String> = Vec::new();
    for obs in observations.iter() {
        let Some(text_obs) = obs.downcast_ref::<VNRecognizedTextObservation>() else {
            continue;
        };
        let candidates = text_obs.topCandidates(1);
        if let Some(best) = candidates.firstObject() {
            let line = best.string().to_string();
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

fn cg_image_to_dynamic(image: &CGImage) -> Option<DynamicImage> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return None;
    }
    let row_bytes = image.bytes_per_row();
    let bpp = image.bits_per_pixel() / 8;
    if bpp < 3 {
        return None;
    }
    let data = image.data();
    let bytes = data.to_vec();
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row_start = y * row_bytes;
        for x in 0..width {
            let i = row_start + x * bpp;
            if i + 2 >= bytes.len() {
                return None;
            }
            match bpp {
                4 => {
                    rgba.push(bytes[i + 2]);
                    rgba.push(bytes[i + 1]);
                    rgba.push(bytes[i]);
                    rgba.push(bytes[i + 3]);
                }
                3 => {
                    rgba.push(bytes[i]);
                    rgba.push(bytes[i + 1]);
                    rgba.push(bytes[i + 2]);
                    rgba.push(255);
                }
                _ => return None,
            }
        }
    }
    ImageBuffer::from_raw(width as u32, height as u32, rgba).map(DynamicImage::ImageRgba8)
}

fn kCGWindowOwnerPID() -> CFString {
    CFString::from_static_string("kCGWindowOwnerPID")
}

fn kCGWindowLayer() -> CFString {
    CFString::from_static_string("kCGWindowLayer")
}

fn kCGWindowNumber() -> CFString {
    CFString::from_static_string("kCGWindowNumber")
}

fn cf_number_i32(v: &*const std::ffi::c_void) -> Option<i32> {
    if v.is_null() {
        return None;
    }
    // SAFETY: window-list values for PID/layer are CFNumbers.
    let n = unsafe { CFNumber::wrap_under_get_rule(*v as _) };
    n.to_i32()
}

fn cf_number_u32(v: &*const std::ffi::c_void) -> Option<u32> {
    if v.is_null() {
        return None;
    }
    let n = unsafe { CFNumber::wrap_under_get_rule(*v as _) };
    n.to_i32().and_then(|i| u32::try_from(i).ok())
}

/// Digest for telemetry — never log full OCR bodies.
pub fn text_digest(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = twox_hash::XxHash64::with_seed(0);
    text.len().hash(&mut h);
    text.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_changes_with_content() {
        let a = text_digest("hello");
        let b = text_digest("world");
        assert_ne!(a, b);
    }
}
