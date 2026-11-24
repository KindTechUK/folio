//! Signed/versioned model-pack validation & verification at the Rust trust
//! boundary — a byte-for-byte mirror of `core/src/modelpack.ts` (the SPEC +
//! ORACLE). Verification must run in Rust in the Tauri backend because the TS
//! module relies on `node:crypto`, which does not exist in the webview.
//!
//! The app FAILS CLOSED: any unsigned, tampered, malformed, or unsupported
//! model pack is rejected with a single, clear, human-readable reason. No
//! partial trust, no "best effort" loading.
//!
//! # Canonicalization contract (must match the TS oracle exactly)
//!
//! The bytes that are signed/verified are the compact JSON of the EIGHT known
//! manifest fields, keys in ascending order, the `signature` field excluded,
//! encoded UTF-8:
//!
//! ```text
//! {"attribution":...,"engine":...,"engine_min_version":...,"id":...,
//!  "license":...,"sha256":...,"task":...,"version":...}
//! ```
//!
//! We achieve byte-identity by serializing a private struct whose fields are
//! declared in exactly that sorted order via `serde_json::to_vec` (serde
//! preserves struct field declaration order and emits compact JSON). The shared
//! cross-language conformance fixture is the guardrail (`tests/conformance.rs`).
//!
//! Ed25519 verification uses PureEdDSA, matching Node's `crypto.verify(null, ..)`.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use ed25519_dalek::pkcs8::DecodePublicKey;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Engines this build knows how to drive (mirrors `ENGINES` in the TS).
pub const ENGINES: [&str; 3] = ["whisper.cpp", "sherpa-onnx", "llama.cpp"];

/// Tasks a model pack can perform (mirrors `TASKS` in the TS).
pub const TASKS: [&str; 3] = ["asr", "diarization", "llm"];

/// A validated model-pack manifest. Field NAMES match the TS interface; the
/// declared field ORDER here is the canonical sorted order so that
/// [`canonicalize`] is byte-identical to the TS `canonicalize`.
///
/// `signature` is deliberately LAST and `#[serde(skip)]`-ed so it never appears
/// in the canonical bytes (a signature can't cover itself).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Manifest {
    pub attribution: String,
    pub engine: String,
    pub engine_min_version: String,
    pub id: String,
    pub license: String,
    pub sha256: String,
    pub task: String,
    pub version: String,
    #[serde(skip)]
    pub signature: String,
}

/// A successfully verified pack: the trusted manifest plus its attribution
/// (surfaced to firm IT alongside the loaded model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub manifest: Manifest,
    pub attribution: String,
}

