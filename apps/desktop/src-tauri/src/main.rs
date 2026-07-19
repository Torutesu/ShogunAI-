// Prevents an extra console window on Windows in release (not relevant on macOS, kept for parity).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    shogun_desktop_spike_lib::run();
}
