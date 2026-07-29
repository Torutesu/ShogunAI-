//! `Shougun.md` のパース・注入基盤（純粋ロジック）。

pub mod model;
pub mod parse;

pub use model::{
    Charm, ParseReport, Profile, RawSection, SectionError, ShougunConfig, Style, Workflow,
};
pub use parse::parse_shougun;