/// The trusted vendor ed25519 public key, accepted either as raw 32 bytes or as
/// an SPKI PEM string (fixtures provide both forms).
pub enum PublicKey<'a> {
    /// Raw 32-byte ed25519 public key.
    Raw(&'a [u8]),
    /// SPKI PEM (`-----BEGIN PUBLIC KEY-----`).
    SpkiPem(&'a str),
}

impl<'a> PublicKey<'a> {
    fn to_verifying_key(&self) -> Result<VerifyingKey, ()> {
        match self {
            PublicKey::Raw(bytes) => {
                let arr: [u8; 32] = (*bytes).try_into().map_err(|_| ())?;
                VerifyingKey::from_bytes(&arr).map_err(|_| ())
            }
            PublicKey::SpkiPem(pem) => VerifyingKey::from_public_key_pem(pem).map_err(|_| ()),
        }
    }
}

// --- semver ---------------------------------------------------------------

/// True iff `s` matches `^\d+\.\d+\.\d+$` (mirrors the TS `SEMVER_RE`).
fn is_semver(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// True iff `s` is exactly 64 lowercase hex chars (mirrors the TS `SHA256_RE`).
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Numeric compare of two `x.y.z` versions (mirrors the TS `compareSemver`).
/// Assumes both are valid `x.y.z` (callers validate first); missing components
/// are treated as 0, matching the TS `?? 0`.
pub fn compare_semver(a: &str, b: &str) -> Ordering {
    let pa: Vec<u64> = a.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    let pb: Vec<u64> = b.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    for i in 0..3 {
        let da = pa.get(i).copied().unwrap_or(0);
        let db = pb.get(i).copied().unwrap_or(0);
        match da.cmp(&db) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

// --- canonicalization -----------------------------------------------------

/// Produce the canonical UTF-8 bytes the vendor signs / we verify (see the
/// module-level canonicalization contract).
pub fn canonicalize(manifest: &Manifest) -> Vec<u8> {
    serde_json::to_vec(manifest).expect("Manifest serializes to JSON infallibly")
}

// --- validation -----------------------------------------------------------

/// Read a field from the JSON object as a non-empty string (TS `isNonEmptyString`).
fn non_empty_string<'a>(m: &'a serde_json::Map<String, Value>, k: &str) -> Option<&'a str> {
    match m.get(k) {
        Some(Value::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Structurally validate an untrusted manifest value. Returns the first failure
/// as a clear, human-readable reason — SAME field/type/semver/sha256 checks IN
/// THE SAME ORDER as the TS `validateManifest`. Does NOT check hash or signature.
pub fn validate_manifest(value: &Value) -> Result<Manifest, String> {
    let m = match value {
        Value::Object(map) => map,
        _ => return Err("manifest is not a JSON object".to_string()),
    };

    // Required non-empty string fields (presence + type), in TS order.
    for field in ["id", "version", "engine_min_version", "license", "attribution", "sha256"] {
        if !m.contains_key(field) {
            return Err(format!("manifest is missing required field \"{field}\""));
        }
        if non_empty_string(m, field).is_none() {
            return Err(format!("manifest field \"{field}\" must be a non-empty string"));
        }
    }

    // signature: must be present and a string (an EMPTY signature is a valid
    // shape that fails closed later as "unsigned", not a structural error).
    if !m.contains_key("signature") {
        return Err("manifest is missing required field \"signature\"".to_string());
    }
    if !matches!(m.get("signature"), Some(Value::String(_))) {
        return Err("manifest field \"signature\" must be a string".to_string());
    }

    let engine = match non_empty_string(m, "engine") {
        Some(s) => s,
        None => return Err("manifest is missing required field \"engine\"".to_string()),
    };
    if !ENGINES.contains(&engine) {
        return Err(format!(
            "manifest engine \"{engine}\" is not one of: {}",
            ENGINES.join(", ")
        ));
    }

    let task = match non_empty_string(m, "task") {
        Some(s) => s,
        None => return Err("manifest is missing required field \"task\"".to_string()),
    };
    if !TASKS.contains(&task) {
        return Err(format!(
            "manifest task \"{task}\" is not one of: {}",
            TASKS.join(", ")
        ));
    }

    let version = m.get("version").and_then(Value::as_str).unwrap_or("");
    if !is_semver(version) {
        return Err(format!(
            "manifest version \"{version}\" is not valid semver (expected x.y.z)"
        ));
    }
    let engine_min_version = m.get("engine_min_version").and_then(Value::as_str).unwrap_or("");
    if !is_semver(engine_min_version) {
        return Err(format!(
            "manifest engine_min_version \"{engine_min_version}\" is not valid semver (expected x.y.z)"
        ));
    }

    let sha256 = m.get("sha256").and_then(Value::as_str).unwrap_or("");
    if !is_sha256_hex(sha256) {
        return Err("manifest sha256 must be 64 lowercase hex characters".to_string());
    }

    let signature = m.get("signature").and_then(Value::as_str).unwrap_or("").to_string();
    Ok(Manifest {
        attribution: non_empty_string(m, "attribution").unwrap().to_string(),
        engine: engine.to_string(),
        engine_min_version: engine_min_version.to_string(),
        id: non_empty_string(m, "id").unwrap().to_string(),
        license: non_empty_string(m, "license").unwrap().to_string(),
        sha256: sha256.to_string(),
        task: task.to_string(),
        version: version.to_string(),
        signature,
    })
}

// --- verification ----------------------------------------------------------

/// Constant-time equality for two hex digest strings of equal expected length
/// (mirrors the TS `timingSafeHexEqual`).
fn timing_safe_hex_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Fully verify a model pack, failing closed at the first problem. Steps, IN
/// ORDER (mirrors the TS `verifyModelPack`):
///   1. [`validate_manifest`] passes;
///   2. `sha256(weights)` hex == `manifest.sha256` (constant-time compare);
///   3. base64 signature is a valid vendor ed25519 (PureEdDSA) signature over
///      [`canonicalize`] of the manifest;
///   4. `manifest.engine_min_version <=` this build's engine version.
///
/// Empty/garbage signatures fall into the "unsigned or untrusted" reject; this
/// function never panics on attacker-controlled input.
pub fn verify_model_pack(
    manifest_value: &Value,
    weights: &[u8],
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
) -> Result<Verified, String> {
    verify_model_pack_reader(manifest_value, weights, pubkey, engine_versions)
}

/// Bytes read per step when hashing weights. Packs can be several GB (a Q4
/// 4B LLM is ~2.4 GB), so weights are never held in memory whole.
pub const HASH_CHUNK_BYTES: usize = 1 << 20;

/// Lowercase-hex SHA-256 of everything `reader` yields, read
/// [`HASH_CHUNK_BYTES`] at a time.
pub fn sha256_hex_reader<R: Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK_BYTES];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Lowercase-hex SHA-256 of the file at `path`, streamed in chunks.
pub fn sha256_hex_file(path: &Path) -> std::io::Result<String> {
    sha256_hex_reader(std::fs::File::open(path)?)
}

/// [`verify_model_pack`] over a weights STREAM (e.g. an open `weights.bin`),
/// hashed in [`HASH_CHUNK_BYTES`] chunks so multi-GB packs never sit in RAM.
/// Same steps, same order, same reasons; a read error mid-stream fails closed.
pub fn verify_model_pack_reader<R: Read>(
    manifest_value: &Value,
    weights: R,
    pubkey: &PublicKey<'_>,
    engine_versions: &BTreeMap<String, String>,
) -> Result<Verified, String> {
    // 1. structural validation
    let manifest = validate_manifest(manifest_value)?;

    // 2. weights integrity
    let actual = sha256_hex_reader(weights)
        .map_err(|e| format!("cannot read weights (tampered or corrupt): {e}"))?;
    if !timing_safe_hex_equal(&actual, &manifest.sha256) {
        return Err("weights hash mismatch (tampered or corrupt)".to_string());
    }

    // 3. signature over the canonical manifest
    if !signature_valid(&manifest, pubkey) {
        return Err("signature invalid (unsigned or untrusted pack)".to_string());
    }

    // 4. engine version gate
    let available = match engine_versions.get(&manifest.engine) {
        Some(v) => v,
        None => {
            return Err(format!(
                "engine not supported by this build (no \"{}\" engine available)",
                manifest.engine
            ))
        }
    };
    if !is_semver(available) {
        return Err(format!(
            "installed engine version \"{available}\" for \"{}\" is not valid semver",
            manifest.engine
        ));
    }
    if compare_semver(&manifest.engine_min_version, available) == Ordering::Greater {
        return Err(format!(
            "model pack requires {} >= {}, but this build has {}; update the app to load this pack",
            manifest.engine, manifest.engine_min_version, available
        ));
    }

    let attribution = manifest.attribution.clone();
    Ok(Verified { manifest, attribution })
}

/// Verify the manifest's base64 ed25519 signature over its canonical bytes.
/// Returns false (never panics) for empty/garbage signatures or untrusted keys.
fn signature_valid(manifest: &Manifest, pubkey: &PublicKey<'_>) -> bool {
    let sig_bytes = match BASE64_STANDARD.decode(manifest.signature.as_bytes()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    if sig_bytes.is_empty() {
        return false;
    }
    let sig_arr: [u8; 64] = match sig_bytes.as_slice().try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let signature = Signature::from_bytes(&sig_arr);
    let key = match pubkey.to_verifying_key() {
        Ok(k) => k,
        Err(_) => return false,
    };
    let data = canonicalize(manifest);
    key.verify(&data, &signature).is_ok()
}

/// Lowercase hex encode (matches Node's `digest("hex")`).
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "id": "acme/whisper-base-en",
            "version": "1.2.3",
            "engine": "whisper.cpp",
            "engine_min_version": "1.5.0",
            "task": "asr",
            "license": "MIT",
            "attribution": "Acme Speech Labs",
            "sha256": "a".repeat(64),
            "signature": "AA=="
        })
    }

    #[test]
    fn canonicalize_is_sorted_compact_json_without_signature() {
        let m = validate_manifest(&base()).unwrap();
        let bytes = canonicalize(&m);
        let s = String::from_utf8(bytes).unwrap();
        // keys ascending, signature absent, compact (no whitespace).
        assert_eq!(
            s,
            r#"{"attribution":"Acme Speech Labs","engine":"whisper.cpp","engine_min_version":"1.5.0","id":"acme/whisper-base-en","license":"MIT","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","task":"asr","version":"1.2.3"}"#
        );
        assert!(!s.contains("signature"));
    }

    #[test]
    fn semver_compare_and_validate() {
        assert_eq!(compare_semver("1.5.0", "1.7.0"), Ordering::Less);
        assert_eq!(compare_semver("2.0.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_semver("1.2.3", "1.2.3"), Ordering::Equal);

        let mut m = base();
        m["version"] = json!("1.2.x");
        assert!(validate_manifest(&m).unwrap_err().contains("not valid semver"));
    }

    /// A file longer than one hash chunk hashes to its SHA-256 (the literal was
    /// computed independently with `shasum -a 256` over the same bytes).
    #[test]
    fn streamed_hash_of_a_file_larger_than_one_chunk() {
        let len = (1usize << 20) + 4097;
        let bytes: Vec<u8> = (0..len).map(|i| ((i * 31 + 7) & 0xff) as u8).collect();
        let path = std::env::temp_dir().join(format!("modelpack-chunk-{}.bin", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();

        let digest = sha256_hex_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(len > HASH_CHUNK_BYTES);
        assert_eq!(
            digest,
            "b0fee64e0701324ea24d718618eb8e39ec084ac78a92948424f1b23b16e7804a"
        );
    }

    #[test]
    fn non_object_manifest_is_rejected() {
        assert_eq!(
            validate_manifest(&json!([])).unwrap_err(),
            "manifest is not a JSON object"
        );
    }

    #[test]
    fn empty_and_garbage_signatures_fail_closed() {
        let pubkey_hex =
            include_str!("../../fixtures/modelpack/keys/test-public.raw.hex").trim();
        let pubkey: Vec<u8> = (0..pubkey_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&pubkey_hex[i..i + 2], 16).unwrap())
            .collect();

        let garbage = [
            String::new(),                                // empty
            "!!!not-base64!!!".to_string(),               // not base64
            "A".repeat(84),                               // 63 bytes: wrong length
            format!("{}==", "A".repeat(86)),              // 64 zero bytes: wrong signature
        ];
        for sig in garbage {
            let mut m = base();
            // SHA-256 of the empty byte string.
            m["sha256"] =
                json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
            m["signature"] = json!(sig);
            let res = verify_model_pack(&m, b"", &PublicKey::Raw(&pubkey), &BTreeMap::new());
            assert!(
                res.unwrap_err().contains("unsigned or untrusted"),
                "signature {sig:?} must fail closed"
            );
        }
    }
}
