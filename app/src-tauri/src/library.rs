//! Transcript-library persistence (the "saved sessions" store).
//!
//! A "session" is ONE saved transcript, stored as a single JSON file
//! `<library>/<id>.json`. Rust treats the session BODY as an opaque
//! `serde_json::Value` passthrough — the TypeScript frontend owns the schema.
//! Rust only reads a handful of top-level fields for listing: `id`,
//! `createdAt`, `updatedAt`, `audioPath`, `audioFilename`, `diarized`,
//! `segments`.
//!
//! Filesystem logic lives in `*_in(dir, …)` cores that never panic on bad
//! input; the `#[tauri::command]` wrappers only resolve the library dir.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

/// Resolve the library directory: `KINDTECH_LIBRARY_DIR` env (dev/test) else the
/// per-user app-data `library/` dir. Created on demand by `save_session_in`.
pub(crate) fn library_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("KINDTECH_LIBRARY_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve app data dir: {e}"))?;
    Ok(base.join("library"))
}

/// True iff `id` is a safe single filename component: non-empty and free of
/// path separators (`/`, `\`) or `..` parent traversal. Guards every read,
/// write, and delete against path-traversal out of the library dir.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\') && !id.contains("..")
}

/// Listing-row metadata for one saved session, surfaced to the UI. The schema
/// owner is the frontend; these are the only fields Rust reads back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub audio_filename: String,
    pub diarized: bool,
    pub segments: u64,
    /// True iff the session's `audioPath` is non-empty and still exists on disk.
    pub audio_available: bool,
}

/// A fully-loaded session: the opaque stored body plus whether its audio file is
/// still present (so the UI can offer "re-link audio" when it has moved).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSession {
    pub session: Value,
    pub audio_available: bool,
}

/// Read a top-level string field, defaulting to `""` when missing/non-string.
fn str_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// True iff `audio_path` is non-empty AND points at an existing file.
fn audio_available(audio_path: &str) -> bool {
    !audio_path.is_empty() && Path::new(audio_path).exists()
}

/// Persist `value` as `<dir>/<id>.json`. Reads `value["id"]` as a
/// non-empty, traversal-safe string (else `Err`), creates `dir`, and writes
/// ATOMICALLY via a `<id>.json.tmp` temp file + rename so a crash mid-write
/// never corrupts an existing session.
pub fn save_session_in(dir: &Path, value: &Value) -> Result<(), String> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "session has no string \"id\"".to_string())?;
    if !is_safe_id(id) {
        return Err(format!("unsafe session id: {id:?}"));
    }
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("cannot create library dir {}: {e}", dir.display()))?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| format!("cannot serialize session: {e}"))?;
    let tmp = dir.join(format!("{id}.json.tmp"));
    let final_path = dir.join(format!("{id}.json"));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &final_path)
        .map_err(|e| format!("cannot finalize {}: {e}", final_path.display()))?;
    Ok(())
}

/// List every saved session in `dir`, newest-first by `updatedAt` (ISO-8601
/// strings sort chronologically). Reads only `*.json` (not in-flight `*.tmp`);
/// SKIPS any that fail to read/parse or lack a string `id` (corrupt-tolerant,
/// never panics). Returns `[]` when `dir` is absent.
pub fn list_sessions_in(dir: &Path) -> Result<Vec<SessionMeta>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let mut out: Vec<SessionMeta> = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = match value.get("id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => continue,
        };
        let audio_path = str_field(&value, "audioPath");
        out.push(SessionMeta {
            id,
            created_at: str_field(&value, "createdAt"),
            updated_at: str_field(&value, "updatedAt"),
            audio_filename: str_field(&value, "audioFilename"),
            diarized: value.get("diarized").and_then(Value::as_bool).unwrap_or(false),
            segments: value.get("segments").and_then(Value::as_u64).unwrap_or(0),
            audio_available: audio_available(&audio_path),
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// Load the full session `<dir>/<id>.json`. Rejects unsafe ids,
/// errors if the file is absent or unparseable, and reports whether its audio
/// file still exists. Returns the EXACT stored body untouched.
pub fn load_session_in(dir: &Path, id: &str) -> Result<LoadedSession, String> {
    if !is_safe_id(id) {
        return Err(format!("unsafe session id: {id:?}"));
    }
    let path = dir.join(format!("{id}.json"));
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("cannot read session {id}: {e}"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("session {id} is not valid JSON: {e}"))?;
    let audio_path = str_field(&value, "audioPath");
    Ok(LoadedSession {
        audio_available: audio_available(&audio_path),
        session: value,
    })
}

/// Delete `<dir>/<id>.json`. Rejects unsafe ids. A missing file is
/// `Ok(())` (idempotent) so a double-delete never errors.
pub fn delete_session_in(dir: &Path, id: &str) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("unsafe session id: {id:?}"));
    }
    let path = dir.join(format!("{id}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot delete session {id}: {e}")),
    }
}

