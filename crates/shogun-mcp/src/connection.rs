//! Per-service connection state (FR-INT-06/07). A pure state machine per integration: connect →
//! sync → (token-expiry / connect-failure → needs-reauth, amber) → reauth → …, and disconnect.
//!
//! Two isolation guarantees (FR-INT-06): a failure on one service turns **only that service** amber
//! and shows a reauth affordance **for that service alone** — the [`ConnectionRegistry`] keys state
//! per service, so applying an event to one leaves the others untouched. Until reauth, that
//! service's data is treated as "last sync point" with its freshness exposed
//! ([`ConnectionRegistry::freshness_ms`]).
//!
//! Disconnect (FR-INT-07) always deletes the Keychain token and stops syncing; whether previously
//! ingested events are deleted is the user's choice (default: keep).

use crate::scope::Service;

/// Why a service needs re-authentication (drives the amber indicator + reauth affordance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReauthReason {
    /// The OAuth token expired / was revoked.
    TokenExpired,
    /// A connection attempt failed.
    ConnectFailed,
}

/// A service's connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// Not connected (no token).
    Disconnected,
    /// Connected and synced; carries the last successful sync time (unix ms).
    Connected { last_sync_ms: i64 },
    /// Amber: needs reauth. Data is stale-but-usable — carries the last sync time so freshness can
    /// be shown (FR-INT-06). `last_sync_ms` is 0 if it never synced.
    NeedsReauth {
        reason: ReauthReason,
        last_sync_ms: i64,
    },
}

impl ConnState {
    /// Whether this state should show the amber indicator (FR-INT-06).
    pub fn is_amber(&self) -> bool {
        matches!(self, ConnState::NeedsReauth { .. })
    }

    fn last_sync(&self) -> Option<i64> {
        match self {
            ConnState::Disconnected => None,
            ConnState::Connected { last_sync_ms } => Some(*last_sync_ms),
            ConnState::NeedsReauth { last_sync_ms, .. } => Some(*last_sync_ms),
        }
    }
}

/// A connection lifecycle event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnEvent {
    /// A fresh connection was established (OAuth completed) at `ts`.
    Connected { ts: i64 },
    /// A successful sync completed at `ts`.
    Synced { ts: i64 },
    /// The token expired / was revoked.
    TokenExpired,
    /// A connection/sync attempt failed.
    ConnectFailed,
    /// Re-authentication completed at `ts`.
    Reauthed { ts: i64 },
}

/// Apply an event to a state (pure transition). Failure/expiry preserve the last sync time so the
/// UI can keep showing "last synced …" while amber (FR-INT-06).
pub fn next(state: ConnState, event: ConnEvent) -> ConnState {
    match event {
        ConnEvent::Connected { ts } => ConnState::Connected { last_sync_ms: ts },
        ConnEvent::Synced { ts } => ConnState::Connected { last_sync_ms: ts },
        ConnEvent::TokenExpired => ConnState::NeedsReauth {
            reason: ReauthReason::TokenExpired,
            last_sync_ms: state.last_sync().unwrap_or(0),
        },
        ConnEvent::ConnectFailed => ConnState::NeedsReauth {
            reason: ReauthReason::ConnectFailed,
            last_sync_ms: state.last_sync().unwrap_or(0),
        },
        // Reauth restores Connected, keeping the last sync time (a sync will refresh it).
        ConnEvent::Reauthed { ts } => ConnState::Connected {
            last_sync_ms: state.last_sync().unwrap_or(ts),
        },
    }
}

/// What a disconnect did (FR-INT-07). The token is always deleted; events are deleted only if the
/// user chose to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisconnectOutcome {
    pub token_deleted: bool,
    pub events_deleted: bool,
}

/// A registry of per-service connection state. Independent per service, so one service's failure
/// never changes another's (FR-INT-06 isolation).
pub struct ConnectionRegistry {
    states: Vec<(Service, ConnState)>,
}

impl Default for ConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionRegistry {
    /// A registry with every service disconnected.
    pub fn new() -> Self {
        Self {
            states: crate::scope::ALL_SERVICES
                .iter()
                .map(|&s| (s, ConnState::Disconnected))
                .collect(),
        }
    }

    pub fn state(&self, service: Service) -> ConnState {
        self.states
            .iter()
            .find(|(s, _)| *s == service)
            .map(|(_, st)| *st)
            .unwrap_or(ConnState::Disconnected)
    }

    fn set(&mut self, service: Service, state: ConnState) {
        if let Some(entry) = self.states.iter_mut().find(|(s, _)| *s == service) {
            entry.1 = state;
        }
    }

