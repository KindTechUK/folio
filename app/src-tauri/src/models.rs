//! Model-pack trust boundary: the app's wrapper around the `modelpack`
//! verification crate. The app enumerates a packs folder, VERIFIES every pack
//! before it is trusted, REFUSES to import unsigned/tampered/untrusted/too-new
//! packs, and resolves verified packs into the model dir the transcribe path
//! uses.
//!
//! A model pack is a DIRECTORY containing `manifest.json` + a single
//! `weights.bin` (matching the `modelpack` crate's one-sha256-per-pack model and
//! the `fixtures/modelpack/cases/*` layout, which double as test packs).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use modelpack::{verify_model_pack, PublicKey};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::Manager;

/// Committed dev/test SPKI key; never the production signing key (release
/// builds reject it, see `vendor_pubkey_pem`).
const DEV_TEST_PUBKEY_SPKI_PEM: &str =
    include_str!("../../../fixtures/modelpack/keys/test-public.spki.pem");

/// The vendor public key this build trusts. `build.rs` makes a release build
/// fail when the value is missing, empty, or equal to the committed test key;
/// the fallback therefore exists only for debug/test builds.
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

/// Verify a single pack directory and map the outcome to a [`PackStatus`]. A
/// missing `weights.bin` reads as empty bytes (→ hash-mismatch reject). Never
/// panics on missing/garbage files; every failure is a human-readable `reason`.
pub fn verify_pack_dir(
    dir: &Path,
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
) -> PackStatus {
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string_lossy().into_owned());

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

/// Resolve the packs directory: `KINDTECH_PACKS_DIR` env (dev) else the per-user
/// app-data `packs/` dir. The app-data dir is created on demand.
fn packs_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("KINDTECH_PACKS_DIR") {
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

// --- verified-pack resolver -----------------------------------------------

/// Lowercase-hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// True iff `name` (a zip entry path) stays within the extraction root: no
/// absolute paths, no `..` components, no root/prefix components. Defends
/// against zip path-traversal ("zip slip") regardless of `ZipArchive::extract`'s
/// own sanitization.
fn zip_entry_is_safe(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let p = Path::new(name);
    p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Extract every file entry of the zip `weights` into `dest`, creating parent
/// dirs and skipping entries whose path would escape `dest`.
fn extract_zip_into(weights: &[u8], dest: &Path) -> Result<(), String> {
    let reader = std::io::Cursor::new(weights);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| format!("weights.bin is not a valid zip: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("reading zip entry {i}: {e}"))?;
        // enclosed_name() is None for unsafe paths — skip those.
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => {
                continue;
            }
        };
        let rel_str = rel.to_string_lossy();
        if !zip_entry_is_safe(&rel_str) {
            continue; // belt-and-braces traversal guard
        }
        let out_path = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("creating dir {}: {e}", out_path.display()))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("creating dir {}: {e}", parent.display()))?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("reading zip entry {rel_str}: {e}"))?;
        std::fs::write(&out_path, &buf)
            .map_err(|e| format!("writing {}: {e}", out_path.display()))?;
    }
    Ok(())
}

/// Verified-pack resolver. Enumerates `packs_dir`, VERIFIES every
/// pack, extracts ONLY verified packs' `weights.bin` zips into a content-
/// addressed cache under `cache_base`, and returns the cache dir IFF it provides
/// every model file the engine requires for `(asr_model, diarize)`. Fails closed
/// at every step: no verified pack, or any missing required file, is an `Err`.
///
/// The cache dir is keyed by the SHA-256 of the verified packs' `sha256` manifest
/// strings (sorted ascending, `\n`-joined), so the same verified set always maps
/// to the same dir and re-extraction is skipped via a `.complete` marker.
pub fn resolve_verified_models_dir_in(
    packs_dir: &Path,
    cache_base: &Path,
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
    asr_model: engine::AsrModel,
    diarize: bool,
) -> Result<PathBuf, String> {
    let mut verified_dirs: Vec<PathBuf> = match std::fs::read_dir(packs_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| verify_pack_dir(p, pubkey, engine_versions).verified)
            .collect(),
        Err(_) => Vec::new(),
    };
    verified_dirs.sort();
    if verified_dirs.is_empty() {
        return Err(format!(
            "no verified model pack found in {}",
            packs_dir.display()
        ));
    }

    let mut sha_list: Vec<String> = Vec::new();
    for dir in &verified_dirs {
        let bytes = std::fs::read(dir.join("manifest.json"))
            .map_err(|e| format!("reading {}: {e}", dir.join("manifest.json").display()))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("parsing {}: {e}", dir.join("manifest.json").display()))?;
        let sha = value
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("manifest in {} has no sha256", dir.display()))?
            .to_string();
        sha_list.push(sha);
    }
    sha_list.sort();
    let combined_key = sha256_hex(sha_list.join("\n").as_bytes());

    let composed = cache_base.join("verified-models").join(&combined_key);
    let complete_marker = composed.join(".complete");
    if !complete_marker.exists() {
        // Fresh extraction: clear any partial dir, recreate, extract each
        // verified pack's zip-valid weights.bin (skip non-zip packs).
        if composed.exists() {
            std::fs::remove_dir_all(&composed)
                .map_err(|e| format!("clearing partial cache {}: {e}", composed.display()))?;
        }
        std::fs::create_dir_all(&composed)
            .map_err(|e| format!("creating cache dir {}: {e}", composed.display()))?;
        for dir in &verified_dirs {
            let weights = std::fs::read(dir.join("weights.bin")).unwrap_or_default();
            // A non-zip weights.bin contributes no files — skip rather than abort.
            if zip::ZipArchive::new(std::io::Cursor::new(&weights)).is_err() {
                continue;
            }
            extract_zip_into(&weights, &composed)?;
        }
        std::fs::write(&complete_marker, b"")
            .map_err(|e| format!("writing completion marker {}: {e}", complete_marker.display()))?;
    }

    // Fail closed unless EVERY required file is present.
    let missing: Vec<String> = engine::required_model_files(asr_model, diarize)
        .into_iter()
        .filter(|rel| !composed.join(rel).exists())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "no verified pack provides the required model file(s): {missing:?}"
        ));
    }
    Ok(composed)
}

