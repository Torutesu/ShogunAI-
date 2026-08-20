//! Meeting-query helpers kept beside the meeting domain.

use super::Db;

impl Db {
    /// Lexical search over meeting recaps and transcripts. Query-relevant, not latest-session.
    pub fn search_meetings(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<shogun_memory::search::MeetingSearchHit> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.with_conn("search.search_meetings", |c| {
            shogun_memory::search::search_meetings(c, query, limit)
        })
        .unwrap_or_default()
    }
}
