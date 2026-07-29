//! `Shougun.md` のパース・注入基盤（純粋ロジック）。

pub mod model;

pub use model::{
    Charm, ParseReport, Profile, RawSection, SectionError, ShougunConfig, Style, Workflow,
};
