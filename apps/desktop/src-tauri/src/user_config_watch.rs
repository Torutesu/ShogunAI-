//! `~/Shougun.md` を監視して再パースし、共有状態を更新する。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use shogun_core::user_config::{
    default_path, load_or_create, load_report, render_directives, ParseReport, SectionError,
    ShougunConfig,
};

/// フロントに公開する設定状態。
#[derive(Clone, Default)]
pub struct UserConfigState {
    pub cfg: Arc<RwLock<ShougunConfig>>,
}

impl UserConfigState {
    /// 現在の設定から directives 文字列を得る。
    pub fn directives(&self) -> String {
        self.cfg
            .read()
            .map(|c| render_directives(&c))
            .unwrap_or_default()
    }
}

/// 起動時に呼ぶ: 初回ロード（無ければサンプル生成）＋ファイル監視を開始する。
pub fn spawn_user_config_watch(state: UserConfigState) {
    let Some(path) = default_path() else { return };

    if let Ok((cfg, _created)) = load_or_create(&path) {
        if let Ok(mut w) = state.cfg.write() {
            *w = cfg;
        }
    }

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher: RecommendedWatcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[user-config] watcher init failed: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            eprintln!("[user-config] watch failed: {e}");
            return;
        }
        while rx.recv().is_ok() {
            std::thread::sleep(Duration::from_millis(500));
            while rx.try_recv().is_ok() {}
            if let Ok((cfg, report)) = load_report(&path) {
                if let Ok(mut w) = state.cfg.write() {
                    *w = cfg;
                }
                if !report.ok {
                    eprintln!("[user-config] parse issues: {:?}", report.section_errors);
                }
            }
        }
    });
}

// ─── Settings UI commands ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct UserConfigStatus {
    pub exists: bool,
    pub path: String,
    pub last_updated_ms: Option<u64>,
    pub ok: bool,
    pub errors: Vec<SectionErrorDto>,
}

#[derive(serde::Serialize)]
pub struct SectionErrorDto {
    pub section: String,
    pub line: usize,
    pub message: String,
}

impl From<SectionError> for SectionErrorDto {
    fn from(e: SectionError) -> Self {
        SectionErrorDto {
            section: e.section,
            line: e.line,
            message: e.message,
        }
    }
}

fn resolved_path() -> Result<std::path::PathBuf, String> {
    shogun_core::user_config::default_path().ok_or_else(|| "could not resolve home dir".to_string())
}

#[tauri::command]
pub fn get_user_config_status() -> Result<UserConfigStatus, String> {
    let path = resolved_path()?;
    let exists = path.exists();
    let last_updated_ms = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let (report, ok): (ParseReport, bool) = if exists {
        let (_c, r) = shogun_core::user_config::load_report(&path).map_err(|e| e.to_string())?;
        let ok = r.ok;
        (r, ok)
    } else {
        (
            ParseReport {
                ok: true,
                section_errors: vec![],
            },
            true,
        )
    };
    Ok(UserConfigStatus {
        exists,
        path: path.to_string_lossy().to_string(),
        last_updated_ms,
        ok,
        errors: report.section_errors.into_iter().map(Into::into).collect(),
    })
}

#[tauri::command]
pub fn open_shougun_md() -> Result<(), String> {
    let path = resolved_path()?;
    std::process::Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("failed to open: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn regenerate_shougun_md(state: tauri::State<'_, UserConfigState>) -> Result<(), String> {
    let path = resolved_path()?;
    let sample = shogun_core::user_config::sample_markdown();
    std::fs::write(&path, &sample).map_err(|e| e.to_string())?;
    let (cfg, _r) = shogun_core::user_config::parse_shougun(&sample);
    if let Ok(mut w) = state.cfg.write() {
        *w = cfg;
    }
    Ok(())
}
