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
use std::path::{Component, Path, PathBuf};

use modelpack::{compare_semver, verify_model_pack_reader, PublicKey};
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
    m.insert("sherpa-onnx".to_string(), "2.0.1".to_string());
    // llama.cpp v0.6.0 (commit d812350), as vendored by llama-cpp-sys-2 =0.1.161
    // (spike S5). No llama.cpp is linked into the app yet.
    m.insert("llama.cpp".to_string(), "0.6.0".to_string());
    m
}

/// The manifest `task` of a language-model pack (a GGUF for llama.cpp).
const LLM_TASK: &str = "llm";

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
/// missing `weights.bin` reads as empty bytes (→ hash-mismatch reject). The
/// weights are hashed as a stream, never read into memory whole (LLM packs are
/// several GB). Never panics on missing/garbage files; every failure is a
/// human-readable `reason`.
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

    let outcome = match std::fs::File::open(dir.join("weights.bin")) {
        Ok(weights) => verify_model_pack_reader(&manifest_value, weights, pubkey, engine_versions),
        Err(_) => verify_model_pack_reader(&manifest_value, std::io::empty(), pubkey, engine_versions),
    };

    match outcome {
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

/// Extract every file entry of the zip at `weights` into `dest`, creating
/// parent dirs and skipping entries whose path would escape `dest`. Entries are
/// streamed to disk, so a multi-GB GGUF never sits in memory.
fn extract_zip_into(weights: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(weights)
        .map_err(|e| format!("opening {}: {e}", weights.display()))?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
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
        let mut out = std::fs::File::create(&out_path)
            .map_err(|e| format!("writing {}: {e}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("extracting zip entry {rel_str}: {e}"))?;
    }
    Ok(())
}

/// True iff `path` is a readable zip archive.
fn is_zip_file(path: &Path) -> bool {
    std::fs::File::open(path)
        .map(|f| zip::ZipArchive::new(std::io::BufReader::new(f)).is_ok())
        .unwrap_or(false)
}

/// The `sha256` string of the (already verified) manifest in `dir`.
fn manifest_sha256(dir: &Path) -> Result<String, String> {
    let bytes = std::fs::read(dir.join("manifest.json"))
        .map_err(|e| format!("reading {}: {e}", dir.join("manifest.json").display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("parsing {}: {e}", dir.join("manifest.json").display()))?;
    value
        .get("sha256")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("manifest in {} has no sha256", dir.display()))
}

/// Verify every pack directory in `packs_dir`, returning the verified ones.
fn verified_packs(
    packs_dir: &Path,
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
) -> Vec<(PathBuf, PackStatus)> {
    let mut out: Vec<(PathBuf, PackStatus)> = match std::fs::read_dir(packs_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .map(|p| {
                let status = verify_pack_dir(&p, pubkey, engine_versions);
                (p, status)
            })
            .filter(|(_, status)| status.verified)
            .collect(),
        Err(_) => Vec::new(),
    };
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Verified-pack resolver. Enumerates `packs_dir`, VERIFIES every
/// pack, extracts ONLY verified speech packs' `weights.bin` zips into a content-
/// addressed cache under `cache_base`, and returns the cache dir IFF it provides
/// every model file the engine requires for `(asr_model, diarize)`. Fails closed
/// at every step: no verified pack, or any missing required file, is an `Err`.
/// Language-model packs (`task: "llm"`) are left out; see [`resolve_verified_llm_in`].
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
    let verified_dirs: Vec<PathBuf> = verified_packs(packs_dir, pubkey, engine_versions)
        .into_iter()
        .filter(|(_, status)| status.task != LLM_TASK)
        .map(|(dir, _)| dir)
        .collect();
    if verified_dirs.is_empty() {
        return Err(format!(
            "no verified model pack found in {}",
            packs_dir.display()
        ));
    }

    let mut sha_list: Vec<String> = Vec::new();
    for dir in &verified_dirs {
        sha_list.push(manifest_sha256(dir)?);
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
            let weights = dir.join("weights.bin");
            // A non-zip weights.bin contributes no files — skip rather than abort.
            if !is_zip_file(&weights) {
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

/// Verified LLM-pack resolver. Enumerates `packs_dir`, VERIFIES every pack, and
/// takes the verified `task: "llm"` pack with the highest version (ties: last
/// directory name). Its `weights.bin` zip is streamed into
/// `cache_base/verified-models/llm-<sha256>/` (skipped when the `.complete`
/// marker is there) and the path of the single `.gguf` file inside is returned.
/// Fails closed: no verified LLM pack, a pack that is not a zip, or a pack
/// without exactly one top-level `.gguf` is an `Err`. Speech packs are never
/// returned, whatever files they hold.
#[allow(dead_code)] // Spike S5: not called by the app yet; P2 (local notes) will use it.
pub fn resolve_verified_llm_in(
    packs_dir: &Path,
    cache_base: &Path,
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let mut llm_packs: Vec<(PathBuf, PackStatus)> =
        verified_packs(packs_dir, pubkey, engine_versions)
            .into_iter()
            .filter(|(_, status)| status.task == LLM_TASK)
            .collect();
    llm_packs.sort_by(|a, b| compare_semver(&a.1.version, &b.1.version).then(a.0.cmp(&b.0)));
    let Some((dir, status)) = llm_packs.pop() else {
        return Err(format!(
            "no verified LLM model pack found in {}",
            packs_dir.display()
        ));
    };

    let dest = cache_base
        .join("verified-models")
        .join(format!("llm-{}", manifest_sha256(&dir)?));
    let complete_marker = dest.join(".complete");
    if !complete_marker.exists() {
        if dest.exists() {
            std::fs::remove_dir_all(&dest)
                .map_err(|e| format!("clearing partial cache {}: {e}", dest.display()))?;
        }
        std::fs::create_dir_all(&dest)
            .map_err(|e| format!("creating cache dir {}: {e}", dest.display()))?;
        extract_zip_into(&dir.join("weights.bin"), &dest)?;
        std::fs::write(&complete_marker, b"")
            .map_err(|e| format!("writing completion marker {}: {e}", complete_marker.display()))?;
    }

    let ggufs: Vec<PathBuf> = std::fs::read_dir(&dest)
        .map_err(|e| format!("reading cache dir {}: {e}", dest.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
        })
        .collect();
    match ggufs.as_slice() {
        [gguf] => Ok(gguf.clone()),
        [] => Err(format!("verified LLM pack {} holds no .gguf file", status.id)),
        _ => Err(format!(
            "verified LLM pack {} holds more than one .gguf file",
            status.id
        )),
    }
}

/// AppHandle wrapper around [`resolve_verified_llm_in`].
#[allow(dead_code)] // Spike S5: not called by the app yet; P2 (local notes) will use it.
pub fn resolve_verified_llm(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let packs = packs_dir(app)?;
    let cache_base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve app data dir: {e}"))?;
    let pubkey = PublicKey::SpkiPem(vendor_pubkey_pem());
    let versions = engine_versions();
    resolve_verified_llm_in(&packs, &cache_base, &pubkey, &versions)
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

    /// Copy the committed fixture pack `name` into `packs/<name>/`.
    fn copy_fixture_pack(name: &str, packs: &Path) -> PathBuf {
        let src = repo_root().join("fixtures/modelpack/packs").join(name);
        let dest = packs.join(name);
        fs::create_dir_all(&dest).unwrap();
        for file in ["manifest.json", "weights.bin"] {
            fs::copy(src.join(file), dest.join(file)).unwrap();
        }
        dest
    }

    /// A signed `task: "llm"` pack resolves to the GGUF inside it, even with a
    /// speech pack alongside.
    #[test]
    fn resolve_verified_llm_in_returns_the_gguf_of_a_signed_llm_pack() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let packs = repo_root().join("fixtures/modelpack/packs");
        let cache_base = unique_tmp("llm-ok");

        let gguf = resolve_verified_llm_in(&packs, &cache_base, &pubkey, &versions)
            .expect("the committed llm fixture pack must resolve");

        assert_eq!(gguf.file_name().unwrap(), "tiny.gguf");
        assert_eq!(fs::read(&gguf).unwrap(), b"GGUF fixture: not a real model\n");

        let _ = fs::remove_dir_all(&cache_base);
    }

    /// A pack whose weights.bin was changed after signing is never resolved.
    #[test]
    fn resolve_verified_llm_in_rejects_tampered_weights() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let packs = unique_tmp("llm-tampered-packs");
        let pack = copy_fixture_pack("llm-tiny", &packs);
        let mut weights = fs::read(pack.join("weights.bin")).unwrap();
        let last = weights.len() - 1;
        weights[last] ^= 0x01;
        fs::write(pack.join("weights.bin"), &weights).unwrap();
        let cache_base = unique_tmp("llm-tampered-cache");

        let err = resolve_verified_llm_in(&packs, &cache_base, &pubkey, &versions)
            .expect_err("a tampered llm pack must not be trusted");

        assert!(err.contains("no verified LLM model pack found"), "got: {err}");
        let _ = fs::remove_dir_all(&packs);
        let _ = fs::remove_dir_all(&cache_base);
    }

    /// A verified speech pack is never handed out as a language model.
    #[test]
    fn resolve_verified_llm_in_never_returns_an_asr_pack() {
        let (pem, versions) = fixtures_context();
        let pubkey = PublicKey::SpkiPem(&pem);
        let packs = unique_tmp("llm-asr-only-packs");
        copy_fixture_pack("asr-small-en", &packs);
        let cache_base = unique_tmp("llm-asr-only-cache");

        let err = resolve_verified_llm_in(&packs, &cache_base, &pubkey, &versions)
            .expect_err("an asr pack is not an llm pack");

        assert!(err.contains("no verified LLM model pack found"), "got: {err}");
        let _ = fs::remove_dir_all(&packs);
        let _ = fs::remove_dir_all(&cache_base);
    }
}
