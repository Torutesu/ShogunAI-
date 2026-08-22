//! Shared MCP/REST helpers for the meeting microphone setting.

use crate::plan_source::meeting_json_path;

fn normalize(microphone: Option<String>) -> Option<String> {
    microphone.filter(|name| !name.trim().is_empty())
}

pub fn get() -> String {
    let microphone = load()
        .get("microphone")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    serde_json::json!({ "microphone": microphone }).to_string()
}

pub fn set(body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| "invalid meeting microphone request".to_string())?;
    let microphone = match value.get("microphone") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(name)) => Some(name.clone()),
        _ => return Err("meeting microphone must be a string or null".to_string()),
    };
    let microphone = normalize(microphone);
    let mut settings = load();
    let object = settings
        .as_object_mut()
        .ok_or_else(|| "meeting settings are malformed".to_string())?;
    object.insert(
        "microphone".to_string(),
        microphone
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    save(&settings)?;
    Ok(serde_json::json!({ "microphone": microphone }).to_string())
}

fn load() -> serde_json::Value {
    meeting_json_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

fn save(settings: &serde_json::Value) -> Result<(), String> {
    let path = meeting_json_path().ok_or("meeting settings unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("meeting settings unavailable: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, json).map_err(|e| format!("save failed: {e}"))?;
    std::fs::rename(&temporary, path).map_err(|e| format!("save failed: {e}"))
}
