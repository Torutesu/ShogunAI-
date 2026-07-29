//! `~/Shougun.md` を監視して再パースし、共有状態を更新する。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use shogun_core::user_config::{
    default_path, load_or_create, load_report, render_directives, ShougunConfig,
};

/// フロントに公開する設定状態。
#[derive(Clone, Default)]
pub struct UserConfigState {
    pub cfg: Arc<RwLock<ShougunConfig>>,
}

impl UserConfigState {
    /// 現在の設定から directives 文字列を得る。
    pub fn directives(&self) -> String {
        self.cfg.read().map(|c| render_directives(&c)).unwrap_or_default()
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
