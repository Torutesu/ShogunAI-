//! Geometry adapter (spec §3.2, §3.4.7).
//!
//! The math lives in `spike_core::geometry` (Rect/Regions/idle_rect/regions/cg_to_ns,
//! unit-tested on Linux). on-device (T-06) this module only reads the raw macOS values —
//! `NSScreen.safeAreaInsets.top`, `auxiliaryTopLeftArea/RightArea` (research item 4:
//! `NSRect?`, nil = no notch, bottom-left origin) and menubar height — and feeds them into
//! `spike_core::geometry::regions(...)`. CGEvent points are normalised with
//! `cg_to_ns(p, primary_height)` at the module boundary.
#![allow(dead_code)]

pub use spike_core::geometry::{cg_to_ns, idle_rect, regions, GeometryParams, Point, Rect, Regions};
