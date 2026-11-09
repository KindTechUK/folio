//! Data-at-rest helpers: volume-encryption status + "delete matter".
//!
//! A firm running client-confidential matters can (a) be warned when the
//! storage volume is NOT confirmed encrypted (BitLocker on Windows) and (b)
//! delete a matter's source audio and saved transcript. The engine writes no
//! temp files.
//!
//! Everything is network-free. [`parse_manage_bde_status`] and
//! [`delete_matter_authorized_in`] are the unit-tested cores (never panic); the
//! `#[tauri::command]` wrappers add platform paths and native confirmation.
//! Encryption checks fail SOFT (`Unknown`, never an app error).

use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Volume-encryption verdict from parsing `manage-bde -status <drive>` text.
///
/// The `Encrypted`/`Unencrypted` variants are produced only by the Windows
/// `manage-bde` path (and the unit tests); on non-Windows the command wrapper
/// always returns `Unknown`, so allow dead-code there to keep dev builds clean.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionState {
    Encrypted,
    Unencrypted,
    Unknown,
}

impl EncryptionState {
    /// Stable string the frontend keys its banner on.
    fn as_str(self) -> &'static str {
        match self {
            EncryptionState::Encrypted => "encrypted",
            EncryptionState::Unencrypted => "unencrypted",
            EncryptionState::Unknown => "unknown",
        }
    }
}

/// Encryption status surfaced to the UI: a `state` string
/// (`"encrypted" | "unencrypted" | "unknown"`) plus a human-readable `detail`.
#[derive(Debug, Clone, Serialize)]
pub struct EncryptionStatus {
    pub state: String,
    pub detail: String,
}

/// One failed deletion: the path and why it could not be removed.
#[derive(Debug, Clone, Serialize)]
pub struct DeleteFailure {
    pub path: String,
    pub error: String,
}

/// Outcome of a delete-matter request: paths removed (or already gone) vs. paths
/// that genuinely failed to delete.
#[derive(Debug, Clone, Serialize)]
pub struct DeleteReport {
    pub deleted: Vec<String>,
    pub failed: Vec<DeleteFailure>,
}

/// Parse `manage-bde -status <drive>` text (case-insensitive,
/// whitespace-collapsed; never panics).
///
/// Encrypted only when `Protection Status: Protection On` AND conversion is
/// complete (`Conversion Status: Fully Encrypted` or `Percentage Encrypted:
/// 100%`/`100.0%`); suspended protection must never inherit a green status from
/// the conversion fields.
///
/// Unencrypted on any of `Fully Decrypted`, `Percentage Encrypted: 0%`/`0.0%`,
/// or `Protection Status: Protection Off`. Otherwise
/// [`EncryptionState::Unknown`].
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_manage_bde_status(output: &str) -> EncryptionState {
    // Lowercase + collapse runs of whitespace to single spaces so the field
    // labels in real `manage-bde` output (which pads `Label:` to a column with
    // many spaces) match our single-spaced needles.
    let lc: String = {
        let lower = output.to_lowercase();
        let mut out = String::with_capacity(lower.len());
        let mut prev_ws = false;
        for ch in lower.chars() {
            if ch.is_whitespace() {
                if !prev_ws {
                    out.push(' ');
                }
                prev_ws = true;
            } else {
                out.push(ch);
                prev_ws = false;
            }
        }
        out
    };

    let protection_on = lc.contains("protection status: protection on");
    let conversion_complete = lc.contains("conversion status: fully encrypted")
        || lc.contains("percentage encrypted: 100%")
        || lc.contains("percentage encrypted: 100.0%");
    let encrypted = protection_on && conversion_complete;
    if encrypted {
        return EncryptionState::Encrypted;
    }

    let unencrypted = lc.contains("fully decrypted")
        || lc.contains("percentage encrypted: 0%")
        || lc.contains("percentage encrypted: 0.0%")
        || lc.contains("protection status: protection off");
    if unencrypted {
        return EncryptionState::Unencrypted;
    }

    EncryptionState::Unknown
}

