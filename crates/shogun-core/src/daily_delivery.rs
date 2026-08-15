//! 朝・夜サマリーの配達判定（Issue #10, docs/daily-summaries-design.md §2）。
//!
//! 原則: **時刻に割り込まず、「その時刻を過ぎた後、ユーザーがそこにいる最初の瞬間」に出す**。
//! この判定は純粋関数 — 時計もファイルも読まない。シェル（macOS 側）はアクティビティの
//! たびに [`due`] を呼び、`Some` が返ったらハンドル notice を灯し、カードが開かれたら
//! [`SeenState`] の該当日付を進める。
//!
//! テストが固定する境界: 日付跨ぎ・deep-sleep 明け・夜時刻を過去へ変更した当日の再判定なし。

use serde::{Deserialize, Serialize};

/// どちらのサマリーが配達可能になったか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Morning,
    Evening,
}

/// ユーザー設定（Settings → Daily summaries）。`daily_summaries.json` に永続化（非秘匿）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(default = "default_true")]
    pub morning_enabled: bool,
    #[serde(default = "default_true")]
    pub evening_enabled: bool,
    /// 夜の配達開始時刻（ローカル、時）。既定 17。
    #[serde(default = "default_evening_hour")]
    pub evening_hour: u8,
    /// 同、分。既定 30。
    #[serde(default = "default_evening_minute")]
    pub evening_minute: u8,
}

fn default_true() -> bool {
    true
}
fn default_evening_hour() -> u8 {
    17
}
fn default_evening_minute() -> u8 {
    30
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            morning_enabled: true,
            evening_enabled: true,
            evening_hour: 17,
            evening_minute: 30,
        }
    }
}

/// 既読状態。日付は `YYYY-MM-DD`（ローカル日、`local_date_string` と同じ形）。
/// 「その日ぶんは配達済み/既読」を日付一致で表す — カウンタでも bool でもなく日付なのは、
/// 日付跨ぎのリセットを比較1つで済ませ、時計が戻っても二重配達しないため。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeenState {
    #[serde(default)]
    pub morning_seen_date: Option<String>,
    #[serde(default)]
    pub evening_seen_date: Option<String>,
}

/// 現在のローカル時刻（シェルが供給。時計はここでは読まない）。
#[derive(Debug, Clone, Copy)]
pub struct LocalNow<'a> {
    /// `YYYY-MM-DD`。
    pub date: &'a str,
    pub hour: u8,
    pub minute: u8,
}

/// アクティビティの瞬間に呼ぶ配達判定。
///
/// - Morning: その日まだ既読でなければ、最初の呼び出しで `Some(Morning)`（= その日最初の
///   アクティビティ。呼び出し自体がアクティビティ起点なので時刻条件は無い）
/// - Evening: 設定時刻以降、その日まだ既読でなければ `Some(Evening)`
/// - 朝と夜が同時に成立する場合（夜設定時刻後に初めて Mac を開いた日）は **Evening を優先**
///   — 一日の終わりに朝の予定表を出しても仕事は終わっている。Morning はその日の既読扱いにする
///   かは呼び出し側の自由だが、既定では触らず、翌日に自然リセットされる
pub fn due(now: LocalNow<'_>, settings: &Settings, seen: &SeenState) -> Option<Which> {
    let evening_due = settings.evening_enabled
        && seen.evening_seen_date.as_deref() != Some(now.date)
        && (now.hour, now.minute) >= (settings.evening_hour, settings.evening_minute);
    if evening_due {
        return Some(Which::Evening);
    }
    let morning_due =
        settings.morning_enabled && seen.morning_seen_date.as_deref() != Some(now.date);
    if morning_due {
        return Some(Which::Morning);
    }
    None
}

/// カードが開かれた（既読）。該当側の日付を今日に進める。
pub fn mark_seen(seen: &mut SeenState, which: Which, date: &str) {
    match which {
        Which::Morning => seen.morning_seen_date = Some(date.to_string()),
        Which::Evening => seen.evening_seen_date = Some(date.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(date: &str, hour: u8, minute: u8) -> LocalNow<'_> {
        LocalNow { date, hour, minute }
    }

    #[test]
    fn the_first_activity_of_a_day_delivers_the_morning() {
        let s = Settings::default();
        let mut seen = SeenState::default();
        assert_eq!(due(now("2026-08-15", 8, 4), &s, &seen), Some(Which::Morning));
        mark_seen(&mut seen, Which::Morning, "2026-08-15");
        // 同日中は二度出ない
        assert_eq!(due(now("2026-08-15", 9, 0), &s, &seen), None);
        // 日付が変われば自然リセット
        assert_eq!(due(now("2026-08-16", 7, 0), &s, &seen), Some(Which::Morning));
    }

    #[test]
    fn the_evening_waits_for_its_time_then_fires_on_next_activity() {
        let s = Settings::default(); // 17:30
        let mut seen = SeenState::default();
        mark_seen(&mut seen, Which::Morning, "2026-08-15");
        assert_eq!(due(now("2026-08-15", 17, 29), &s, &seen), None, "not yet");
        assert_eq!(due(now("2026-08-15", 17, 30), &s, &seen), Some(Which::Evening), "inclusive");
        // deep-sleep 明けで 21:00 に初めて触っても出る（不在中に消えない）
        assert_eq!(due(now("2026-08-15", 21, 0), &s, &seen), Some(Which::Evening));
        mark_seen(&mut seen, Which::Evening, "2026-08-15");
        assert_eq!(due(now("2026-08-15", 22, 0), &s, &seen), None);
    }

    #[test]
    fn evening_outranks_a_stale_morning_after_hours() {
        // 夜設定時刻の後にその日初めて Mac を開いた: 朝の予定表より一日の締めを出す。
        let s = Settings::default();
        let seen = SeenState::default();
        assert_eq!(due(now("2026-08-15", 19, 0), &s, &seen), Some(Which::Evening));
    }

    #[test]
    fn moving_the_evening_time_into_the_past_does_not_redeliver_today() {
        // 17:30 で既読 → 設定を 12:00 に変更。当日は再判定しない（日付一致で抑制）。
        let mut s = Settings::default();
        let mut seen = SeenState::default();
        mark_seen(&mut seen, Which::Morning, "2026-08-15");
        mark_seen(&mut seen, Which::Evening, "2026-08-15");
        s.evening_hour = 12;
        s.evening_minute = 0;
        assert_eq!(due(now("2026-08-15", 13, 0), &s, &seen), None);
        // 翌日は新しい時刻で動く
        mark_seen(&mut seen, Which::Morning, "2026-08-16");
        assert_eq!(due(now("2026-08-16", 12, 0), &s, &seen), Some(Which::Evening));
    }

    #[test]
    fn disabled_sides_never_fire() {
        let s = Settings {
            morning_enabled: false,
            evening_enabled: false,
            ..Settings::default()
        };
        assert_eq!(due(now("2026-08-15", 8, 0), &s, &SeenState::default()), None);
        assert_eq!(due(now("2026-08-15", 20, 0), &s, &SeenState::default()), None);
    }

    #[test]
    fn settings_json_missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        let seen: SeenState = serde_json::from_str("{}").unwrap();
        assert_eq!(seen, SeenState::default());
    }
}