    /// Apply an event to one service, returning its new state. Other services are untouched.
    pub fn apply(&mut self, service: Service, event: ConnEvent) -> ConnState {
        let new = next(self.state(service), event);
        self.set(service, new);
        new
    }

    /// Disconnect a service (FR-INT-07): delete the token, stop syncing, optionally delete events.
    pub fn disconnect(&mut self, service: Service, delete_events: bool) -> DisconnectOutcome {
        self.set(service, ConnState::Disconnected);
        DisconnectOutcome {
            token_deleted: true,
            events_deleted: delete_events,
        }
    }

    /// Data freshness for a service: `now - last_sync` while it has ever synced (FR-INT-06). `None`
    /// if it never synced / is disconnected.
    pub fn freshness_ms(&self, service: Service, now_ms: i64) -> Option<i64> {
        match self.state(service).last_sync() {
            Some(ts) if ts > 0 => Some(now_ms.saturating_sub(ts)),
            _ => None,
        }
    }

    /// Services currently amber (need reauth) — for the aggregate indicator.
    pub fn amber_services(&self) -> Vec<Service> {
        self.states
            .iter()
            .filter(|(_, st)| st.is_amber())
            .map(|(s, _)| *s)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_then_sync_tracks_last_sync() {
        let s = next(ConnState::Disconnected, ConnEvent::Connected { ts: 100 });
        assert_eq!(s, ConnState::Connected { last_sync_ms: 100 });
        let s = next(s, ConnEvent::Synced { ts: 250 });
        assert_eq!(s, ConnState::Connected { last_sync_ms: 250 });
    }

    #[test]
    fn expiry_goes_amber_keeping_last_sync() {
        let s = ConnState::Connected { last_sync_ms: 250 };
        let s = next(s, ConnEvent::TokenExpired);
        assert_eq!(
            s,
            ConnState::NeedsReauth {
                reason: ReauthReason::TokenExpired,
                last_sync_ms: 250
            }
        );
        assert!(s.is_amber());
    }

    #[test]
    fn reauth_restores_connected() {
        let s = ConnState::NeedsReauth {
            reason: ReauthReason::ConnectFailed,
            last_sync_ms: 250,
        };
        let s = next(s, ConnEvent::Reauthed { ts: 900 });
        assert_eq!(s, ConnState::Connected { last_sync_ms: 250 });
        assert!(!s.is_amber());
    }

    #[test]
    fn failure_on_one_service_does_not_affect_others() {
        let mut reg = ConnectionRegistry::new();
        reg.apply(Service::Gmail, ConnEvent::Connected { ts: 100 });
        reg.apply(Service::GoogleCalendar, ConnEvent::Connected { ts: 100 });
        // Gmail token expires
        reg.apply(Service::Gmail, ConnEvent::TokenExpired);
        assert!(reg.state(Service::Gmail).is_amber());
        // Calendar is untouched
        assert_eq!(
            reg.state(Service::GoogleCalendar),
            ConnState::Connected { last_sync_ms: 100 }
        );
        assert_eq!(reg.amber_services(), vec![Service::Gmail]);
    }

    #[test]
    fn freshness_is_now_minus_last_sync() {
        let mut reg = ConnectionRegistry::new();
        reg.apply(Service::Slack, ConnEvent::Synced { ts: 1_000 });
        assert_eq!(reg.freshness_ms(Service::Slack, 1_500), Some(500));
        // amber still exposes freshness (last sync point)
        reg.apply(Service::Slack, ConnEvent::TokenExpired);
        assert_eq!(reg.freshness_ms(Service::Slack, 2_000), Some(1_000));
    }

    #[test]
    fn disconnected_service_has_no_freshness() {
        let reg = ConnectionRegistry::new();
        assert_eq!(reg.freshness_ms(Service::Notion, 5_000), None);
    }

    #[test]
    fn disconnect_deletes_token_and_events_per_choice() {
        let mut reg = ConnectionRegistry::new();
        reg.apply(Service::GitHub, ConnEvent::Connected { ts: 100 });
        // keep events (default)
        let out = reg.disconnect(Service::GitHub, false);
        assert_eq!(
            out,
            DisconnectOutcome {
                token_deleted: true,
                events_deleted: false
            }
        );
        assert_eq!(reg.state(Service::GitHub), ConnState::Disconnected);
        // delete events (opt-in)
        reg.apply(Service::GitHub, ConnEvent::Connected { ts: 200 });
        let out = reg.disconnect(Service::GitHub, true);
        assert!(out.token_deleted && out.events_deleted);
    }
}
