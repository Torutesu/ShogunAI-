use super::*;
use crate::meeting::statemachine::EndReason;

const SELF_BUNDLE_ID: &str = "com.selectkk.shogun";

fn live(now: i64) -> LiveSignals {
    LiveSignals {
        meeting_app_present: true,
        occurrence_ends_at: None,
        last_sound_at: now,
    }
}

#[test]
fn a_meeting_in_progress_keeps_running() {
    let now = 1_000_000;
    assert_eq!(end_condition(&live(now), now), None);
}

#[test]
fn the_meeting_app_disappearing_ends_the_meeting() {
    let now = 1_000_000;
    let s = LiveSignals {
        meeting_app_present: false,
        ..live(now)
    };
    assert_eq!(end_condition(&s, now), Some(EndReason::AppGone));
}

#[test]
fn silence_past_the_limit_ends_the_meeting() {
    let now = 1_000_000;
    let s = LiveSignals {
        last_sound_at: now - SILENCE_LIMIT_MS - 1,
        ..live(now)
    };
    assert_eq!(end_condition(&s, now), Some(EndReason::Silence));
}

#[test]
fn a_quiet_stretch_short_of_the_limit_does_not_end_the_meeting() {
    // Someone listening to a long presentation is not a meeting that has finished.
    let now = 1_000_000;
    let s = LiveSignals {
        last_sound_at: now - SILENCE_LIMIT_MS + 1,
        ..live(now)
    };
    assert_eq!(end_condition(&s, now), None);
}

#[test]
fn a_meeting_running_well_past_its_slot_ends() {
    let now = 1_000_000;
    let s = LiveSignals {
        occurrence_ends_at: Some(now - OCCURRENCE_GRACE_MS - 1),
        ..live(now)
    };
    assert_eq!(end_condition(&s, now), Some(EndReason::OccurrenceOver));
}

#[test]
fn a_meeting_that_merely_runs_over_is_left_alone() {
    // Meetings overrun. Cutting the notes off at the scheduled end would lose exactly the
    // part people stay behind for, so the grace is generous (10 minutes).
    let now = 1_000_000;
    let s = LiveSignals {
        occurrence_ends_at: Some(now - 60_000),
        ..live(now)
    };
    assert_eq!(end_condition(&s, now), None);
}

#[test]
fn the_app_going_away_wins_over_a_slower_condition() {
    // Both true at once: report the one that actually happened first, so Recap and the health
    // metrics do not learn "silence" for a meeting the user simply quit.
    let now = 1_000_000;
    let s = LiveSignals {
        meeting_app_present: false,
        last_sound_at: now - SILENCE_LIMIT_MS - 1,
        occurrence_ends_at: Some(now - OCCURRENCE_GRACE_MS - 1),
    };
    assert_eq!(end_condition(&s, now), Some(EndReason::AppGone));
}

fn confidence_of(d: &Decision) -> f64 {
    match d {
        Decision::Offer { confidence, .. } => *confidence,
        Decision::Ignore => panic!("expected an offer, got Ignore"),
    }
}

#[test]
fn a_calendar_entry_alone_does_not_open_an_interval() {
    // The rule that keeps SHOGUN from listening to an empty room: a diary entry is not
    // attendance (FR-MT-04).
    let d = decide(&Signals {
        occurrence_now: true,
        ..Default::default()
    });
    assert_eq!(d, Decision::Ignore);
}

#[test]
fn nothing_observed_is_ignored() {
    assert_eq!(decide(&Signals::default()), Decision::Ignore);
}

#[test]
fn a_frontmost_meeting_app_is_enough_to_offer() {
    let d = decide(&Signals {
        meeting_app_frontmost: true,
        ..Default::default()
    });
    assert!(matches!(d, Decision::Offer { .. }));
}

#[test]
fn visible_meeting_controls_are_enough_to_offer() {
    let d = decide(&Signals {
        meeting_controls_visible: true,
        ..Default::default()
    });
    assert!(matches!(d, Decision::Offer { .. }));
}

#[test]
fn a_scheduled_occurrence_raises_confidence_in_what_was_observed() {
    let observed = Signals {
        meeting_app_frontmost: true,
        ..Default::default()
    };
    let corroborated = Signals {
        occurrence_now: true,
        ..observed
    };

    assert!(
        confidence_of(&decide(&corroborated)) > confidence_of(&decide(&observed)),
        "signal (1) must corroborate (2)/(3), even though it cannot stand alone"
    );
}

#[test]
fn more_agreeing_signals_mean_more_confidence() {
    let one = decide(&Signals {
        meeting_app_frontmost: true,
        ..Default::default()
    });
    let two = decide(&Signals {
        meeting_app_frontmost: true,
        meeting_controls_visible: true,
        ..Default::default()
    });
    assert!(confidence_of(&two) > confidence_of(&one));
}

