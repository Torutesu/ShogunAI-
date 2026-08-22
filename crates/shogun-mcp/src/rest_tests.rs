use super::*;
use crate::memory_api::{tool_level, ApiLevel, TokenRegistry};
use shogun_agents::approval::{ApprovalOrigin, ApprovalQueue};
use shogun_agents::entitlement::{entitlements, Entitlements, Plan, TRIAL_DURATION_MS};

/// An entitled plan (active trial) for the routing tests.
fn ent() -> Entitlements {
    Entitlements::trial_not_started()
}

fn reg() -> TokenRegistry {
    let mut r = TokenRegistry::new();
    r.issue("t");
    r
}
fn req(method: Method, path: &str, token: Option<&str>) -> RestRequest {
    RestRequest {
        method,
        path: path.into(),
        token: token.map(str::to_string),
        include_low: false,
        query: None,
        body: None,
        from_ms: None,
        to_ms: None,
        for_generation: false,
        app_bundle_id: None,
        person_id: None,
        project_id: None,
    }
}

#[test]
fn bearer_parsing() {
    assert_eq!(bearer(Some("Bearer abc123")), Some("abc123".into()));
    assert_eq!(bearer(Some("Basic abc")), None);
    assert_eq!(bearer(None), None);
}

#[test]
fn unknown_path_is_404_even_with_a_token() {
    assert_eq!(
        route(&req(Method::Get, "/v1/nope", Some("t")), &reg(), &ent()),
        Routed::NotFound
    );
}

