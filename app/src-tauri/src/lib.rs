//! Tauri v2 application shell (PR2: hardened, zero-egress).
//!
//! Networking is absent BY CONSTRUCTION: only `dialog` + `fs` plugins are
//! registered, there is no http/updater/aptabase plugin, and no HTTP client is
//! in the dependency graph (enforced by scripts/check-no-network.sh).

mod commands;
mod models;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Scoped file dialog + filesystem only. No network plugin exists.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::transcribe,
            models::list_model_packs,
            models::import_model_pack
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