#[test]
fn confidence_never_reaches_certainty() {
    // Detection is inference. Even with everything agreeing it stays below 1.0, so nothing
    // downstream can treat a detected meeting as a fact (FR-MT-04, FR-ST-02).
    let all = decide(&Signals {
        meeting_app_frontmost: true,
        mic_in_use: true,
        meeting_controls_visible: true,
        occurrence_now: true,
    });
    let c = confidence_of(&all);
    assert!(c < 1.0, "confidence was {c}");
    assert!(c > 0.0);
}

#[test]
fn the_offer_records_which_signals_fired() {
    // Provenance is what makes a wrong detection explainable rather than merely annoying.
    let d = decide(&Signals {
        meeting_app_frontmost: true,
        mic_in_use: true,
        ..Default::default()
    });
    let Decision::Offer { provenance, .. } = d else {
        panic!("expected an offer")
    };

    assert!(provenance.contains("meeting_app_frontmost"));
    assert!(provenance.contains("mic_sustained"));
    assert!(
        !provenance.contains("occurrence_now"),
        "a signal that did not fire is not evidence"
    );
    serde_json::from_str::<serde_json::Value>(&provenance).expect("provenance must be JSON");
}

#[test]
fn a_sustained_microphone_is_a_meeting_on_its_own() {
    // `decide` still scores mic-only; product policy gates it unless opted in.
    let d = decide(&Signals {
        mic_in_use: true,
        ..Default::default()
    });
    assert!(matches!(d, Decision::Offer { .. }));
}

#[test]
fn mic_only_is_blocked_by_default_policy() {
    let signals = Signals {
        mic_in_use: true,
        ..Default::default()
    };
    let ctx = DetectionCtx::default();
    let policy = OfferPolicy::default();
    assert_eq!(evaluate_offer(&signals, &ctx, &policy), Decision::Ignore);
}

#[test]
fn mic_only_offers_when_opted_in() {
    let signals = Signals {
        mic_in_use: true,
        ..Default::default()
    };
    let ctx = DetectionCtx::default();
    let policy = OfferPolicy {
        allow_mic_only: true,
    };
    assert!(matches!(
        evaluate_offer(&signals, &ctx, &policy),
        Decision::Offer { .. }
    ));
}

#[test]
fn youtube_url_is_not_a_meeting_and_mic_only_blocked() {
    assert!(!is_meeting_url("https://www.youtube.com/watch?v=abc"));
    assert!(is_media_url("https://www.youtube.com/watch?v=abc"));
    let signals = Signals {
        mic_in_use: mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, true),
        ..Default::default()
    };
    let ctx = DetectionCtx {
        is_browser: true,
        page_host: Some("www.youtube.com"),
        window_title: Some("Rick Astley - YouTube"),
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn empty_host_chrome_with_mic_does_not_offer() {
    let signals = Signals {
        mic_in_use: mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, false),
        ..Default::default()
    };
    let ctx = DetectionCtx {
        is_browser: true,
        page_host: Some(""),
        window_title: Some("New tab - Google Chrome"),
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn product_title_with_url_gap_and_sustained_mic_offers() {
    // Chromium/Safari can deny URL AX while still exposing a product title. The title is only
    // a Weak vote, so it needs the sustained mic; it restores a real meeting rather than
    // promoting an arbitrary browser tab to an opener.
    let title = "abc-defg-hij - Google Meet";
    assert_eq!(title_hint(title), Some(MeetingHint::Weak));
    let signals = Signals {
        mic_in_use: true,
        ..Default::default()
    };
    let ctx = DetectionCtx {
        is_browser: true,
        page_host: None,
        has_weak_meeting_signal: true,
        window_title: Some(title),
        ..Default::default()
    };
    assert!(matches!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Offer { .. }
    ));
}

#[test]
fn generic_or_media_title_with_url_gap_cannot_offer() {
    let signals = Signals {
        mic_in_use: true,
        ..Default::default()
    };
    let generic = DetectionCtx {
        is_browser: true,
        page_host: None,
        window_title: Some("Meeting notes - Google Docs"),
        ..Default::default()
    };
    assert_eq!(title_hint("Meeting notes - Google Docs"), None);
    assert_eq!(
        evaluate_offer(&signals, &generic, &OfferPolicy::default()),
        Decision::Ignore
    );

    // Suppression wins even if a stale product title had supplied a Weak signal.
    let media = DetectionCtx {
        is_browser: true,
        page_host: None,
        has_weak_meeting_signal: true,
        window_title: Some("Picture-in-picture — Google Meet — YouTube"),
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &media, &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn pip_title_suppresses_even_with_mic() {
    let signals = Signals {
        mic_in_use: mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, false),
        ..Default::default()
    };
    let ctx = DetectionCtx {
        is_browser: true,
        page_host: Some(""),
        window_title: Some("Picture-in-picture"),
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Ignore
    );
    assert!(is_suppressed_title("Picture-in-picture"));
}

#[test]
fn meet_url_opens_without_mic() {
    let signals = Signals {
        meeting_app_frontmost: true,
        ..Default::default()
    };
    let ctx = DetectionCtx {
        is_browser: true,
        page_host: Some("meet.google.com"),
        has_meet_url: true,
        ..Default::default()
    };
    assert!(matches!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Offer { .. }
    ));
}

#[test]
fn zoom_bundle_opens_without_mic() {
    let signals = Signals {
        meeting_app_frontmost: true,
        ..Default::default()
    };
    let ctx = DetectionCtx {
        has_strong_bundle: true,
        ..Default::default()
    };
    assert!(matches!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Offer { .. }
    ));
}

