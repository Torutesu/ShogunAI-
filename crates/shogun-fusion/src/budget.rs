//! ローカルなトークン推定と予算充填（Issue #63）。
//!
//! `shogun-fusion` はクラウド/ONNX に依存しない純粋クレートなので、トークン数は言語別の
//! char→token 比のヒューリスティックで見積もる（±10% で予算管理には十分）。正確な
//! トークナイザが必要になったら [`TokenEstimator`] を差し替える。

/// テキストのトークン数を見積もる seam。
pub trait TokenEstimator {
    fn count(&self, text: &str) -> usize;
}

/// 言語別 char→token 比のローカル推定器。CJK は文字あたりのトークンが多く、ラテンは
/// 単語あたり複数文字なので、CJK 文字比率で 2 つの比率を線形補間する。
#[derive(Debug, Clone, Copy)]
pub struct HeuristicEstimator {
    /// ラテン系: 1 トークンあたりの文字数（概ね 4）。
    latin_chars_per_token: f64,
    /// CJK 系: 1 トークンあたりの文字数（概ね 1.5）。
    cjk_chars_per_token: f64,
}

impl Default for HeuristicEstimator {
    fn default() -> Self {
        Self { latin_chars_per_token: 4.0, cjk_chars_per_token: 1.5 }
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF |   // ひらがな・カタカナ
        0x3400..=0x4DBF |   // CJK 拡張A
        0x4E00..=0x9FFF |   // CJK 統合漢字
        0xFF00..=0xFFEF)    // 全角
}

impl TokenEstimator for HeuristicEstimator {
    fn count(&self, text: &str) -> usize {
        let total = text.chars().count();
        if total == 0 {
            return 0;
        }
        let cjk = text.chars().filter(|c| is_cjk(*c)).count();
        let cjk_ratio = cjk as f64 / total as f64;
        let chars_per_token =
            self.cjk_chars_per_token * cjk_ratio + self.latin_chars_per_token * (1.0 - cjk_ratio);
        (total as f64 / chars_per_token).ceil() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_zero_tokens() {
        assert_eq!(HeuristicEstimator::default().count(""), 0);
    }

    #[test]
    fn latin_uses_about_four_chars_per_token() {
        // 40 文字のラテン → 約 10 トークン。
        let s = "a".repeat(40);
        let n = HeuristicEstimator::default().count(&s);
        assert!((9..=11).contains(&n), "got {n}");
    }

    #[test]
    fn cjk_costs_more_tokens_than_latin_for_same_length() {
        let est = HeuristicEstimator::default();
        let latin = est.count(&"a".repeat(30));
        let cjk = est.count(&"あ".repeat(30));
        assert!(cjk > latin, "cjk={cjk} latin={latin}");
    }
}
