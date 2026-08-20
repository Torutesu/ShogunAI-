use shogun_agents::entitlement::Entitlements;

use crate::memory_api::{AuthResult, TokenRegistry};

use super::rest_routing::{resolve, RouteMiss};
use super::{RestRequest, Routed};

pub fn bearer(authorization: Option<&str>) -> Option<String> {
    authorization?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

/// Route a request: resolve the endpoint, then apply auth, then the plan gate. `/v1/status` and
/// `/v1/metrics` are the two unauthenticated endpoints (localhost-bound health/discovery, no
/// capture content); every tool endpoint requires a valid token (FR-API-03) AND a plan that
/// includes the Memory API (issue #97: Pro/Trial only). The plan gate lives here in the shared
/// routing layer so the REST server and CLI face cannot drift.
pub fn route(req: &RestRequest, tokens: &TokenRegistry, ent: &Entitlements) -> Routed {
    match resolve(req.method, &req.path) {
        Err(RouteMiss::NotFound) => Routed::NotFound,
        Err(RouteMiss::MethodNotAllowed) => Routed::MethodNotAllowed,
        Ok(Routed::Status) => Routed::Status, // unauthenticated discovery
        Ok(Routed::Metrics) => Routed::Metrics, // unauthenticated health (NFR-SLO-00)
        Ok(resolved) => match tokens.authenticate(req.token.as_deref()) {
            AuthResult::Granted if ent.memory_api => resolved,
            AuthResult::Granted => Routed::PlanLocked,
            _ => Routed::Unauthorized,
        },
    }
}
