//! Tauri v2 application shell (PR2: hardened, zero-egress).
//!
//! Networking is absent BY CONSTRUCTION: only `dialog` + `fs` plugins are
//! registered, there is no http/updater/aptabase plugin, and no HTTP client is
//! in the dependency graph (enforced by scripts/check-no-network.sh).

mod commands;
mod library;
mod models;
mod system;

#[cfg(test)]
#[path = "../build_support.rs"]
mod build_support;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Scoped file dialog + filesystem only. No network plugin exists.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::transcribe,
            commands::debug_asr_model_override,
            models::list_model_packs,
            models::import_model_pack,
            library::save_session,
            library::list_sessions,
            library::load_session,
            library::delete_session,
            system::volume_encryption_status,
            system::delete_matter
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod security_config_tests {
    use serde_json::Value;

    #[test]
    fn renderer_cannot_read_or_write_backend_app_data() {
        let capability: Value = serde_json::from_str(include_str!("../capabilities/default.json"))
            .expect("default capability is valid JSON");
        let capability_text = capability.to_string();
        assert!(!capability_text.contains("$APPDATA"));
        assert!(!capability_text.contains("$APPLOCALDATA"));
        assert!(!capability_text.contains("fs:scope"));
        assert!(!capability_text.contains("fs:allow-read"));
        for broad_root in ["$AUDIO", "$DOCUMENT", "$DESKTOP", "$DOWNLOAD"] {
            assert!(!capability_text.contains(broad_root));
        }

        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("Tauri config is valid JSON");
        let config_text = config.to_string();
        assert!(!config_text.contains("$APPDATA"));
        assert!(!config_text.contains("$APPLOCALDATA"));
    }
}
