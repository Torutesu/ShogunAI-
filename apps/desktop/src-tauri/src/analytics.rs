//! Tauri シェル側の分析アダプタ。`analytics.json`（distinct_id + opt_out）の永続化、
//! distinct_id の生成、共通プロパティ組み立て、ハンドル生成、opt-out コマンドを担う。
//!
//! 送信ロジック本体は `shogun_core::analytics`。ここは OS/設定/配線だけ。

use serde::{Deserialize, Serialize};

/// `analytics.json` の内容（非シークレット）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsState {
    /// 匿名の永続 distinct_id（UUIDv4 文字列）。
    pub distinct_id: String,
    /// テレメトリ送信を止めるか（既定 false = 送信ON）。
    #[serde(default)]
    pub opt_out: bool,
}

/// OS CSPRNG から UUIDv4 文字列を生成する（getrandom、シェルに既存の乱数源）。
pub fn new_distinct_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| format!("csprng failed: {e}"))?;
    // version 4 / variant 10xx
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_id_is_uuid_v4_shaped() {
        let id = new_distinct_id().unwrap();
        // 8-4-4-4-12 の 36 文字
        assert_eq!(id.len(), 36);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        // version nibble = 4
        assert_eq!(&id[14..15], "4");
        // variant nibble ∈ {8,9,a,b}
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn two_ids_differ() {
        assert_ne!(new_distinct_id().unwrap(), new_distinct_id().unwrap());
    }

    #[test]
    fn state_roundtrips_json_with_opt_out_default_false() {
        let json = r#"{"distinct_id":"x"}"#;
        let s: AnalyticsState = serde_json::from_str(json).unwrap();
        assert_eq!(s.distinct_id, "x");
        assert!(!s.opt_out); // #[serde(default)]
    }
}
