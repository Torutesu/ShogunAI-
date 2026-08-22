// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    shogun_shell_lib::install_app_user_model_id();
    shogun_shell_lib::run();
}