#[test]
fn controls_alone_need_a_second_signal() {
    let signals = Signals {
        meeting_controls_visible: true,
        ..Default::default()
    };
    assert!(matches!(decide(&signals), Decision::Offer { .. }));
    assert_eq!(
        evaluate_offer(&signals, &DetectionCtx::default(), &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn host_from_url_parses_meet() {
    assert_eq!(
        host_from_url("https://meet.google.com/abc-defg-hij").as_deref(),
        Some("meet.google.com")
    );
}

#[test]
fn zoom_is_a_known_meeting_app() {
    assert!(is_meeting_app("us.zoom.xos"));
}

#[test]
fn an_ordinary_app_is_not_a_meeting_app() {
    assert!(!is_meeting_app("com.apple.Safari"));
    assert!(!is_meeting_app(""));
}

#[test]
fn google_meet_urls_are_recognised() {
    assert!(is_meeting_url("https://meet.google.com/abc-defg-hij"));
}

#[test]
fn youtube_is_not_a_meeting_url() {
    assert!(!is_meeting_url(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
    assert!(!is_meeting_url("https://youtu.be/dQw4w9WgXcQ"));
    assert!(!is_meeting_url("https://m.youtube.com/watch?v=abc"));
}

#[test]
fn media_urls_are_recognised() {
    assert!(is_media_url("https://www.youtube.com/watch?v=abc"));
    assert!(is_media_url("https://youtu.be/abc"));
    assert!(is_media_url("https://music.youtube.com/watch?v=abc"));
    assert!(is_media_url("https://www.netflix.com/watch/123"));
    assert!(is_media_url("https://www.twitch.tv/somechannel"));
}

#[test]
fn meet_urls_are_not_media() {
    assert!(!is_media_url("https://meet.google.com/abc-defg-hij"));
}

#[test]
fn mic_on_media_never_counts_without_meeting_context() {
    assert!(!mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, true));
    assert!(!mic_counts_as_signal(
        MIC_ONLY_SUSTAIN_MS + 1_000,
        false,
        true
    ));
}

#[test]
fn mic_on_media_counts_with_meeting_context_after_short_sustain() {
    assert!(!mic_counts_as_signal(MIC_SUSTAIN_MS - 1, true, true));
    assert!(mic_counts_as_signal(MIC_SUSTAIN_MS, true, true));
}

#[test]
fn mic_alone_needs_the_longer_sustain() {
    assert!(!mic_counts_as_signal(MIC_SUSTAIN_MS, false, false));
    assert!(mic_counts_as_signal(MIC_ONLY_SUSTAIN_MS, false, false));
}

#[test]
fn mic_with_meeting_context_uses_the_shorter_sustain() {
    assert!(mic_counts_as_signal(MIC_SUSTAIN_MS, true, false));
}

#[test]
fn a_lookalike_host_is_not_a_meeting_url() {
    // Host matching must not be substring matching, or an attacker-controlled or merely
    // unlucky domain turns the microphone offer on.
    assert!(!is_meeting_url("https://meet.google.com.evil.test/x"));
    assert!(!is_meeting_url("https://notmeet.google.com/x"));
    assert!(!is_meeting_url("https://example.test/?u=meet.google.com"));
}

#[test]
fn meeting_surface_matrix_normalizes_supported_variants() {
    for (bundle, expected) in [
        ("us.zoom.xos", Some(MeetingHint::Strong)),
        (" US.ZOOM.XOS ", Some(MeetingHint::Strong)),
        ("com.microsoft.teams2", Some(MeetingHint::Weak)),
        ("COM.MICROSOFT.TEAMS", Some(MeetingHint::Weak)),
        ("com.microsoft.teams.work", Some(MeetingHint::Weak)),
        ("com.apple.Safari", None),
    ] {
        assert_eq!(bundle_hint(bundle), expected, "bundle {bundle}");
    }

    for (url, expected) in [
        (
            "https://MEET.GOOGLE.COM./abc-defg-hij",
            Some(MeetingHint::Strong),
        ),
        ("https://zoom.us/j/123456789", Some(MeetingHint::Strong)),
        (
            "https://us02web.zoom.us/j/123456789",
            Some(MeetingHint::Strong),
        ),
        (
            "https://app.zoom.us/wc/123456789/join",
            Some(MeetingHint::Strong),
        ),
        (
            "https://teams.microsoft.com/l/meetup-join/19%3ameeting",
            Some(MeetingHint::Weak),
        ),
        (
            "https://teams.cloud.microsoft/meet/abc",
            Some(MeetingHint::Weak),
        ),
        (
            "https://teams.microsoft.us/meet/abc",
            Some(MeetingHint::Weak),
        ),
        ("https://zoom.us/pricing", None),
        ("https://app.zoom.us/account", None),
        ("https://us02web.zoom.us/", None),
        ("https://teams.microsoft.com/_#/conversations/General", None),
        ("https://teams.live.com/calendar", None),
        (
            "https://teams.microsoft.com.evil.test/l/meetup-join/abc",
            None,
        ),
        ("https://zoom.us.evil.test/j/123456789", None),
    ] {
        assert_eq!(host_hint(url), expected, "url {url}");
    }

    for (process, expected) in [
        (" Zoom Workplace ", Some(MeetingHint::Weak)),
        ("MICROSOFT TEAMS", Some(MeetingHint::Weak)),
        ("Meeting Helper", None),
    ] {
        assert_eq!(process_hint(process), expected, "process {process}");
    }

    for (title, expected) in [
        ("Weekly sync | Zoom Meeting", Some(MeetingHint::Weak)),
        ("abc-defg-hij - Google Meet", Some(MeetingHint::Weak)),
        ("Team standup | Microsoft Teams", Some(MeetingHint::Weak)),
        ("Meeting notes - Google Docs", None),
        ("Meet the team - Company wiki", None),
    ] {
        assert_eq!(title_hint(title), expected, "title {title}");
    }
}

// ── Plan A: tiered (Strong/Weak) detection ─────────────────────────────────────────────

#[test]
fn a_weak_bundle_alone_does_not_offer() {
    // A-0/A-2: a resident chat app in front is someone reading messages, not a meeting.
    let signals = Signals::default();
    let ctx = DetectionCtx {
        has_weak_meeting_signal: true,
        window_title: Some("Chat | Microsoft Teams"),
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn a_weak_bundle_with_sustained_mic_offers() {
    // A-0: the Weak vote plus sustained mic is the two-signal case — and it must clear the
    // default policy's mic-only gate, because the mic is not "only" here.
    let signals = Signals {
        mic_in_use: mic_counts_as_signal(MIC_SUSTAIN_MS, true, false),
        ..Default::default()
    };
    let ctx = DetectionCtx {
        has_weak_meeting_signal: true,
        ..Default::default()
    };
    let d = evaluate_offer(&signals, &ctx, &OfferPolicy::default());
    let Decision::Offer { provenance, .. } = d else {
        panic!("expected an offer")
    };
    assert!(provenance.contains("mic_sustained"));
    assert!(
        provenance.contains("weak_meeting_signal"),
        "the Weak vote is evidence too"
    );
    serde_json::from_str::<serde_json::Value>(&provenance).expect("provenance must be JSON");
}

#[test]
fn strong_hosts_still_offer_alone() {
    // The A-0 refactor must not weaken what already worked: a Strong URL frontmost is an
    // opener by itself, exactly as before the tiers existed.
    for (url, host) in [
        ("https://meet.google.com/abc-defg-hij", "meet.google.com"),
        ("https://app.zoom.us/wc/1234567890/join", "app.zoom.us"),
    ] {
        assert_eq!(host_hint(url), Some(MeetingHint::Strong), "{url}");
        let signals = Signals {
            meeting_app_frontmost: true,
            ..Default::default()
        };
        let ctx = DetectionCtx {
            is_browser: true,
            page_host: Some(host),
            has_meet_url: true,
            ..Default::default()
        };
        assert!(
            matches!(
                evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
                Decision::Offer { .. }
            ),
            "{url} should offer alone"
        );
    }
}

#[test]
fn zoom_web_client_host_offers() {
    // A-1: the Zoom Web client lives on app.zoom.us and is Strong like the native app.
    assert_eq!(
        host_hint("https://app.zoom.us/wc/1234567890/start"),
        Some(MeetingHint::Strong)
    );
    assert!(is_meeting_url("https://app.zoom.us/wc/1234567890/start"));
}

#[test]
fn zoom_marketing_site_does_not() {
    // A-1: zoom.us is the marketing site, not the meeting — and no lookalike or subdomain
    // trick may promote itself to app.zoom.us.
    assert_eq!(host_hint("https://zoom.us/pricing"), None);
    assert_eq!(host_hint("https://www.zoom.us/"), None);
    assert_eq!(host_hint("https://app.zoom.us.evil.test/wc"), None);
    assert_eq!(host_hint("https://example.test/?u=app.zoom.us"), None);
}

#[test]
fn teams_frontmost_alone_never_offers() {
    // A-2: Teams is where chat lives all day. Frontmost — even with a calendar occurrence
    // agreeing — is not an observation of attendance and must never open the offer.
    for bundle in ["com.microsoft.teams2", "com.microsoft.teams"] {
        assert_eq!(bundle_hint(bundle), Some(MeetingHint::Weak), "{bundle}");
    }
    let signals = Signals {
        occurrence_now: true,
        ..Default::default()
    };
    let ctx = DetectionCtx {
        has_weak_meeting_signal: true,
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn teams_with_mic_sustained_offers() {
    // A-2: the same Teams window plus a sustained mic is a call.
    let signals = Signals {
        mic_in_use: mic_counts_as_signal(MIC_SUSTAIN_MS, true, false),
        ..Default::default()
    };
    let ctx = DetectionCtx {
        has_weak_meeting_signal: true,
        window_title: Some("Weekly sync | Microsoft Teams"),
        ..Default::default()
    };
    assert!(matches!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Offer { .. }
    ));
}

#[test]
fn webex_subdomain_matches() {
    // A-2: every Webex tenant is its own subdomain; the suffix match is exact-label, so a
    // lookalike registering evilwebex.com gains nothing.
    assert_eq!(
        host_hint("https://acme.webex.com/meet/team"),
        Some(MeetingHint::Weak)
    );
    assert_eq!(host_hint("https://webex.com/"), Some(MeetingHint::Weak));
    assert_eq!(host_hint("https://evilwebex.com/"), None);
    assert_eq!(host_hint("https://acme.webex.com.evil.test/"), None);
    assert_eq!(bundle_hint("Cisco-Systems.Spark"), Some(MeetingHint::Weak));
}

#[test]
fn teams_meeting_ends_on_silence_not_on_app_presence() {
    // A-2: Teams keeps running after everyone hangs up, so AppGone cannot be the end for a
    // Weak-bundle meeting — the silence limit is, and the app's continued presence must not
    // postpone it.
    let now = 1_000_000;
    let call_over_app_still_running = LiveSignals {
        meeting_app_present: true,
        occurrence_ends_at: None,
        last_sound_at: now - SILENCE_LIMIT_MS - 1,
    };
    assert_eq!(
        end_condition(&call_over_app_still_running, now),
        Some(EndReason::Silence)
    );

    let still_talking = LiveSignals {
        meeting_app_present: true,
        occurrence_ends_at: None,
        last_sound_at: now,
    };
    assert_eq!(end_condition(&still_talking, now), None);
}

#[test]
fn weak_bundle_quit_does_not_read_as_leaving_the_call() {
    // A-2, the call_clearly_ended side of the same rule: only a Strong bundle's disappearance
    // means "left". A quit Teams with the mic still open is a call routed elsewhere.
    assert!(!call_clearly_ended(
        false,
        None,
        1_000_000,
        true, // mic still open
        None,
        Some("com.microsoft.teams2"),
        false, // app no longer running
    ));
    // Zoom (Strong) quitting still ends it, exactly as before the tiers.
    assert!(call_clearly_ended(
        false,
        None,
        1_000_000,
        true,
        None,
        Some("us.zoom.xos"),
        false
    ));
}

// ── Plan A-4: Slack huddles ────────────────────────────────────────────────────────────

#[test]
fn huddle_title_plus_mic_offers() {
    // The hint needs the huddle word AND call-control vocabulary, then mic-sustained is the
    // second vote. English and Japanese UIs both count.
    assert!(huddle_hint(
        Some("Huddle • #design"),
        &["Mute", "Leave huddle"]
    ));
    assert!(huddle_hint(Some("ハドル • #design"), &["ミュート", "退出"]));

    let signals = Signals {
        mic_in_use: mic_counts_as_signal(MIC_SUSTAIN_MS, true, false),
        ..Default::default()
    };
    let ctx = DetectionCtx {
        has_huddle_hint: true,
        window_title: Some("Huddle • #design"),
        ..Default::default()
    };
    let d = evaluate_offer(&signals, &ctx, &OfferPolicy::default());
    let Decision::Offer { provenance, .. } = d else {
        panic!("expected an offer")
    };
    assert!(
        provenance.contains("huddle_hint"),
        "the hint is evidence and must be recorded"
    );
}

#[test]
fn huddle_title_alone_does_nothing() {
    // One vote is not a meeting: without the mic the hint must produce no offer at all,
    // however clearly the window says "Huddle".
    assert!(huddle_hint(Some("Huddle • #design"), &["Mute"]));
    let signals = Signals::default();
    let ctx = DetectionCtx {
        has_huddle_hint: true,
        window_title: Some("Huddle • #design"),
        ..Default::default()
    };
    assert_eq!(
        evaluate_offer(&signals, &ctx, &OfferPolicy::default()),
        Decision::Ignore
    );
}

#[test]
fn slack_chat_mentioning_huddle_does_not_trigger() {
    // "huddle" in message text is conversation, not a call — the control vocabulary is what
    // separates the huddle UI from chat about huddles.
    assert!(!huddle_hint(
        Some("#general – Slack"),
        &[
            "did you catch the huddle earlier?",
            "yes — notes are in the doc"
        ],
    ));
    assert!(!huddle_hint(None, &["let's huddle tomorrow morning"]));
    assert!(!huddle_hint(
        Some("#general – Slack"),
        &["昨日のハドルどうだった？"]
    ));
    // And control words with no huddle word at all (an ordinary call app's UI) do nothing.
    assert!(!huddle_hint(Some("#general – Slack"), &["Mute", "Leave"]));
}

#[test]
fn huddle_hint_lost_needs_grace_before_wrap() {
    // Mirrors the Meet-tab leave grace: a redraw or a glance at another channel must not
    // wrap the huddle mid-sentence.
    let lost = 1_000_000;
    assert!(!huddle_hint_lost_past_grace(
        Some(lost),
        lost + HUDDLE_HINT_LOST_GRACE_MS - 1
    ));
    assert!(huddle_hint_lost_past_grace(
        Some(lost),
        lost + HUDDLE_HINT_LOST_GRACE_MS
    ));
}

#[test]
fn huddle_hint_never_lost_has_no_grace_deadline() {
    assert!(!huddle_hint_lost_past_grace(None, 9_999_999));
}

#[test]
fn huddle_past_grace_stays_present_while_mic_open() {
    // Alt-tabbing away from Slack hides the huddle UI, but the open mic proves the call is
    // still running — exactly the Meet-tab rule.
    let lost = 1_000_000;
    let after = lost + HUDDLE_HINT_LOST_GRACE_MS;
    assert!(huddle_session_present(Some(lost), after, true, None));
}

#[test]
fn huddle_past_grace_ends_after_mic_quiet() {
    let lost = 1_000_000;
    let after_grace = lost + HUDDLE_HINT_LOST_GRACE_MS;
    let closed = after_grace;
    assert!(huddle_session_present(
        Some(lost),
        closed,
        false,
        Some(closed)
    ));
    let quiet = closed + MIC_QUIET_AFTER_URL_LEFT_MS;
    assert!(!huddle_session_present(
        Some(lost),
        quiet,
        false,
        Some(closed)
    ));
}

#[test]
fn huddle_within_grace_is_present_regardless_of_mic() {
    // A redraw or a glance at another channel must not wrap the huddle mid-sentence.
    let lost = 1_000_000;
    let within = lost + HUDDLE_HINT_LOST_GRACE_MS - 1;
    assert!(huddle_session_present(
        Some(lost),
        within,
        false,
        Some(lost)
    ));
}

#[test]
fn huddle_hint_never_lost_is_always_present() {
    assert!(huddle_session_present(None, 9_999_999, false, Some(0)));
}

#[test]
fn meet_url_in_browser_counts_as_present() {
    assert!(browser_meeting_page_present(
        Some("https://meet.google.com/abc-defg-hij"),
        Some("Meet – weekly – Google Chrome"),
    ));
}

#[test]
fn gmail_tab_means_meeting_page_left() {
    assert!(!browser_meeting_page_present(
        Some("https://mail.google.com/mail/u/0/"),
        Some("Inbox - Gmail"),
    ));
}

#[test]
fn pip_with_unreadable_url_still_counts_as_present() {
    assert!(browser_meeting_page_present(
        None,
        Some("Picture-in-picture"),
    ));
}

#[test]
fn meeting_url_left_needs_grace_before_wrap() {
    let lost = 1_000_000;
    assert!(!meeting_url_left_past_grace(
        Some(lost),
        lost + MEETING_URL_LEFT_GRACE_MS - 1
    ));
    assert!(meeting_url_left_past_grace(
        Some(lost),
        lost + MEETING_URL_LEFT_GRACE_MS
    ));
}

#[test]
fn meeting_url_never_lost_has_no_grace_deadline() {
    assert!(!meeting_url_left_past_grace(None, 9_999_999));
}

#[test]
fn meet_url_past_grace_stays_present_while_mic_open() {
    let lost = 1_000_000;
    let after = lost + MEETING_URL_LEFT_GRACE_MS;
    assert!(meet_url_session_present(Some(lost), after, true, None));
}

#[test]
fn meet_url_past_grace_ends_after_mic_quiet() {
    let lost = 1_000_000;
    let after_grace = lost + MEETING_URL_LEFT_GRACE_MS;
    let closed = after_grace;
    assert!(meet_url_session_present(
        Some(lost),
        closed,
        false,
        Some(closed)
    ));
    let quiet = closed + MIC_QUIET_AFTER_URL_LEFT_MS;
    assert!(!meet_url_session_present(
        Some(lost),
        quiet,
        false,
        Some(closed)
    ));
}

#[test]
fn tab_switch_with_mic_open_keeps_session() {
    let lost = 1_000_000;
    let later = lost + MEETING_URL_LEFT_GRACE_MS + 60_000;
    assert!(meet_url_session_present(Some(lost), later, true, None));
}

/// A coarse (system-wide) observation while `app` is in front.
fn coarse<'a>(in_use: bool, app: &'a str, meeting_context: bool) -> MicObservation<'a> {
    MicObservation {
        in_use,
        source: MicSource::SystemWide,
        frontmost_bundle_id: app,
        meeting_context,
    }
}

/// The pre-existing shape of these tests: coarse signal, meeting app in front.
fn in_meeting(in_use: bool) -> MicObservation<'static> {
    coarse(in_use, "us.zoom.xos", true)
}

#[test]
fn a_brief_burst_of_microphone_use_is_not_a_meeting() {
    // Dictation, a voice memo, "hey" into a chat app. Offering to take notes on those is how
    // the panel becomes something the user learns to dismiss without reading.
    let mut w = MicWatch::new();
    assert!(!w.observe(&in_meeting(true), 0));
    assert!(!w.observe(&in_meeting(true), 5_000));
    assert!(!w.observe(&in_meeting(false), 6_000));
    assert!(
        !w.observe(&in_meeting(true), 7_000),
        "the clock restarts when the mic closes"
    );
}

#[test]
fn sustained_microphone_use_is_a_meeting() {
    let mut w = MicWatch::new();
    w.observe(&in_meeting(true), 0);
    assert!(!w.observe(&in_meeting(true), MIC_SUSTAIN_MS - 1));
    assert!(w.observe(&in_meeting(true), MIC_SUSTAIN_MS));
}

#[test]
fn the_signal_stays_true_while_the_call_continues() {
    // It has to keep answering yes: the detector asks once a second, and a meeting that
    // "became true and then went quiet" would close the interval mid-call.
    let mut w = MicWatch::new();
    w.observe(&in_meeting(true), 0);
    for t in 0..30 {
        let now = MIC_SUSTAIN_MS + t * 1_000;
        assert!(
            w.observe(&in_meeting(true), now),
            "second {t} of the call reported no meeting"
        );
    }
}

#[test]
fn hanging_up_and_calling_again_needs_the_full_sustain_again() {
    let mut w = MicWatch::new();
    w.observe(&in_meeting(true), 0);
    assert!(w.observe(&in_meeting(true), MIC_SUSTAIN_MS));

    w.observe(&in_meeting(false), MIC_SUSTAIN_MS + 1_000);

    assert!(
        !w.observe(&in_meeting(true), MIC_SUSTAIN_MS + 2_000),
        "a new call starts its own clock"
    );
}

// ---- the stuck coarse signal (observed on-device 2026-07-31) ------------------------------

#[test]
fn an_always_on_holder_stops_reporting_a_meeting_in_every_app() {
    // The bug this check exists for: a voice utility held an input device from login, so the
    // system-wide flag was true in Finder, in the login window and everywhere else — and the
    // watch answered "meeting" for all of them.
    let mut w = MicWatch::new();
    let apps = [
        "com.apple.finder",
        "com.google.Chrome",
        "com.tinyspeck.slackmacgap",
    ];

    // Before the tally can condemn it the signal still reports: it has no reason yet.
    w.observe(&coarse(true, apps[0], false), 0);
    assert!(w.observe(&coarse(true, apps[0], false), MIC_SUSTAIN_MS));

    // The user moves through unrelated apps; past the floor, the signal is written off.
    let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
    w.observe(&coarse(true, apps[1], false), t);
    let last = w.observe(&coarse(true, apps[2], false), t + 1_000);

    assert!(
        w.is_stuck(),
        "three unrelated apps past the floor is a stuck device"
    );
    assert!(!last, "a stuck signal must not report a meeting");
    assert!(!w.observe(&coarse(true, "com.apple.loginwindow", false), t + 2_000));
}

#[test]
fn multitasking_early_in_a_call_does_not_condemn_the_signal() {
    // Joining a call and immediately opening the agenda, the calendar and a scratchpad is
    // exactly the shape of the stuck pattern — the time floor is what tells them apart.
    let mut w = MicWatch::new();
    w.observe(&coarse(true, "com.hnc.Discord", false), 0);
    w.observe(&coarse(true, "com.apple.Calendar", false), 5_000);
    let reporting = w.observe(&coarse(true, "com.apple.Notes", false), MIC_SUSTAIN_MS + 1);

    assert!(
        !w.is_stuck(),
        "three apps inside the floor is a busy call, not a stuck device"
    );
    assert!(reporting, "a real call must keep its opener");
}

#[test]
fn a_meeting_coming_into_view_clears_the_verdict() {
    // "Stuck" means no explanation was ever offered. A meeting in front is an explanation.
    let mut w = MicWatch::new();
    let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
    w.observe(&coarse(true, "com.apple.finder", false), 0);
    w.observe(&coarse(true, "com.google.Chrome", false), t);
    w.observe(&coarse(true, "com.tinyspeck.slackmacgap", false), t + 1_000);
    assert!(w.is_stuck());

    let back = w.observe(&coarse(true, "us.zoom.xos", true), t + 2_000);
    assert!(!w.is_stuck(), "an explained device is not a stuck one");
    assert!(
        back,
        "the opener returns for a meeting that is actually in front"
    );
}

#[test]
fn releasing_the_device_clears_the_verdict() {
    let mut w = MicWatch::new();
    let t = MIC_STUCK_MIN_MS + MIC_SUSTAIN_MS;
    w.observe(&coarse(true, "a", false), 0);
    w.observe(&coarse(true, "b", false), t);
    w.observe(&coarse(true, "c", false), t + 1_000);
    assert!(w.is_stuck());

    w.observe(&coarse(false, "a", false), t + 2_000);
    assert!(
        !w.is_stuck(),
        "a device that can be released was never permanently stuck"
    );
}

// ---- attributed use (macOS 14.4+) --------------------------------------------------------

fn held_by<'a>(holder: &'a str, front: &'a str) -> MicObservation<'a> {
    MicObservation {
        in_use: true,
        source: MicSource::Holder { bundle_id: holder },
        frontmost_bundle_id: front,
        meeting_context: false,
    }
}

#[test]
fn a_background_daemon_holding_the_mic_is_not_a_meeting() {
    let mut w = MicWatch::new();
    w.observe(&held_by("com.voiceos.app", "com.apple.finder"), 0);
    assert!(!w.observe(
        &held_by("com.voiceos.app", "com.apple.finder"),
        MIC_SUSTAIN_MS * 10
    ));
}

#[test]
fn a_meeting_app_holding_the_mic_counts_even_from_the_background() {
    // The call is in Zoom while the user takes notes in another app. Requiring the holder to
    // be frontmost would drop the signal exactly when the user is doing the thing SHOGUN is
    // for.
    let mut w = MicWatch::new();
    w.observe(&held_by("us.zoom.xos", "com.apple.Notes"), 0);
    assert!(w.observe(&held_by("us.zoom.xos", "com.apple.Notes"), MIC_SUSTAIN_MS));
}

#[test]
fn the_app_in_front_holding_the_mic_counts_even_if_unlisted() {
    // A huddle in an app nobody put in the bundle table is still a call.
    let mut w = MicWatch::new();
    w.observe(&held_by("com.hnc.Discord", "com.hnc.Discord"), 0);
    assert!(w.observe(
        &held_by("com.hnc.Discord", "com.hnc.Discord"),
        MIC_SUSTAIN_MS
    ));
}

#[test]
fn our_own_capture_never_counts_as_a_new_meeting() {
    // SHOGUN's ASR holds the input during a meeting it is already noting. Reading that back
    // as evidence would let the app detect itself.
    let mut w = MicWatch::new();
    w.observe(&held_by(SELF_BUNDLE_ID, SELF_BUNDLE_ID), 0);
    assert!(!w.observe(
        &held_by(SELF_BUNDLE_ID, SELF_BUNDLE_ID),
        MIC_SUSTAIN_MS * 10
    ));
}
