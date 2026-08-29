//! Data-at-rest helpers (PR-A): volume-encryption status + one-click
//! "delete matter".
//!
//! Both features exist so a firm running client-confidential matters can (a) be
//! warned when the storage volume is NOT confirmed encrypted (BitLocker on
//! Windows) and (b) wipe a matter's only on-disk artifact — the source audio —
//! in one click. The engine writes no temp files, so the picked audio path is
//! the entire footprint to remove.
//!
//! Everything is network-free. The PURE cores — [`parse_manage_bde_status`]
//! and [`delete_matter_authorized_in`] — are unit-tested and never panic; the
//! `#[tauri::command]` wrappers resolve platform paths and native confirmation.
//! Encryption checks fail SOFT (`Unknown`, never an app error).

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;
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

/// PURE, testable parser over Windows `manage-bde -status <drive>` text. Applies
/// case-insensitive heuristics and NEVER panics on garbage/empty input.
///
/// Encrypted when the text shows any of:
///   - `Protection Status: Protection On`
///   - `Conversion Status: Fully Encrypted`
///   - `Percentage Encrypted: 100%` / `100.0%`
///
/// Unencrypted when it shows any of:
///   - `Fully Decrypted`
///   - `Percentage Encrypted: 0%` / `0.0%`
///   - `Protection Status: Protection Off`
///
/// Otherwise [`EncryptionState::Unknown`]. A volume is reported as encrypted
/// only when conversion is complete AND BitLocker protection is on; suspended
/// protection must never inherit a green status from the conversion fields.
///
/// Consumed by the Windows `manage-bde` worker and the unit tests; allow
/// dead-code on non-Windows dev builds where only the tests exercise it.
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

/// PURE, testable delete helper. For each path: if it exists, remove the file
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

/// Delete the audio path recorded by a backend-owned saved session, but only
/// when its canonical target is within one of the canonical allowed roots.
/// This prevents a compromised renderer from turning the command into an
/// arbitrary `remove_file` primitive. Symlinks that escape a root are rejected.
pub fn delete_matter_authorized_in<F>(
    library_dir: &Path,
    id: &str,
    allowed_roots: &[PathBuf],
    authorize: F,
) -> Result<DeleteReport, String>
where
    F: FnOnce(&Path) -> bool,
{
    let loaded = crate::library::load_session_in(library_dir, id)?;
    let audio_path = loaded
        .session
        .get("audioPath")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if audio_path.is_empty() {
        return Ok(DeleteReport {
            deleted: Vec::new(),
            failed: Vec::new(),
        });
    }

    let audio = Path::new(audio_path);
    if !audio.exists() {
        return Ok(DeleteReport {
            deleted: vec![audio_path.to_string()],
            failed: Vec::new(),
        });
    }

    let canonical_audio = audio
        .canonicalize()
        .map_err(|e| format!("cannot resolve saved audio path {audio_path:?}: {e}"))?;
    let inside_allowed_root = allowed_roots.iter().any(|root| {
        root.canonicalize()
            .map(|canonical_root| canonical_audio.starts_with(canonical_root))
            .unwrap_or(false)
    });
    if !inside_allowed_root {
        return Err(format!(
            "refusing to delete saved audio outside approved user-media folders: {}",
            canonical_audio.display()
        ));
    }
    if !canonical_audio.is_file() {
        return Err(format!(
            "refusing to delete a non-file matter path: {}",
            canonical_audio.display()
        ));
    }
    if !authorize(&canonical_audio) {
        return Err("source-audio deletion was cancelled".to_string());
    }

    Ok(delete_paths(&[audio_path.to_string()]))
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

/// Delete the source audio recorded by a saved session. The renderer supplies
/// only the session id; the backend resolves the stored path and permits
/// deletion only inside standard user audio/document/desktop/download roots.
/// A native backend-owned confirmation dialog displays the canonical path and
/// must authorize the deletion; renderer-side confirmation is not trusted.
#[tauri::command]
pub async fn delete_matter(app: tauri::AppHandle, id: String) -> Result<DeleteReport, String> {
    let library = crate::library::library_dir(&app)?;
    let resolver = app.path();
    let allowed_roots: Vec<PathBuf> = [
        resolver.audio_dir(),
        resolver.document_dir(),
        resolver.desktop_dir(),
        resolver.download_dir(),
    ]
    .into_iter()
    .filter_map(Result::ok)
    .collect();
    tauri::async_runtime::spawn_blocking(move || {
        delete_matter_authorized_in(&library, &id, &allowed_roots, |audio_path| {
            app.dialog()
                .message(format!(
                    "Permanently delete this source audio file?\n\n{}",
                    audio_path.display()
                ))
                .title("Delete source audio")
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
        // A directory exists() == true but remove_file() errors → failed.
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
    fn delete_matter_removes_only_saved_audio_inside_an_allowed_root() {
        let dir = unique_tmp("matter-allowed");
        let library = dir.join("library");
        let audio_root = dir.join("audio");
        fs::create_dir_all(&audio_root).unwrap();
        let audio = audio_root.join("recording.wav");
        fs::write(&audio, b"audio").unwrap();
        save_session(&library, "matter-1", &audio);

        let report =
            delete_matter_authorized_in(&library, "matter-1", &[audio_root], |_| true).unwrap();

        assert_eq!(report.deleted, vec![audio.to_string_lossy()]);
        assert!(report.failed.is_empty());
        assert!(!audio.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_matter_rejects_saved_audio_outside_allowed_roots() {
        let dir = unique_tmp("matter-outside");
        let library = dir.join("library");
        let audio_root = dir.join("audio");
        let outside_root = dir.join("outside");
        fs::create_dir_all(&audio_root).unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        let outside = outside_root.join("unrelated.txt");
        fs::write(&outside, b"keep").unwrap();
        save_session(&library, "matter-2", &outside);

        let result = delete_matter_authorized_in(&library, "matter-2", &[audio_root], |_| true);

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
        save_session(&library, "matter-denied", &audio);

        let result = delete_matter_authorized_in(
            &library,
            "matter-denied",
            &[audio_root],
            |_| false,
        );

        assert!(result.is_err());
        assert!(audio.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn delete_matter_rejects_symlink_that_escapes_allowed_root() {
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
        save_session(&library, "matter-3", &link);

        let result = delete_matter_authorized_in(&library, "matter-3", &[audio_root], |_| true);

        assert!(result.is_err());
        assert!(link.exists());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
