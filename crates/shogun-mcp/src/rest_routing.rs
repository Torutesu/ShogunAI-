use shogun_agents::permission::Level;

use crate::memory_api::Tool;

use super::{Method, Routed};

pub(super) fn resolve(method: Method, path: &str) -> Result<Routed, RouteMiss> {
    // trim a trailing slash, then split.
    let path = path.strip_suffix('/').unwrap_or(path);
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    // A state noun endpoint: /v1/state/<noun>[/<id>]
    let state_tool = |noun: &str, has_id: bool| -> Option<Tool> {
        Some(match (noun, has_id) {
            ("people", false) => Tool::StatePeopleList,
            ("people", true) => Tool::StatePeopleGet,
            ("projects", false) => Tool::StateProjectsList,
            ("projects", true) => Tool::StateProjectsGet,
            ("commitments", false) => Tool::StateCommitmentsList,
            ("commitments", true) => Tool::StateCommitmentsGet,
            ("open_loops", false) => Tool::StateOpenLoopsList,
            ("open_loops", true) => Tool::StateOpenLoopsGet,
            _ => return None,
        })
    };

    match segs.as_slice() {
        ["v1", "status"] => method_is(method, Method::Get, Routed::Status),
        ["v1", "metrics"] => method_is(method, Method::Get, Routed::Metrics),
        ["v1", "memory", "search"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::MemorySearch,
                id: None,
            },
        ),
        ["v1", "memory", "context"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::MemoryGetContext,
                id: None,
            },
        ),
        // FR-API-08: the grounded context pack (`?q=` carries the task/question).
        ["v1", "memory", "context_pack"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::MemoryGetContextPack,
                id: None,
            },
        ),
        // Issue #10 (invariant 6): the Evening Wrap the notch card shows, as a read.
        ["v1", "memory", "wrap"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::MemoryGetWrap,
                id: None,
            },
        ),
        ["v1", "profile", "whoami"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::ProfileWhoami,
                id: None,
            },
        ),
        ["v1", "profile"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::ProfileSet,
                level: Level::L1,
            },
        ),
        ["v1", "voice_dictionary", "terms"] => match method {
            Method::Get => Ok(Routed::Read {
                tool: Tool::VoiceDictionaryList,
                id: None,
            }),
            Method::Post => Ok(Routed::Write {
                tool: Tool::VoiceDictionaryCreate,
                level: Level::L1,
            }),
        },
        ["v1", "voice_dictionary", "terms", id] => match id.parse::<i64>() {
            Ok(_) => method_is(
                method,
                Method::Post,
                Routed::Write {
                    tool: Tool::VoiceDictionaryUpdate,
                    level: Level::L1,
                },
            ),
            Err(_) => Err(RouteMiss::NotFound),
        },
        ["v1", "voice_dictionary", "terms", id, "delete"] => match id.parse::<i64>() {
            Ok(_) => method_is(
                method,
                Method::Post,
                Routed::Write {
                    tool: Tool::VoiceDictionaryDelete,
                    level: Level::L1,
                },
            ),
            Err(_) => Err(RouteMiss::NotFound),
        },
        ["v1", "meeting", "microphone"] => match method {
            Method::Get => Ok(Routed::Read {
                tool: Tool::MeetingMicrophoneGet,
                id: None,
            }),
            Method::Post => Ok(Routed::Write {
                tool: Tool::MeetingMicrophoneSet,
                level: Level::L1,
            }),
        },
        ["v1", "device", "onboarding"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::DeviceOnboardingGet,
                id: None,
            },
        ),
        ["v1", "visual_recall", "status"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::VisualRecallStatus,
                id: None,
            },
        ),
        ["v1", "visual_recall", "enabled"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::VisualRecallSetEnabled,
                level: Level::L1,
            },
        ),
        ["v1", "visual_recall", "retention"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::VisualRecallSetRetention,
                level: Level::L1,
            },
        ),
        ["v1", "visual_recall", "frames", "search"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::VisualRecallSearchFrames,
                id: None,
            },
        ),
        ["v1", "visual_recall", "frames", id, "rescan"] => match id.parse::<i64>() {
            Ok(parsed) => method_is(
                method,
                Method::Post,
                Routed::Read {
                    tool: Tool::VisualRecallRescanFrame,
                    id: Some(parsed),
                },
            ),
            Err(_) => Err(RouteMiss::NotFound),
        },
        ["v1", "visual_recall", "frames", "delete"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::VisualRecallDeleteFrame,
                level: Level::L1,
            },
        ),
        ["v1", "visual_recall", "frames", id] => match id.parse::<i64>() {
            Ok(parsed) => method_is(
                method,
                Method::Get,
                Routed::Read {
                    tool: Tool::VisualRecallGetFrame,
                    id: Some(parsed),
                },
            ),
            Err(_) => Err(RouteMiss::NotFound),
        },
        // Lessons (L5, Plan D-5 — invariant 6 symmetry with the Learned UI).
        ["v1", "lessons"] => method_is(
            method,
            Method::Get,
            Routed::Read {
                tool: Tool::LessonsList,
                id: None,
            },
        ),
        ["v1", "lessons", "active"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::LessonsSetActive,
                level: Level::L1,
            },
        ),
        ["v1", "memory", "notes"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::MemoryAppendNote,
                level: Level::L1,
            },
        ),
        ["v1", "state", "proposals"] => method_is(
            method,
            Method::Post,
            Routed::Write {
                tool: Tool::StateProposeUpdate,
                level: Level::L2,
            },
        ),
        ["v1", "actions", "execute"] => method_is(method, Method::Post, Routed::Action),
        ["v1", "actions", "status", id] => match id.parse::<u64>() {
            Ok(id) if id > 0 => method_is(method, Method::Get, Routed::ApprovalStatus { id }),
            _ => Err(RouteMiss::NotFound),
        },
        // state list: /v1/state/<noun>
        ["v1", "state", noun] => match state_tool(noun, false) {
            Some(tool) => method_is(method, Method::Get, Routed::Read { tool, id: None }),
            None => Err(RouteMiss::NotFound),
        },
        // state get: /v1/state/<noun>/<id>
        ["v1", "state", noun, id] => match (state_tool(noun, true), id.parse::<i64>()) {
            (Some(tool), Ok(parsed)) => method_is(
                method,
                Method::Get,
                Routed::Read {
                    tool,
                    id: Some(parsed),
                },
            ),
            _ => Err(RouteMiss::NotFound),
        },
        _ => Err(RouteMiss::NotFound),
    }
}

pub(super) enum RouteMiss {
    NotFound,
    MethodNotAllowed,
}

fn method_is(actual: Method, expected: Method, ok: Routed) -> Result<Routed, RouteMiss> {
    if actual == expected {
        Ok(ok)
    } else {
        Err(RouteMiss::MethodNotAllowed)
    }
}