/// For each path: if it exists, remove the file
/// (`remove_file`); a path that is already gone counts as `deleted` (idempotent,
/// not a failure); anything that genuinely fails to remove lands in `failed`
/// with its error string. Never panics.
pub fn delete_paths(paths: &[String]) -> DeleteReport {
    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for path in paths {
        let p = std::path::Path::new(path);
        if !p.exists() {
            // Already gone — the matter's artifact is absent, which is the goal.
            deleted.push(path.clone());
            continue;
        }
        match std::fs::remove_file(p) {
            Ok(()) => deleted.push(path.clone()),
            Err(e) => failed.push(DeleteFailure {
                path: path.clone(),
                error: e.to_string(),
            }),
        }
    }

    DeleteReport { deleted, failed }
}

const APPROVED_RECORDINGS_FILE: &str = ".approved-recordings";

fn approved_recordings_in(library_dir: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let path = library_dir.join(APPROVED_RECORDINGS_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(format!(
                "cannot read approved-recordings registry {}: {error}",
                path.display()
            ))
        }
    };
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "approved-recordings registry {} is invalid: {error}",
            path.display()
        )
    })
}

fn write_approved_recordings_in(
    library_dir: &Path,
    recordings: &BTreeSet<PathBuf>,
) -> Result<(), String> {
    std::fs::create_dir_all(library_dir).map_err(|error| {
        format!(
            "cannot create library dir {}: {error}",
            library_dir.display()
        )
    })?;
    let path = library_dir.join(APPROVED_RECORDINGS_FILE);
    let tmp = library_dir.join(format!("{APPROVED_RECORDINGS_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(recordings)
        .map_err(|error| format!("cannot serialize approved recordings: {error}"))?;
    std::fs::write(&tmp, bytes)
        .map_err(|error| format!("cannot write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .map_err(|error| format!("cannot finalize {}: {error}", path.display()))
}

/// Persist the exact canonical file selected by the backend-owned native file
/// picker. Renderer-provided paths never enter this registry. Symlinks are
/// rejected so later confirmation and deletion refer to the same filesystem
/// entry the user selected.
pub fn approve_recording_in(library_dir: &Path, path: &Path) -> Result<PathBuf, String> {
    let canonical = canonical_recording(path)?;
    let mut approved = approved_recordings_in(library_dir)?;
    approved.insert(canonical.clone());
    write_approved_recordings_in(library_dir, &approved)?;
    Ok(canonical)
}

fn canonical_recording(path: &Path) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect selected recording {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to approve a symlink recording path: {}",
            path.display()
        ));
    }
    let canonical = path.canonicalize().map_err(|error| {
        format!(
            "cannot resolve selected recording {}: {error}",
            path.display()
        )
    })?;
    if !canonical.is_file() {
        return Err(format!(
            "selected recording is not a regular file: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

/// Re-authorize a recording saved by an older release. The user must select
/// the exact same canonical regular file in a new backend-owned native dialog;
/// choosing any sibling or replacement path fails closed.
pub fn reauthorize_recording_in(
    library_dir: &Path,
    expected: &Path,
    selected: &Path,
) -> Result<PathBuf, String> {
    let expected = canonical_recording(expected)?;
    let selected = canonical_recording(selected)?;
    if selected != expected {
        return Err(format!(
            "selected recording does not match the saved matter path: {}",
            expected.display()
        ));
    }
    let mut approved = approved_recordings_in(library_dir)?;
    approved.insert(expected.clone());
    write_approved_recordings_in(library_dir, &approved)?;
    Ok(expected)
}

fn recording_is_approved_in(library_dir: &Path, path: &Path) -> Result<bool, String> {
    let canonical = canonical_recording(path)?;
    Ok(approved_recordings_in(library_dir)?.contains(&canonical))
}

fn forget_approved_recording_in(library_dir: &Path, path: &Path) {
    let Ok(mut approved) = approved_recordings_in(library_dir) else {
        return;
    };
    if approved.remove(path) {
        let _ = write_approved_recordings_in(library_dir, &approved);
    }
}

/// Delete the audio path recorded by a backend-owned saved session, but only
/// when it resolves to the exact canonical file previously approved by the
/// backend-owned native picker. This permits recordings on external drives and
/// in arbitrary folders without making the command an arbitrary file-removal
/// primitive. Symlink paths are always rejected.
pub fn delete_matter_authorized_in<F>(
    library_dir: &Path,
    id: &str,
    authorize: F,
) -> Result<DeleteReport, String>
where
    F: FnOnce(Option<&Path>, bool) -> bool,
{
    let loaded = crate::library::load_session_in(library_dir, id)?;
    let audio_path = loaded
        .session
        .get("audioPath")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if audio_path.is_empty() {
        if !authorize(None, false) {
            return Err("matter deletion was cancelled".to_string());
        }
        crate::library::delete_session_in(library_dir, id)?;
        return Ok(DeleteReport {
            deleted: Vec::new(),
            failed: Vec::new(),
        });
    }

    let audio = Path::new(audio_path);
    if !audio.exists() {
        if !authorize(Some(audio), false) {
            return Err("matter deletion was cancelled".to_string());
        }
        forget_approved_recording_in(library_dir, audio);
        crate::library::delete_session_in(library_dir, id)?;
        return Ok(DeleteReport {
            deleted: vec![audio_path.to_string()],
            failed: Vec::new(),
        });
    }

    let metadata = std::fs::symlink_metadata(audio)
        .map_err(|error| format!("cannot inspect saved audio path {audio_path:?}: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to delete a symlink matter path: {}",
            audio.display()
        ));
    }
    let canonical_audio = audio
        .canonicalize()
        .map_err(|e| format!("cannot resolve saved audio path {audio_path:?}: {e}"))?;
    let approved = approved_recordings_in(library_dir)?;
    if !approved.contains(&canonical_audio) {
        return Err(format!(
            "refusing to delete a recording not approved by the native picker: {}",
            canonical_audio.display()
        ));
    }
    if !canonical_audio.is_file() {
        return Err(format!(
            "refusing to delete a non-file matter path: {}",
            canonical_audio.display()
        ));
    }
    if !authorize(Some(&canonical_audio), true) {
        return Err("matter deletion was cancelled".to_string());
    }

    let canonical_string = canonical_audio.to_string_lossy().into_owned();
    let report = delete_paths(&[canonical_string]);
    if report.failed.is_empty() {
        forget_approved_recording_in(library_dir, &canonical_audio);
        crate::library::delete_session_in(library_dir, id)?;
    }
    Ok(report)
}

/// Report the encryption state of the volume holding `path`.
///
/// On Windows: derive the drive (e.g. `C:`) from `path`, run
/// `manage-bde -status <drive>`, parse with [`parse_manage_bde_status`], and map
/// to [`EncryptionStatus`]. If the command cannot run (missing tool, no perms,
/// odd path), return `state = "unknown"` with an explanatory `detail` — this
/// check must NEVER error the app.
///
/// On non-Windows (dev/macOS): always `state = "unknown"` with a soft note that
/// the check is Windows-only.
#[tauri::command]
pub fn volume_encryption_status(path: String) -> Result<EncryptionStatus, String> {
    #[cfg(windows)]
    {
        Ok(windows_encryption_status(&path))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(EncryptionStatus {
            state: EncryptionState::Unknown.as_str().to_string(),
            detail: "encryption status check is only available on Windows".to_string(),
        })
    }
}

/// Windows-only worker: derive the drive from `path`, shell out to `manage-bde`,
/// and parse the result. Fails SOFT to `Unknown` on every error path.
#[cfg(windows)]
fn windows_encryption_status(path: &str) -> EncryptionStatus {
    let drive = match drive_letter(path) {
        Some(d) => d,
        None => {
            return EncryptionStatus {
                state: EncryptionState::Unknown.as_str().to_string(),
                detail: format!("could not derive a drive letter from path: {path}"),
            }
        }
    };

    let output = std::process::Command::new("manage-bde")
        .arg("-status")
        .arg(&drive)
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let state = parse_manage_bde_status(&text);
            let detail = match state {
                EncryptionState::Encrypted => format!("{drive} reported as BitLocker-encrypted"),
                EncryptionState::Unencrypted => {
                    format!("{drive} is not BitLocker-encrypted")
                }
                EncryptionState::Unknown => {
                    format!("could not determine encryption status for {drive}")
                }
            };
            EncryptionStatus {
                state: state.as_str().to_string(),
                detail,
            }
        }
        Err(e) => EncryptionStatus {
            state: EncryptionState::Unknown.as_str().to_string(),
            detail: format!("could not run manage-bde for {drive}: {e}"),
        },
    }
}

/// Derive a `C:`-style drive spec from a Windows path. Returns `None` when the
/// path has no recognizable drive prefix.
#[cfg(windows)]
fn drive_letter(path: &str) -> Option<String> {
    let mut chars = path.chars();
    let first = chars.next()?;
    let second = chars.next()?;
    if first.is_ascii_alphabetic() && second == ':' {
        Some(format!("{}:", first.to_ascii_uppercase()))
    } else {
        None
    }
}

/// Pick a recording through a backend-owned native dialog and persist its exact
/// canonical path in a backend-only approval registry. The renderer receives
/// the path for transcription but cannot add arbitrary registry entries.
#[tauri::command]
pub async fn pick_recording(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let selected = app
            .dialog()
            .file()
            .add_filter(
                "Audio",
                &["wav", "m4a", "mp3", "flac", "mp4", "mov", "aac", "ogg"],
            )
            .blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let path = selected
            .into_path()
            .map_err(|error| format!("selected recording is not a local file: {error}"))?;
        let approved = approve_recording_in(&library, &path)?;
        Ok(Some(approved.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|error| format!("recording-picker task failed: {error}"))?
}

/// Delete the source audio and saved transcript for a session. The renderer
/// supplies only the session id; the backend resolves the stored path and
/// requires an exact native-picker approval, with an exact-file re-selection
/// flow for legacy sessions. A backend-owned dialog confirms the complete
/// deletion even when the source recording is already missing.
#[tauri::command]
pub async fn delete_matter(app: tauri::AppHandle, id: String) -> Result<DeleteReport, String> {
    let library = crate::library::library_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let loaded = crate::library::load_session_in(&library, &id)?;
        let saved_audio = loaded
            .session
            .get("audioPath")
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);

        if let Some(saved_audio) = saved_audio.as_deref().filter(|path| path.exists()) {
            if !recording_is_approved_in(&library, saved_audio)? {
                let continue_reauthorization = app
                    .dialog()
                    .message(format!(
                        "This matter was imported by an older Folio version. To delete it safely, re-select this exact source recording:\n\n{}",
                        saved_audio.display()
                    ))
                    .title("Re-authorize source recording")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "Re-select recording".to_string(),
                        "Cancel".to_string(),
                    ))
                    .blocking_show();
                if !continue_reauthorization {
                    return Err("matter deletion was cancelled".to_string());
                }
                let selected = app
                    .dialog()
                    .file()
                    .add_filter(
                        "Audio",
                        &["wav", "m4a", "mp3", "flac", "mp4", "mov", "aac", "ogg"],
                    )
                    .blocking_pick_file()
                    .ok_or_else(|| "recording re-authorization was cancelled".to_string())?
                    .into_path()
                    .map_err(|error| {
                        format!("selected recording is not a local file: {error}")
                    })?;
                reauthorize_recording_in(&library, saved_audio, &selected)?;
            }
        }

        delete_matter_authorized_in(&library, &id, |audio_path, audio_exists| {
            let source_detail = match (audio_path, audio_exists) {
                (Some(path), true) => format!("Source recording:\n{}", path.display()),
                (Some(path), false) => {
                    format!("Source recording is already missing:\n{}", path.display())
                }
                (None, _) => "No source recording is attached.".to_string(),
            };
            app.dialog()
                .message(format!(
                    "Permanently delete this matter?\n\n{source_detail}\n\nThe saved transcript will also be permanently deleted."
                ))
                .title("Delete matter")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Delete".to_string(),
                    "Cancel".to_string(),
                ))
                .blocking_show()
        })
    })
    .await
    .map_err(|e| format!("delete-matter task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    fn unique_tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "podcast-system-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    // --- parse_manage_bde_status --------------------------------------------

    /// Representative `manage-bde -status C:` output for a fully-encrypted,
    /// protection-on volume → Encrypted.
    #[test]
    fn parses_encrypted_status() {
        let sample = "\
BitLocker Drive Encryption: Configuration Tool version 10.0.19041
Copyright (C) 2013 Microsoft Corporation. All rights reserved.

Volume C: [OSDisk]
[OS Volume]

    Size:                 475.43 GB
    BitLocker Version:    2.0
    Conversion Status:    Fully Encrypted
    Percentage Encrypted: 100.0%
    Encryption Method:    XTS-AES 128
    Protection Status:    Protection On
    Lock Status:          Unlocked
    Identification Field: Unknown
    Key Protectors:
        TPM
        Numerical Password
";
        assert_eq!(parse_manage_bde_status(sample), EncryptionState::Encrypted);
    }

    /// Representative output for a decrypted/protection-off volume → Unencrypted.
    #[test]
    fn parses_unencrypted_status() {
        let sample = "\
BitLocker Drive Encryption: Configuration Tool version 10.0.19041
Copyright (C) 2013 Microsoft Corporation. All rights reserved.

Volume D: [Data]
[Data Volume]

    Size:                 931.51 GB
    BitLocker Version:    None
    Conversion Status:    Fully Decrypted
    Percentage Encrypted: 0.0%
    Encryption Method:    None
    Protection Status:    Protection Off
    Lock Status:          Unlocked
    Identification Field: None
    Key Protectors:       None Found
";
        assert_eq!(
            parse_manage_bde_status(sample),
            EncryptionState::Unencrypted
        );
    }

    /// A fully converted volume with protection suspended is not protected at
    /// rest: `Protection Off` must override the 100% conversion signals.
    #[test]
    fn suspended_bitlocker_is_not_reported_as_encrypted() {
        let sample = "\
Volume C: [OSDisk]
    Conversion Status:    Fully Encrypted
    Percentage Encrypted: 100.0%
    Protection Status:    Protection Off
";
        assert_eq!(
            parse_manage_bde_status(sample),
            EncryptionState::Unencrypted
        );
    }

    /// Garbage / empty / unrelated text → Unknown, never a panic.
    #[test]
    fn parses_unknown_for_garbage() {
        assert_eq!(parse_manage_bde_status(""), EncryptionState::Unknown);
        assert_eq!(
            parse_manage_bde_status("ERROR: An unknown error occurred (0x...)"),
            EncryptionState::Unknown
        );
        assert_eq!(
            parse_manage_bde_status("the quick brown fox"),
            EncryptionState::Unknown
        );
    }

    /// Heuristics are case-insensitive, but incomplete output is not enough to
    /// claim the volume is protected.
    #[test]
    fn parses_case_insensitively() {
        assert_eq!(
            parse_manage_bde_status("PROTECTION STATUS:    PROTECTION ON"),
            EncryptionState::Unknown
        );
        assert_eq!(
            parse_manage_bde_status("conversion status: fully encrypted"),
            EncryptionState::Unknown
        );
    }

    // --- delete_paths -------------------------------------------------------

    /// Existing files are removed and listed under `deleted`; nothing fails.
    #[test]
    fn delete_paths_removes_existing_files() {
        let dir = unique_tmp("del");
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("audio.wav");
        let b = dir.join("audio.mp3");
        fs::write(&a, b"x").unwrap();
        fs::write(&b, b"y").unwrap();

        let paths = vec![
            a.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
        ];
        let report = delete_paths(&paths);

        assert_eq!(report.deleted.len(), 2);
        assert!(report.failed.is_empty());
        assert!(!a.exists());
        assert!(!b.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    /// A non-existent path is idempotently reported as deleted (already gone),
    /// not as a failure.
    #[test]
    fn delete_paths_treats_missing_as_deleted() {
        let dir = unique_tmp("missing");
        let ghost = dir.join("never-existed.wav");
        let paths = vec![ghost.to_string_lossy().into_owned()];

        let report = delete_paths(&paths);

        assert_eq!(report.deleted, paths);
        assert!(report.failed.is_empty());
    }

    /// Best-effort: a path that cannot be removed as a file (here: an existing
    /// DIRECTORY, which `remove_file` refuses) lands in `failed` with an error
    /// string, and the helper does not panic.
    #[test]
    fn delete_paths_reports_undeletable_in_failed() {
        let dir = unique_tmp("undeletable");
        fs::create_dir_all(&dir).unwrap();
        let paths = vec![dir.to_string_lossy().into_owned()];

        let report = delete_paths(&paths);

        assert!(report.deleted.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].path, paths[0]);
        assert!(!report.failed[0].error.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    fn save_session(library: &Path, id: &str, audio_path: &Path) {
        crate::library::save_session_in(
            library,
            &json!({
                "id": id,
                "audioPath": audio_path.to_string_lossy(),
            }),
        )
        .unwrap();
    }

    #[test]
    fn delete_matter_removes_an_exact_backend_approved_recording_from_any_folder() {
        let dir = unique_tmp("matter-allowed");
        let library = dir.join("library");
        let external_drive = dir.join("external-drive");
        fs::create_dir_all(&external_drive).unwrap();
        let audio = external_drive.join("recording.wav");
        fs::write(&audio, b"audio").unwrap();
        let approved = approve_recording_in(&library, &audio).unwrap();
        save_session(&library, "matter-1", &audio);

        let report = delete_matter_authorized_in(&library, "matter-1", |_, _| true).unwrap();

        assert_eq!(report.deleted, vec![approved.to_string_lossy()]);
        assert!(report.failed.is_empty());
        assert!(!audio.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_matter_rejects_a_saved_path_not_approved_by_the_backend_picker() {
        let dir = unique_tmp("matter-outside");
        let library = dir.join("library");
        let outside = dir.join("unrelated.txt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&outside, b"keep").unwrap();
        save_session(&library, "matter-2", &outside);

        let result = delete_matter_authorized_in(&library, "matter-2", |_, _| true);

        assert!(result.is_err());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_matter_requires_backend_authorizer_approval() {
        let dir = unique_tmp("matter-denied");
        let library = dir.join("library");
        let audio_root = dir.join("audio");
        fs::create_dir_all(&audio_root).unwrap();
        let audio = audio_root.join("recording.wav");
        fs::write(&audio, b"keep").unwrap();
        approve_recording_in(&library, &audio).unwrap();
        save_session(&library, "matter-denied", &audio);

        let result = delete_matter_authorized_in(&library, "matter-denied", |_, _| false);

        assert!(result.is_err());
        assert!(audio.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn backend_picker_approval_rejects_symlink_paths() {
        use std::os::unix::fs::symlink;

        let dir = unique_tmp("matter-symlink");
        let library = dir.join("library");
        let audio_root = dir.join("audio");
        let outside_root = dir.join("outside");
        fs::create_dir_all(&audio_root).unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        let outside = outside_root.join("unrelated.txt");
        let link = audio_root.join("recording.wav");
        fs::write(&outside, b"keep").unwrap();
        symlink(&outside, &link).unwrap();
        let result = approve_recording_in(&library, &link);

        assert!(result.is_err());
        assert!(link.exists());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn delete_matter_rejects_a_symlink_even_when_its_target_was_approved() {
        use std::os::unix::fs::symlink;

        let dir = unique_tmp("matter-approved-target-symlink");
        let library = dir.join("library");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("recording.wav");
        let link = dir.join("recording-link.wav");
        fs::write(&target, b"keep").unwrap();
        approve_recording_in(&library, &target).unwrap();
        symlink(&target, &link).unwrap();
        save_session(&library, "matter-4", &link);

        let result = delete_matter_authorized_in(&library, "matter-4", |_, _| true);

        assert!(result.is_err());
        assert!(link.exists());
        assert!(target.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_recording_reauthorization_requires_the_same_exact_file() {
        let dir = unique_tmp("legacy-reauthorization");
        let library = dir.join("library");
        fs::create_dir_all(&dir).unwrap();
        let expected = dir.join("expected.wav");
        let other = dir.join("other.wav");
        fs::write(&expected, b"expected").unwrap();
        fs::write(&other, b"other").unwrap();

        assert!(reauthorize_recording_in(&library, &expected, &other).is_err());
        let canonical = reauthorize_recording_in(&library, &expected, &expected).unwrap();
        save_session(&library, "legacy", &expected);

        let report = delete_matter_authorized_in(&library, "legacy", |_, _| true).unwrap();
        assert_eq!(report.deleted, vec![canonical.to_string_lossy()]);
        assert!(crate::library::load_session_in(&library, "legacy").is_err());
        assert!(other.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_audio_still_requires_confirmation_before_transcript_deletion() {
        let dir = unique_tmp("missing-audio-confirmation");
        let library = dir.join("library");
        fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("gone.wav");
        fs::write(&audio, b"audio").unwrap();
        approve_recording_in(&library, &audio).unwrap();
        save_session(&library, "missing-audio", &audio);
        fs::remove_file(&audio).unwrap();

        let denied = delete_matter_authorized_in(&library, "missing-audio", |path, exists| {
            assert_eq!(path, Some(audio.as_path()));
            assert!(!exists);
            false
        });
        assert!(denied.is_err());
        assert!(crate::library::load_session_in(&library, "missing-audio").is_ok());

        delete_matter_authorized_in(&library, "missing-audio", |path, exists| {
            assert_eq!(path, Some(audio.as_path()));
            assert!(!exists);
            true
        })
        .unwrap();
        assert!(crate::library::load_session_in(&library, "missing-audio").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
