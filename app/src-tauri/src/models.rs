//! Model-pack trust boundary (PR #3c): the running app's wrapper around the
//! `modelpack` verification crate. This is the security-visible piece firm
//! IT/DPO see — the app enumerates a packs folder, VERIFIES every pack before
//! it is trusted, and REFUSES to import unsigned/tampered/untrusted/too-new
//! packs. Verify + import + display ONLY; the engine/transcribe path is NOT
//! rewired to consume verified packs here (that is PR #3d).
//!
//! A model pack is a DIRECTORY containing `manifest.json` + a single
//! `weights.bin` (matching the `modelpack` crate's one-sha256-per-pack model and
//! the `fixtures/modelpack/cases/*` layout, which double as test packs).
//!
//! Fail closed: any unsigned/tampered/untrusted/too-new/malformed pack is never
//! imported nor listed as verified. The pure `verify_pack_dir` never panics on
//! missing or garbage files — it maps every failure to a human-readable reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use modelpack::{verify_model_pack, PublicKey};
use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

/// DEV/TEST KEY — the production build injects the real vendor key via the
/// `VENDOR_PUBKEY_SPKI_PEM` build-time env var (`option_env!`). This fallback is
/// the TEST SPKI key from `fixtures/modelpack/context.json`, used only in
/// dev/CI builds so the committed fixture packs verify. It must NEVER be the key
/// that signs production packs.
const DEV_TEST_PUBKEY_SPKI_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA1HsQTCcJ2eUma5lmkjVdzCce0p3EaxywplN4xyZcSpU=\n-----END PUBLIC KEY-----\n";

/// The vendor public key this build trusts. Real vendor key when injected at
/// build time, else the dev/test fallback above.
fn vendor_pubkey_pem() -> &'static str {
    option_env!("VENDOR_PUBKEY_SPKI_PEM").unwrap_or(DEV_TEST_PUBKEY_SPKI_PEM)
}

/// Engine versions this build ships, gating `engine_min_version`. Mirrors
/// `fixtures/modelpack/context.json` (`whisper.cpp`, `sherpa-onnx`). NOTE:
/// derive these from the bundled engine crates later rather than hard-coding.
fn engine_versions() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("whisper.cpp".to_string(), "1.7.0".to_string());
    m.insert("sherpa-onnx".to_string(), "1.13.2".to_string());
    m
}

/// The verified/rejected status of a single model pack, surfaced to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct PackStatus {
    /// Pack directory name (stable identifier for the row, even when rejected).
    pub dir: String,
    pub id: String,
    pub version: String,
    pub engine: String,
    pub task: String,
    pub attribution: String,
    pub verified: bool,
    /// Human-readable rejection reason when `verified == false`, else `None`.
    pub reason: Option<String>,
}

impl PackStatus {
    /// A fully-rejected status that could not even surface manifest fields
    /// (e.g. unreadable/garbage `manifest.json`). Fail closed.
    fn rejected(dir: &str, reason: String) -> Self {
        PackStatus {
            dir: dir.to_string(),
            id: String::new(),
            version: String::new(),
            engine: String::new(),
            task: String::new(),
            attribution: String::new(),
            verified: false,
            reason: Some(reason),
        }
    }
}

/// PURE, testable core: verify a single pack directory and map the outcome to a
/// [`PackStatus`]. Reads `manifest.json` (rejecting unreadable/garbage files)
/// and `weights.bin` if present (else empty bytes → hash-mismatch reject), then
/// calls [`modelpack::verify_model_pack`]. Never panics on missing/garbage
/// files; every failure is a human-readable `reason`.
pub fn verify_pack_dir(
    dir: &Path,
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
) -> PackStatus {
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string_lossy().into_owned());

    // manifest.json — must be readable and parse as JSON.
    let manifest_bytes = match std::fs::read(dir.join("manifest.json")) {
        Ok(b) => b,
        Err(e) => {
            return PackStatus::rejected(&dir_name, format!("cannot read manifest.json: {e}"))
        }
    };
    let manifest_value: Value = match serde_json::from_slice(&manifest_bytes) {
        Ok(v) => v,
        Err(e) => {
            return PackStatus::rejected(&dir_name, format!("manifest.json is not valid JSON: {e}"))
        }
    };

    // weights.bin — empty bytes when absent (verification then rejects on the
    // hash mismatch, fail closed).
    let weights = std::fs::read(dir.join("weights.bin")).unwrap_or_default();

    match verify_model_pack(&manifest_value, &weights, pubkey, engine_versions) {
        Ok(verified) => PackStatus {
            dir: dir_name,
            id: verified.manifest.id,
            version: verified.manifest.version,
            engine: verified.manifest.engine,
            task: verified.manifest.task,
            attribution: verified.attribution,
            verified: true,
            reason: None,
        },
        Err(reason) => {
            // Surface manifest fields when they are structurally present so the
            // UI can show which pack was rejected — but NEVER mark it verified.
            let m = manifest_value.as_object();
            let field = |k: &str| {
                m.and_then(|o| o.get(k))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            PackStatus {
                dir: dir_name,
                id: field("id"),
                version: field("version"),
                engine: field("engine"),
                task: field("task"),
                attribution: field("attribution"),
                verified: false,
                reason: Some(reason),
            }
        }
    }
}