#[test]
fn wrong_method_is_405() {
    // search is GET-only
    assert_eq!(
        route(
            &req(Method::Post, "/v1/memory/search", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::MethodNotAllowed
    );
}

#[test]
fn tool_endpoints_require_a_token_including_reads() {
    assert_eq!(
        route(&req(Method::Get, "/v1/memory/search", None), &reg(), &ent()),
        Routed::Unauthorized
    );
    assert_eq!(
        route(
            &req(Method::Get, "/v1/state/people", Some("wrong")),
            &reg(),
            &ent()
        ),
        Routed::Unauthorized
    );
}

#[test]
fn locked_plan_is_403_on_every_tool_endpoint_but_status_stays_open() {
    // Issue #97: Standard / expired trial → the Memory API is refused with a valid token,
    // reads included; the unauthenticated health endpoints keep answering.
    let expired = entitlements(
        Plan::Trial {
            started_at_ms: Some(0),
        },
        TRIAL_DURATION_MS,
    );
    for locked in [entitlements(Plan::Standard, 0), expired] {
        for (method, path) in [
            (Method::Get, "/v1/memory/search"),
            (Method::Get, "/v1/state/people"),
            (Method::Post, "/v1/memory/notes"),
            (Method::Post, "/v1/actions/execute"),
        ] {
            let routed = route(&req(method, path, Some("t")), &reg(), &locked);
            assert_eq!(routed, Routed::PlanLocked, "{path} must be plan-locked");
            assert_eq!(status_code(&routed), 403);
        }
        assert_eq!(
            route(&req(Method::Get, "/v1/status", None), &reg(), &locked),
            Routed::Status
        );
        assert_eq!(
            route(&req(Method::Get, "/v1/metrics", None), &reg(), &locked),
            Routed::Metrics
        );
        // no token still reads as 401, not 403 (auth first — the plan is not disclosed)
        assert_eq!(
            route(
                &req(Method::Get, "/v1/memory/search", None),
                &reg(),
                &locked
            ),
            Routed::Unauthorized
        );
    }
    // Pro passes
    let pro = entitlements(Plan::Pro, 0);
    assert!(matches!(
        route(
            &req(Method::Get, "/v1/memory/search", Some("t")),
            &reg(),
            &pro
        ),
        Routed::Read { .. }
    ));
}

#[test]
fn status_is_unauthenticated() {
    assert_eq!(
        route(&req(Method::Get, "/v1/status", None), &reg(), &ent()),
        Routed::Status
    );
    assert_eq!(status_code(&Routed::Status), 200);
}

#[test]
fn metrics_is_unauthenticated_and_get_only() {
    // health endpoint: open like status (NFR-SLO-00), no capture content, localhost-bound.
    assert_eq!(
        route(&req(Method::Get, "/v1/metrics", None), &reg(), &ent()),
        Routed::Metrics
    );
    assert_eq!(status_code(&Routed::Metrics), 200);
    // still GET-only
    assert_eq!(
        route(&req(Method::Post, "/v1/metrics", Some("t")), &reg(), &ent()),
        Routed::MethodNotAllowed
    );
}

#[test]
fn read_endpoints_resolve_to_read_tools() {
    assert_eq!(
        route(
            &req(Method::Get, "/v1/memory/search", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::MemorySearch,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Get, "/v1/state/commitments", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::StateCommitmentsList,
            id: None
        }
    );
    // trailing id selects the get variant
    assert_eq!(
        route(
            &req(Method::Get, "/v1/state/people/42", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::StatePeopleGet,
            id: Some(42)
        }
    );
}

#[test]
fn write_endpoints_carry_their_levels_and_202() {
    let note = route(
        &req(Method::Post, "/v1/memory/notes", Some("t")),
        &reg(),
        &ent(),
    );
    assert_eq!(
        note,
        Routed::Write {
            tool: Tool::MemoryAppendNote,
            level: Level::L1
        }
    );
    assert_eq!(status_code(&note), 202);

    let propose = route(
        &req(Method::Post, "/v1/state/proposals", Some("t")),
        &reg(),
        &ent(),
    );
    assert_eq!(
        propose,
        Routed::Write {
            tool: Tool::StateProposeUpdate,
            level: Level::L2
        }
    );
    assert_eq!(
        route(&req(Method::Post, "/v1/profile", Some("t")), &reg(), &ent()),
        Routed::Write {
            tool: Tool::ProfileSet,
            level: Level::L1
        }
    );
}

#[test]
fn voice_dictionary_routes_are_token_and_plan_gated_like_other_local_writes() {
    assert_eq!(
        route(
            &req(Method::Get, "/v1/voice_dictionary/terms", Some("t")),
            &reg(),
            &ent(),
        ),
        Routed::Read {
            tool: Tool::VoiceDictionaryList,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/voice_dictionary/terms/7", Some("t")),
            &reg(),
            &ent(),
        ),
        Routed::Write {
            tool: Tool::VoiceDictionaryUpdate,
            level: Level::L1
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/voice_dictionary/terms/7/delete", None),
            &reg(),
            &ent(),
        ),
        Routed::Unauthorized
    );
}

#[test]
fn whoami_endpoint_is_a_token_protected_structured_read() {
    assert_eq!(
        route(
            &req(Method::Get, "/v1/profile/whoami", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::ProfileWhoami,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Get, "/v1/profile/whoami", None),
            &reg(),
            &ent()
        ),
        Routed::Unauthorized
    );
}

#[test]
fn actions_execute_routes_to_action() {
    assert_eq!(
        route(
            &req(Method::Post, "/v1/actions/execute", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Action
    );
    assert_eq!(status_code(&Routed::Action), 202);
}

#[test]
fn trailing_slash_is_tolerated() {
    assert_eq!(
        route(
            &req(Method::Get, "/v1/state/projects/", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::StateProjectsList,
            id: None
        }
    );
}

#[test]
fn respond_renders_status_and_json_body() {
    let tokens = reg();
    // unauthenticated status
    let (s, b) = respond(&req(Method::Get, "/v1/status", None), &tokens, &ent());
    assert_eq!(s, 200);
    assert!(b.contains("shogun-memory-api"));
    // authed read → 200 with tool + empty results
    let (s, b) = respond(
        &req(Method::Get, "/v1/memory/search", Some("t")),
        &tokens,
        &ent(),
    );
    assert_eq!(s, 200);
    assert!(b.contains("\"tool\":\"memory.search\""));
    assert!(b.contains("\"results\":[]"));
    // missing token → 401
    let (s, b) = respond(
        &req(Method::Get, "/v1/memory/search", None),
        &tokens,
        &ent(),
    );
    assert_eq!(s, 401);
    assert!(b.contains("unauthorized"));
    // write → 202 with level
    let (s, b) = respond(
        &req(Method::Post, "/v1/memory/notes", Some("t")),
        &tokens,
        &ent(),
    );
    assert_eq!(s, 202);
    assert!(b.contains("\"level\":\"L1\""));
}

#[test]
fn respond_with_backend_returns_data_confidence_filtered() {
    use crate::backend::{MemoryBackend, ReadItem};

    struct Fake;
    impl MemoryBackend for Fake {
        fn read(&self, _tool: Tool, _params: &crate::backend::ReadParams) -> Vec<ReadItem> {
            vec![
                ReadItem::new("high", 0.9),   // included, not possibly
                ReadItem::new("medium", 0.6), // included, possibly
                ReadItem::new("low", 0.3),    // excluded by default
            ]
        }
    }
    let tokens = reg();

    // default: low excluded, medium flagged possibly
    let (s, b) = respond_with(
        &req(Method::Get, "/v1/state/people", Some("t")),
        &tokens,
        &ent(),
        &Fake,
    );
    assert_eq!(s, 200);
    assert!(b.contains("\"text\":\"high\""));
    assert!(b.contains(r#""text":"medium","confidence":0.6,"possibly":true"#));
    assert!(!b.contains("\"low\""), "low confidence excluded by default");

    // include_low pulls the low one in
    let with_low = RestRequest {
        include_low: true,
        ..req(Method::Get, "/v1/state/people", Some("t"))
    };
    let (_, b2) = respond_with(&with_low, &tokens, &ent(), &Fake);
    assert!(b2.contains("\"text\":\"low\""));
}

#[test]
fn respond_with_still_enforces_auth_and_404() {
    use crate::backend::StubBackend;
    let tokens = reg();
    let (s, _) = respond_with(
        &req(Method::Get, "/v1/state/people", None),
        &tokens,
        &ent(),
        &StubBackend,
    );
    assert_eq!(s, 401, "no token still 401 even with a backend");
    let (s, _) = respond_with(
        &req(Method::Get, "/v1/nope", Some("t")),
        &tokens,
        &ent(),
        &StubBackend,
    );
    assert_eq!(s, 404);
}

#[test]
fn act_local_is_authorized_immediately() {
    let mut q = ApprovalQueue::new();
    let (s, b) = act(
        Some(r#"{"kind":"local_search","query":"budget"}"#),
        0,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Present,
    );
    assert_eq!(s, 200);
    assert!(b.contains("\"executed\":\"local\""));
    assert!(b.contains("\"level\":\"L1\""));
    assert_eq!(
        q.pending_len(),
        0,
        "a local action never enqueues an approval"
    );
}

#[test]
fn act_send_enqueues_pending_l3_approval() {
    let mut q = ApprovalQueue::new();
    let (s, b) = act(
        Some(r#"{"kind":"send_email","to":"a@b.com","subject":"Hi","body":"hello"}"#),
        1000,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Present,
    );
    assert_eq!(s, 202);
    assert!(b.contains("\"pending\":true"));
    assert!(b.contains("\"approval_id\":"));
    assert!(b.contains("\"level\":\"L3\""));
    assert_eq!(
        q.pending_len(),
        1,
        "the send awaits UI confirmation (FR-API-04)"
    );
}

#[test]
fn act_rejects_missing_and_malformed_bodies() {
    let mut q = ApprovalQueue::new();
    assert_eq!(
        act(
            None,
            0,
            &mut q,
            ApprovalOrigin::Api,
            ApprovalSurface::Present
        )
        .0,
        400
    );
    assert_eq!(
        act(
            Some("not json"),
            0,
            &mut q,
            ApprovalOrigin::Api,
            ApprovalSurface::Present
        )
        .0,
        400
    );
    assert_eq!(
        act(
            Some(r#"{"kind":"unknown_thing"}"#),
            0,
            &mut q,
            ApprovalOrigin::Api,
            ApprovalSurface::Present
        )
        .0,
        400
    );
    // a send kind missing a required field is also rejected
    assert_eq!(
        act(
            Some(r#"{"kind":"send_email"}"#),
            0,
            &mut q,
            ApprovalOrigin::Api,
            ApprovalSurface::Present
        )
        .0,
        400
    );
}

#[test]
fn act_refuses_rows_the_persisted_store_would_reject_on_load() {
    // Empty destination: the load-time row check (`action_from_wire`) rejects it, so an
    // enqueue that accepted it would brick the persisted store for every face.
    let mut q = ApprovalQueue::new();
    let (s, b) = act(
        Some(r#"{"kind":"post_message","channel":"","body":"x"}"#),
        0,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Present,
    );
    assert_eq!(s, 400);
    assert!(b.contains("bad_action_request"));
    assert_eq!(q.pending_len(), 0);

    // Oversized body (> 256 KiB) is refused for the same reason.
    let big = "x".repeat(256 * 1024 + 1);
    let (s, _) = act(
        Some(&format!(
            r#"{{"kind":"post_message","channel":"c","body":"{big}"}}"#
        )),
        0,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Present,
    );
    assert_eq!(s, 400);
    assert_eq!(q.pending_len(), 0);
}

#[test]
fn act_refuses_enqueue_beyond_max_pending() {
    let mut q = ApprovalQueue::new();
    let body = r#"{"kind":"post_message","channel":"c","body":"x"}"#;
    for _ in 0..64 {
        let (s, _) = act(
            Some(body),
            0,
            &mut q,
            ApprovalOrigin::Api,
            ApprovalSurface::Present,
        );
        assert_eq!(s, 202);
    }
    let (s, b) = act(
        Some(body),
        0,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Present,
    );
    assert_eq!(s, 429);
    assert!(b.contains("approval_queue_full"));
    assert_eq!(q.pending_len(), 64, "the 65th enqueue must not be stored");
}

#[test]
fn a_send_is_refused_outright_when_nothing_can_confirm_it() {
    // Headless (`shogun-api` / `shogun-mcp` standalone): enqueuing here would look like it
    // worked and then expire in silence. Say so instead — invariant 4 was never at risk,
    // invariant 6's "the API face behaves like the human one" is what this protects.
    let mut q = ApprovalQueue::new();
    let (status, body) = act(
        Some(r#"{"kind":"send_email","to":"a@b.com","subject":"Hi","body":"hello"}"#),
        1000,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Absent,
    );
    assert_eq!(status, 501);
    assert!(body.contains("no_approval_surface"), "{body}");
    assert_eq!(q.pending_len(), 0, "nothing may be stranded in the queue");

    // A local action still runs — only external sends need a confirm surface.
    let (ok, _) = act(
        Some(r#"{"kind":"local_search","query":"x"}"#),
        1000,
        &mut q,
        ApprovalOrigin::Api,
        ApprovalSurface::Absent,
    );
    assert_eq!(ok, 200);
    assert_eq!(q.pending_len(), 0);
}

#[test]
fn json_escape_handles_quotes_and_controls() {
    assert_eq!(json_escape(r#"a"b\c"#), "a\\\"b\\\\c");
    assert_eq!(json_escape("line\nbreak"), "line\\nbreak");
}

#[test]
fn visual_recall_endpoints_resolve() {
    assert_eq!(
        route(
            &req(Method::Get, "/v1/visual_recall/status", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::VisualRecallStatus,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/visual_recall/enabled", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Write {
            tool: Tool::VisualRecallSetEnabled,
            level: Level::L1
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/visual_recall/retention", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Write {
            tool: Tool::VisualRecallSetRetention,
            level: Level::L1
        }
    );
    assert_eq!(
        route(
            &req(Method::Get, "/v1/visual_recall/frames/search", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::VisualRecallSearchFrames,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Get, "/v1/visual_recall/frames/12", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::VisualRecallGetFrame,
            id: Some(12)
        }
    );
    assert_eq!(
        route(
            &req(
                Method::Post,
                "/v1/visual_recall/frames/12/rescan",
                Some("t")
            ),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::VisualRecallRescanFrame,
            id: Some(12)
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/visual_recall/frames/delete", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Write {
            tool: Tool::VisualRecallDeleteFrame,
            level: Level::L1
        }
    );
}

#[test]
fn the_wrap_endpoint_resolves_as_a_read() {
    // Issue #10 (invariant 6): the Evening Wrap is a plain authorized GET, like every read.
    assert_eq!(
        route(
            &req(Method::Get, "/v1/memory/wrap", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Read {
            tool: Tool::MemoryGetWrap,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/memory/wrap", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::MethodNotAllowed
    );
    assert_eq!(
        route(&req(Method::Get, "/v1/memory/wrap", None), &reg(), &ent()),
        Routed::Unauthorized
    );
}

#[test]
fn lessons_endpoints_resolve_at_the_learned_ui_levels() {
    // GET /v1/lessons → the list read; POST /v1/lessons/active → the L1 toggle (invariant 6).
    assert_eq!(
        route(&req(Method::Get, "/v1/lessons", Some("t")), &reg(), &ent()),
        Routed::Read {
            tool: Tool::LessonsList,
            id: None
        }
    );
    assert_eq!(
        route(
            &req(Method::Post, "/v1/lessons/active", Some("t")),
            &reg(),
            &ent()
        ),
        Routed::Write {
            tool: Tool::LessonsSetActive,
            level: Level::L1
        }
    );
    // wrong methods are 405, auth still applies, and the plan gate holds
    assert_eq!(
        route(&req(Method::Post, "/v1/lessons", Some("t")), &reg(), &ent()),
        Routed::MethodNotAllowed
    );
    assert_eq!(
        route(&req(Method::Get, "/v1/lessons", None), &reg(), &ent()),
        Routed::Unauthorized
    );
    let locked = entitlements(Plan::Standard, 0);
    assert_eq!(
        route(&req(Method::Get, "/v1/lessons", Some("t")), &reg(), &locked),
        Routed::PlanLocked
    );
}

#[test]
fn resolved_read_tool_is_actually_a_read() {
    // guard against a routing table that points a read path at a write tool
    if let Routed::Read { tool, .. } = route(
        &req(Method::Get, "/v1/state/open_loops", Some("t")),
        &reg(),
        &ent(),
    ) {
        assert_eq!(tool_level(tool), ApiLevel::Read);
    } else {
        panic!("expected a read");
    }
}
