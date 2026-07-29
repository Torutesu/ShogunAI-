//! `Shougun.md` の行ベースパーサ（fail-soft）。

/// 見出し `# X` で本文を分割する。戻り値は (heading, start_line, body_lines)。
/// `#` 1個の ATX 見出しのみをセクション境界とする（`##` 以下は本文扱い）。
pub(crate) fn split_sections(input: &str) -> Vec<(String, usize, Vec<String>)> {
    let mut out: Vec<(String, usize, Vec<String>)> = Vec::new();
    let mut cur: Option<(String, usize, Vec<String>)> = None;
    for (i, raw) in input.lines().enumerate() {
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some(sec) = cur.take() {
                out.push(sec);
            }
            cur = Some((rest.trim().to_string(), i + 1, Vec::new()));
        } else if let Some((_, _, body)) = cur.as_mut() {
            body.push(line.to_string());
        }
    }
    if let Some(sec) = cur.take() {
        out.push(sec);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_h1_headings_only() {
        let md = "# Profile\n- Role: PM\n\n# Style\n- Tone: warm\n## Sub\ntext";
        let secs = split_sections(md);
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].0, "Profile");
        assert_eq!(secs[0].1, 1); // line number (1-based)
        assert_eq!(secs[1].0, "Style");
        // `## Sub` は Style の本文として残る
        assert!(secs[1].2.iter().any(|l| l == "## Sub"));
    }

    #[test]
    fn text_before_first_heading_is_ignored() {
        let secs = split_sections("intro text\n# Profile\n- Role: PM");
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].0, "Profile");
    }
}
