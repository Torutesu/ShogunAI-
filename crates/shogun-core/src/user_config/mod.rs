//! `Shougun.md` のパース・注入基盤（純粋ロジック）。

pub mod directives;
pub mod model;
pub mod parse;

pub use directives::render_directives;
pub use model::{
    Charm, ParseReport, Profile, RawSection, SectionError, ShougunConfig, Style, Workflow,
};
pub use parse::parse_shougun;