/// AppHandle wrapper around [`resolve_verified_models_dir_in`].
pub fn resolve_verified_models_dir(
    app: &tauri::AppHandle,
    asr_model: engine::AsrModel,
    diarize: bool,
) -> Result<PathBuf, String> {
    let packs = packs_dir(app)?;
    let cache_base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve app data dir: {e}"))?;
    let pubkey = PublicKey::SpkiPem(vendor_pubkey_pem());
    let versions = engine_versions();
    resolve_verified_models_dir_in(&packs, &cache_base, &pubkey, &versions, asr_model, diarize)
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

    /// `verify_pack_dir` over the fixture cases agrees with each case's
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

    fn unique_tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "podcast-resolver-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// POSITIVE: the committed `asr-small-en` fixture pack verifies, extracts,
    /// and provides every file `engine::required_model_files(SmallEn, false)`
    /// needs. A second call reuses the `.complete` marker (idempotent).
    #[test]
    fn resolve_verified_models_dir_in_extracts_committed_fixture_pack() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let packs = repo_root().join("fixtures/modelpack/packs");
        let cache_base = unique_tmp("ok");
        let _ = fs::remove_dir_all(&cache_base);

        let dir = resolve_verified_models_dir_in(
            &packs,
            &cache_base,
            &pubkey,
            &versions,
            engine::AsrModel::SmallEn,
            false,
        )
        .expect("committed verified pack must resolve");

        for rel in engine::required_model_files(engine::AsrModel::SmallEn, false) {
            assert!(
                dir.join(&rel).exists(),
                "required file {rel} missing under {}",
                dir.display()
            );
        }
        assert!(dir.join(".complete").exists(), ".complete marker must exist");

        // Second call is idempotent: same dir, reuses the marker.
        let dir2 = resolve_verified_models_dir_in(
            &packs,
            &cache_base,
            &pubkey,
            &versions,
            engine::AsrModel::SmallEn,
            false,
        )
        .expect("second resolve must succeed");
        assert_eq!(dir, dir2, "resolver must be deterministic / idempotent");

        let _ = fs::remove_dir_all(&cache_base);
    }

    /// FAIL-CLOSED: a packs dir holding only the `unsigned` fixture case (which
    /// the verifier rejects) yields no verified pack → `Err`.
    #[test]
    fn resolve_verified_models_dir_in_fails_closed_when_no_verified_pack() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);

        let packs = unique_tmp("unsigned-packs");
        let _ = fs::remove_dir_all(&packs);
        let dest = packs.join("unsigned");
        fs::create_dir_all(&dest).unwrap();
        let src = repo_root().join("fixtures/modelpack/cases/unsigned");
        for file in ["manifest.json", "weights.bin"] {
            let from = src.join(file);
            if from.exists() {
                fs::copy(&from, dest.join(file)).unwrap();
            }
        }
        let cache_base = unique_tmp("unsigned-cache");

        let err = resolve_verified_models_dir_in(
            &packs,
            &cache_base,
            &pubkey,
            &versions,
            engine::AsrModel::SmallEn,
            false,
        )
        .expect_err("an unsigned pack must not be trusted");
        assert!(
            err.contains("no verified model pack found"),
            "expected no-verified-pack error, got: {err}"
        );

        let _ = fs::remove_dir_all(&packs);
        let _ = fs::remove_dir_all(&cache_base);
    }

    /// DIARIZE-MISSING: the asr-only fixture pack provides no seg/emb model, so
    /// resolving with `diarize=true` fails closed naming the missing file(s).
    #[test]
    fn resolve_verified_models_dir_in_fails_closed_when_diarization_models_missing() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let packs = repo_root().join("fixtures/modelpack/packs");
        let cache_base = unique_tmp("diarize");
        let _ = fs::remove_dir_all(&cache_base);

        let err = resolve_verified_models_dir_in(
            &packs,
            &cache_base,
            &pubkey,
            &versions,
            engine::AsrModel::SmallEn,
            true,
        )
        .expect_err("asr-only pack must not satisfy diarization");
        assert!(
            err.contains("required model file") && err.contains("pyannote-segmentation"),
            "expected missing seg/emb error, got: {err}"
        );

        let _ = fs::remove_dir_all(&cache_base);
    }
}