/// Persist a transcript session. The `session` Value owns its own schema; Rust
/// requires only a non-empty, traversal-safe string `id`.
#[tauri::command]
pub fn save_session(app: tauri::AppHandle, session: Value) -> Result<(), String> {
    let dir = library_dir(&app)?;
    save_session_in(&dir, &session)
}

/// List every saved session, newest-first. Empty list (not an error) on a fresh
/// install with no library dir yet.
#[tauri::command]
pub fn list_sessions(app: tauri::AppHandle) -> Result<Vec<SessionMeta>, String> {
    let dir = library_dir(&app)?;
    list_sessions_in(&dir)
}

/// Load one saved session by id, with its audio-availability flag.
#[tauri::command]
pub fn load_session(app: tauri::AppHandle, id: String) -> Result<LoadedSession, String> {
    let dir = library_dir(&app)?;
    load_session_in(&dir, &id)
}

/// Delete one saved session by id (idempotent).
#[tauri::command]
pub fn delete_session(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = library_dir(&app)?;
    delete_session_in(&dir, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn unique_tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "podcast-library-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// save → list → load round-trip: two sessions persist, list returns both
    /// with the right fields, and load returns the EXACT stored body (nested
    /// `editorState` intact).
    #[test]
    fn save_list_load_round_trip() {
        let dir = unique_tmp("roundtrip");
        let _ = fs::remove_dir_all(&dir);

        let s1 = json!({
            "id": "alpha",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "audioPath": "",
            "audioFilename": "alpha.mp3",
            "diarized": true,
            "segments": 12,
            "editorState": { "blocks": [{ "text": "hello" }], "cursor": 3 }
        });
        let s2 = json!({
            "id": "beta",
            "createdAt": "2026-02-01T00:00:00Z",
            "updatedAt": "2026-02-01T00:00:00Z",
            "audioPath": "",
            "audioFilename": "beta.wav",
            "diarized": false,
            "segments": 5,
            "editorState": { "blocks": [], "cursor": 0 }
        });
        save_session_in(&dir, &s1).unwrap();
        save_session_in(&dir, &s2).unwrap();

        let metas = list_sessions_in(&dir).unwrap();
        assert_eq!(metas.len(), 2, "both sessions listed");

        let alpha = metas.iter().find(|m| m.id == "alpha").unwrap();
        assert_eq!(alpha.audio_filename, "alpha.mp3");
        assert!(alpha.diarized);
        assert_eq!(alpha.segments, 12);
        assert_eq!(alpha.created_at, "2026-01-01T00:00:00Z");

        let beta = metas.iter().find(|m| m.id == "beta").unwrap();
        assert!(!beta.diarized);
        assert_eq!(beta.segments, 5);

        // load returns the EXACT stored Value, editorState untouched.
        let loaded = load_session_in(&dir, "alpha").unwrap();
        assert_eq!(loaded.session, s1, "loaded body must equal stored body");
        assert_eq!(loaded.session["editorState"]["cursor"], json!(3));

        let _ = fs::remove_dir_all(&dir);
    }

    /// list sort: sessions come back newest-first by `updatedAt`.
    #[test]
    fn list_sorts_newest_first() {
        let dir = unique_tmp("sort");
        let _ = fs::remove_dir_all(&dir);

        let older = json!({
            "id": "older", "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z", "audioPath": "",
            "audioFilename": "", "diarized": false, "segments": 0
        });
        let newer = json!({
            "id": "newer", "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-03-01T00:00:00Z", "audioPath": "",
            "audioFilename": "", "diarized": false, "segments": 0
        });
        save_session_in(&dir, &older).unwrap();
        save_session_in(&dir, &newer).unwrap();

        let metas = list_sessions_in(&dir).unwrap();
        assert_eq!(metas.len(), 2);
        assert_eq!(metas[0].id, "newer", "newest updatedAt must sort first");
        assert_eq!(metas[1].id, "older");

        let _ = fs::remove_dir_all(&dir);
    }

    /// corrupt file: a non-JSON `garbage.json` is SKIPPED, not a panic/error.
    #[test]
    fn corrupt_file_is_skipped() {
        let dir = unique_tmp("corrupt");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let good = json!({
            "id": "good", "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z", "audioPath": "",
            "audioFilename": "", "diarized": false, "segments": 0
        });
        save_session_in(&dir, &good).unwrap();
        fs::write(dir.join("garbage.json"), b"not json at all {{{").unwrap();

        let metas = list_sessions_in(&dir).unwrap();
        assert_eq!(metas.len(), 1, "only the valid session is listed");
        assert_eq!(metas[0].id, "good");

        let _ = fs::remove_dir_all(&dir);
    }

    /// delete: removes the file; a second delete of the same id is still Ok.
    #[test]
    fn delete_is_idempotent() {
        let dir = unique_tmp("delete");
        let _ = fs::remove_dir_all(&dir);

        let s = json!({
            "id": "gone", "createdAt": "", "updatedAt": "",
            "audioPath": "", "audioFilename": "", "diarized": false, "segments": 0
        });
        save_session_in(&dir, &s).unwrap();
        assert!(load_session_in(&dir, "gone").is_ok());

        delete_session_in(&dir, "gone").unwrap();
        assert!(load_session_in(&dir, "gone").is_err(), "session removed");
        assert!(list_sessions_in(&dir).unwrap().is_empty(), "session no longer listed");

        // Second delete of the same id is still Ok (idempotent).
        delete_session_in(&dir, "gone").expect("double delete must be Ok");

        let _ = fs::remove_dir_all(&dir);
    }

    /// traversal guard: unsafe ids are rejected by save, load, and delete.
    #[test]
    fn rejects_path_traversal_ids() {
        let dir = unique_tmp("traversal");
        let _ = fs::remove_dir_all(&dir);

        let evil = json!({ "id": "../evil" });
        assert!(save_session_in(&dir, &evil).is_err(), "save must reject ../evil");
        assert!(load_session_in(&dir, "../evil").is_err(), "load must reject ../evil");
        assert!(delete_session_in(&dir, "../evil").is_err(), "delete must reject ../evil");
        // Separator-bearing and empty ids too.
        assert!(load_session_in(&dir, "sub/dir").is_err());
        assert!(load_session_in(&dir, "").is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    /// audioAvailable: a real audio file → true; a bogus path → false.
    #[test]
    fn audio_available_reflects_disk() {
        let dir = unique_tmp("audio");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let real_audio = dir.join("real.wav");
        fs::write(&real_audio, b"RIFF....").unwrap();

        let present = json!({
            "id": "present", "createdAt": "", "updatedAt": "",
            "audioPath": real_audio.to_string_lossy(), "audioFilename": "real.wav",
            "diarized": false, "segments": 0
        });
        let missing = json!({
            "id": "missing", "createdAt": "", "updatedAt": "",
            "audioPath": "/no/such/path/bogus.wav", "audioFilename": "bogus.wav",
            "diarized": false, "segments": 0
        });
        save_session_in(&dir, &present).unwrap();
        save_session_in(&dir, &missing).unwrap();

        let loaded_present = load_session_in(&dir, "present").unwrap();
        assert!(loaded_present.audio_available, "real audio file → available");
        let loaded_missing = load_session_in(&dir, "missing").unwrap();
        assert!(!loaded_missing.audio_available, "bogus path → unavailable");

        let metas = list_sessions_in(&dir).unwrap();
        assert!(metas.iter().find(|m| m.id == "present").unwrap().audio_available);
        assert!(!metas.iter().find(|m| m.id == "missing").unwrap().audio_available);

        let _ = fs::remove_dir_all(&dir);
    }
}
