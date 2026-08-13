//! `Shougun.md` のパース・注入基盤（純粋ロジック）。

pub mod directives;
pub mod model;
pub mod parse;
pub mod sample;

pub use directives::{
    render_directives, render_directives_with_lessons, render_learned_section, LearnedLesson,
};
pub use model::{
    Charm, ParseReport, Profile, RawSection, SectionError, ShougunConfig, Style, Workflow,
};
pub use parse::parse_shougun;
pub use sample::sample_markdown;

use std::path::{Path, PathBuf};

/// 既定のファイルパス（ホーム直下 `~/Shougun.md`）。
pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Shougun.md"))
}

/// ファイルを読み、無ければサンプルを書き出す。
/// 戻り値: (パース済み設定, 新規作成したか)。
pub fn load_or_create(path: &Path) -> std::io::Result<(ShougunConfig, bool)> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let (cfg, _report) = parse_shougun(&content);
        Ok((cfg, false))
    } else {
        let sample = sample_markdown();
        std::fs::write(path, &sample)?;
        let (cfg, _report) = parse_shougun(&sample);
        Ok((cfg, true))
    }
}

/// ファイルを読んでパース結果とレポートを返す（存在しなければ空＋ok）。
pub fn load_report(path: &Path) -> std::io::Result<(ShougunConfig, ParseReport)> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(parse_shougun(&content))
    } else {
        Ok((ShougunConfig::default(), ParseReport { ok: true, section_errors: Vec::new() }))
    }
}

#[cfg(test)]
mod io_tests {
    use super::*;

    #[test]
    fn load_or_create_writes_sample_when_missing() {
        let dir = std::env::temp_dir().join(format!("shougun_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Shougun.md");
        let _ = std::fs::remove_file(&path);

        let (cfg, created) = load_or_create(&path).expect("load_or_create");
        assert!(created, "missing file should be created");
        assert!(path.exists());
        // 2回目は作成しない
        let (_cfg2, created2) = load_or_create(&path).expect("second load");
        assert!(!created2);
        let _ = cfg; // 使用済み扱い
        let _ = std::fs::remove_dir_all(&dir);
    }
}
