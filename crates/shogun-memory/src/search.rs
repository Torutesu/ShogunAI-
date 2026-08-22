//! Local hybrid search (FR-MEM-20, NFR-SLO-04).
//!
//! Public façade for focused search modules. Existing callers keep using
//! `shogun_memory::search::*`; parsing, filtering, ranking, retrieval, and tests stay isolated.

mod excerpt;
mod filtering;
mod model;
mod query;
mod ranking;
mod retrieval;

pub use excerpt::excerpt;
pub use filtering::{
    evidence_source_label, query_asks_about_screen, query_time_window, query_wants_visual_recall,
    visual_recall_window, LocalDayBounds,
};
pub use model::SearchHit;
pub use query::lexical_terms;
pub use ranking::reciprocal_rank_fusion;
pub use retrieval::{
    fts_search, fts_search_since, fts_search_source, hydrate, search, search_cold_partitions,
    search_hybrid, search_hybrid_since, search_hybrid_with_options, search_meetings,
    search_warm_first, ColdScan, ColdScanStats, DeepSearchResult, MeetingSearchHit, SearchDepth,
    SearchOptions, COLD_CUTOFF_MS, DEFAULT_MAX_COLD_PARTITIONS, WARM_WINDOW_MS,
};

pub(crate) use query::fts_query;

#[cfg(test)]
pub(crate) use query::MAX_FTS_TERMS;

#[cfg(test)]
mod tests;
