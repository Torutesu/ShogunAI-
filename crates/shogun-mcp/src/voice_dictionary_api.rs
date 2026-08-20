//! Typed voice-dictionary management contract shared by desktop, MCP, CLI, and loopback REST.
//!
//! This is intentionally separate from the text-oriented Memory API write seam. Vocabulary is
//! private user data, so adapters must deserialize this closed operation set and the DB remains
//! the single validation authority.

use shogun_memory::voice_terms::{NewVoiceTerm, VoiceTerm};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceDictionaryOperation {
    List,
    Create(NewVoiceTerm),
    Update { id: i64, term: NewVoiceTerm },
    Delete { id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceDictionaryResult {
    Terms(Vec<VoiceTerm>),
    Term(VoiceTerm),
    Deleted(bool),
}

/// Typed CRUD only. Implementations must use the daemon `Db` methods and must not trace term
/// spellings or aliases.
pub trait VoiceDictionaryBackend: Send + Sync {
    fn manage_voice_dictionary(
        &self,
        operation: VoiceDictionaryOperation,
    ) -> Result<VoiceDictionaryResult, String>;
}

pub fn parse_term(body: &str) -> Result<NewVoiceTerm, String> {
    serde_json::from_str(body).map_err(|_| "invalid voice dictionary term".to_string())
}

pub fn render(result: VoiceDictionaryResult) -> String {
    match result {
        VoiceDictionaryResult::Terms(terms) => serde_json::json!({ "terms": terms }).to_string(),
        VoiceDictionaryResult::Term(term) => serde_json::json!({ "term": term }).to_string(),
        VoiceDictionaryResult::Deleted(deleted) => {
            serde_json::json!({ "deleted": deleted }).to_string()
        }
    }
}
