//! Ingestion-facing daemon types and connector boundary.

use super::Db;

/// The outcome of ingesting a batch of synced integration items ([`Db::ingest_integration`]):
/// how many were processed, how many were genuinely new (the `IntegrationSynced` bus count), and
/// how many low-confidence state candidates the new items yielded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IngestSummary {
    pub processed: usize,
    pub newly_inserted: usize,
    pub candidates: usize,
}

/// The first-layer connector runtime ([`shogun_integrations::ConnectorRuntime`]) hands each synced
/// batch to this sink; the daemon persists it into the event log via [`Db::ingest_integration`].
/// `newly_inserted` is what an `IntegrationSynced` bus event reports (§6.9). This keeps data gravity
/// in the core (invariant 1) — the connector crate never touches the DB.
impl shogun_integrations::IngestSink for Db {
    fn ingest(&self, items: &[shogun_mcp::sync::IngestItem]) -> usize {
        self.ingest_integration(items).newly_inserted
    }
}