/// Resolve the packs directory: `PODCAST_PACKS_DIR` env (dev) else the per-user
/// app-data `packs/` dir. The app-data dir is created on demand.
fn packs_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("PODCAST_PACKS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve app data dir: {e}"))?;
    Ok(base.join("packs"))
}

/// List + verify every pack in the packs dir. Returns an empty list (not an
/// error) when the dir is absent so a fresh install shows "no packs" cleanly.
#[tauri::command]
pub fn list_model_packs(app: tauri::AppHandle) -> Result<Vec<PackStatus>, String> {
    let dir = packs_dir(&app)?;
    let pubkey = PublicKey::SpkiPem(vendor_pubkey_pem());
    let versions = engine_versions();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()), // no packs dir yet → no packs.
    };

    let mut out: Vec<PackStatus> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| verify_pack_dir(&p, &pubkey, &versions))
        .collect();
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(out)
}

/// Verify the selected directory; if verified, copy it into the packs dir and
/// return the verified status. If NOT verified, return the rejection reason and
/// DO NOT install anything (fail closed).
#[tauri::command]
pub fn import_model_pack(app: tauri::AppHandle, src_dir: String) -> Result<PackStatus, String> {
    let src = PathBuf::from(&src_dir);
    let pubkey = PublicKey::SpkiPem(vendor_pubkey_pem());
    let versions = engine_versions();

    let status = verify_pack_dir(&src, &pubkey, &versions);
    if !status.verified {
        return Ok(status); // rejected → surface reason, install nothing.
    }

    // Verified — copy manifest.json + weights.bin into <packs>/<name>/.
    let dir = packs_dir(&app)?;
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "source directory has no name".to_string())?;
    let dest = dir.join(&name);
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("cannot create packs dir {}: {e}", dest.display()))?;
    for file in ["manifest.json", "weights.bin"] {
        let from = src.join(file);
        if from.exists() {
            std::fs::copy(&from, dest.join(file))
                .map_err(|e| format!("cannot copy {file}: {e}"))?;
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = <repo>/app/src-tauri
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("app/src-tauri has a repo-root grandparent")
            .to_path_buf()
    }

    fn fixtures_context() -> (String, BTreeMap<String, String>) {
        let fixtures = repo_root().join("fixtures/modelpack");
        let v: Value =
            serde_json::from_str(&fs::read_to_string(fixtures.join("context.json")).unwrap())
                .unwrap();
        let pem = v["pubkeySpkiPem"].as_str().unwrap().to_string();
        let mut versions = BTreeMap::new();
        for (k, val) in v["engineVersions"].as_object().unwrap() {
            versions.insert(k.clone(), val.as_str().unwrap().to_string());
        }
        (pem, versions)
    }

    /// `verify_pack_dir` over the PR #3a fixture cases agrees with each case's
    /// `expected.json` — the app's folder-reading wrapper reaches the SAME
    /// verified/rejected outcomes as the cross-language conformance suite.
    #[test]
    fn verify_pack_dir_agrees_with_fixtures() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let cases = repo_root().join("fixtures/modelpack/cases");

        let mut names: Vec<String> = fs::read_dir(&cases)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert!(!names.is_empty(), "expected fixture cases");

        for name in &names {
            let dir = cases.join(name);
            let expected: Value =
                serde_json::from_str(&fs::read_to_string(dir.join("expected.json")).unwrap())
                    .unwrap();
            let outcome = expected["outcome"].as_str().unwrap();

            let status = verify_pack_dir(&dir, &pubkey, &versions);

            match outcome {
                "ok" => {
                    assert!(
                        status.verified,
                        "case {name}: expected verified, got reason {:?}",
                        status.reason
                    );
                    assert!(status.reason.is_none());
                    // Verified packs surface their manifest fields.
                    assert!(!status.id.is_empty(), "case {name}: missing id");
                    assert!(!status.attribution.is_empty(), "case {name}: missing attribution");
                }
                "reject" => {
                    assert!(!status.verified, "case {name}: expected reject, got verified");
                    let reason = status.reason.expect("reject must carry a reason");
                    if let Some(needle) = expected["reasonContains"].as_str() {
                        assert!(
                            reason.contains(needle),
                            "case {name}: reason {reason:?} does not contain {needle:?}"
                        );
                    }
                }
                other => panic!("case {name}: unknown expected outcome {other:?}"),
            }
        }
    }

    /// Missing manifest.json fails closed with a reason and never panics.
    #[test]
    fn missing_manifest_fails_closed() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let tmp = std::env::temp_dir().join("podcast-packs-test-empty");
        let _ = fs::create_dir_all(&tmp);
        let status = verify_pack_dir(&tmp, &pubkey, &versions);
        assert!(!status.verified);
        assert!(status.reason.unwrap().contains("manifest.json"));
    }

    /// A valid manifest with a MISSING weights.bin rejects on the hash mismatch
    /// (empty bytes), never crashes — fail closed.
    #[test]
    fn missing_weights_rejects_on_hash() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let tmp = std::env::temp_dir().join("podcast-packs-test-noweights");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let valid_manifest =
            repo_root().join("fixtures/modelpack/cases/valid/manifest.json");
        fs::copy(&valid_manifest, tmp.join("manifest.json")).unwrap();
        let status = verify_pack_dir(&tmp, &pubkey, &versions);
        assert!(!status.verified);
        assert!(status.reason.unwrap().contains("weights hash mismatch"));
    }
}
