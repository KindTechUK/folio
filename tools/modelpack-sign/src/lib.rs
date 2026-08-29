//! Vendor-side producer logic for creating signed model packs.
//!
//! This crate is the PRODUCER counterpart to the `modelpack` verifier crate. It
//! reuses `modelpack::{Manifest, canonicalize, validate_manifest,
//! verify_model_pack}` directly so the signing pipeline can never drift from the
//! verifier: whatever bytes we sign are exactly the bytes the app re-canonicalizes
//! and verifies.
//!
//! Logic lives here (not in `main.rs`) so it is unit-testable.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::spki::EncodePublicKey;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::{Signer, SigningKey};
use modelpack::{Manifest, PublicKey};
use sha2::{Digest, Sha256};

/// Engine versions this build's self-check pretends to bundle. Used only for the
/// post-pack self-verification (the producer must prove the pack it just wrote
/// passes the real verifier).
pub fn self_check_engine_versions() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("whisper.cpp".to_string(), "1.7.0".to_string());
    m.insert("sherpa-onnx".to_string(), "1.13.2".to_string());
    m
}

/// Generate a fresh ed25519 signing key and return its PKCS8 private PEM and SPKI
/// public PEM (both LF line endings).
pub fn generate_keypair() -> Result<(String, String)> {
    let signing = SigningKey::generate(&mut rand_core::OsRng);
    let private_pem = signing
        .to_pkcs8_pem(LineEnding::LF)
        .context("encoding private key to PKCS8 PEM")?
        .to_string();
    let public_pem = signing
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .context("encoding public key to SPKI PEM")?;
    Ok((private_pem, public_pem))
}

fn secure_key_directory(out_dir: &Path) -> Result<()> {
    if out_dir.exists() {
        bail!("key out-dir already exists: {}", out_dir.display());
    }
    if let Some(parent) = out_dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(out_dir)
            .with_context(|| format!("creating secure key out-dir {}", out_dir.display()))?;
    }

    #[cfg(windows)]
    {
        std::fs::create_dir(out_dir)
            .with_context(|| format!("creating key out-dir {}", out_dir.display()))?;
        if let Err(error) = secure_windows_acl(out_dir, true) {
            let _ = std::fs::remove_dir(out_dir);
            return Err(error);
        }
    }

    #[cfg(not(any(unix, windows)))]
    std::fs::create_dir(out_dir)
        .with_context(|| format!("creating key out-dir {}", out_dir.display()))?;

    Ok(())
}

#[cfg(windows)]
fn current_windows_user_sid() -> Result<String> {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .context("running whoami to resolve the current user SID")?;
    if !output.status.success() {
        bail!("whoami could not resolve the current user SID");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split(|character: char| character == ',' || character == '"' || character.is_whitespace())
        .find(|field| field.starts_with("S-1-"))
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("whoami returned no Windows user SID"))
}

#[cfg(windows)]
fn secure_windows_acl(path: &Path, directory: bool) -> Result<()> {
    let sid = current_windows_user_sid()?;
    let grant = if directory {
        format!("*{sid}:(OI)(CI)F")
    } else {
        format!("*{sid}:F")
    };
    let status = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritancelevel:r")
        .arg("/grant:r")
        .arg(grant)
        .status()
        .with_context(|| format!("running icacls to secure {}", path.display()))?;
    if !status.success() {
        bail!("icacls could not secure {}", path.display());
    }
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8], private: bool) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;

    let mut file = options.open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!("key file already exists: {}", path.display())
        } else {
            anyhow::anyhow!("cannot create {}: {e}", path.display())
        }
    })?;
    #[cfg(windows)]
    if private {
        if let Err(error) = secure_windows_acl(path, false) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
    }
    file.write_all(bytes)
        .with_context(|| format!("writing {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing {}", path.display()))?;
    Ok(())
}

/// Write a freshly generated keypair into `out_dir`, returning the two paths.
pub fn keygen(out_dir: &Path) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    secure_key_directory(out_dir)?;
    let private_path = out_dir.join("private.pkcs8.pem");
    let public_path = out_dir.join("public.spki.pem");
    if private_path.exists() || public_path.exists() {
        bail!("key file already exists in {}", out_dir.display());
    }

    let (private_pem, public_pem) = generate_keypair()?;
    write_new_file(&private_path, private_pem.as_bytes(), true)?;
    if let Err(error) = write_new_file(&public_path, public_pem.as_bytes(), false) {
        let _ = std::fs::remove_file(&private_path);
        return Err(error);
    }
    Ok((private_path, public_path))
}

/// Recursively zip the CONTENTS of `model_dir` into an in-memory byte vector,
/// preserving relative paths (`model_dir/sub/x.onnx` -> entry `sub/x.onnx`).
pub fn zip_model_dir(model_dir: &Path) -> Result<Vec<u8>> {
    if !model_dir.is_dir() {
        bail!("model dir {} is not a directory", model_dir.display());
    }
    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // Collect + sort entries so the zip is deterministic regardless of FS order.
        let mut entries: Vec<_> = walkdir::WalkDir::new(model_dir)
            .sort_by_file_name()
            .into_iter()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("walking model dir")?;
        entries.sort_by(|a, b| a.path().cmp(b.path()));

        for entry in entries {
            let path = entry.path();
            let rel = path
                .strip_prefix(model_dir)
                .context("stripping model-dir prefix")?;
            if rel.as_os_str().is_empty() {
                continue; // the root itself
            }
            let rel_str = rel
                .to_str()
                .with_context(|| format!("non-UTF-8 path: {}", rel.display()))?
                .replace('\\', "/");
            if entry.file_type().is_dir() {
                zip.add_directory(format!("{rel_str}/"), options)
                    .context("adding dir to zip")?;
            } else if entry.file_type().is_file() {
                zip.start_file(rel_str, options)
                    .context("starting zip file entry")?;
                let mut f = std::fs::File::open(path)
                    .with_context(|| format!("opening {}", path.display()))?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)
                    .with_context(|| format!("reading {}", path.display()))?;
                zip.write_all(&buf).context("writing zip file bytes")?;
            }
        }
        zip.finish().context("finishing zip")?;
    }
    Ok(cursor.into_inner())
}

/// Lowercase hex sha256 of the given bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Metadata for a pack, mirroring the eight signed manifest fields (minus the
/// derived `sha256`, which we compute from the weights).
pub struct PackMeta {
    pub id: String,
    pub version: String,
    pub engine: String,
    pub engine_min_version: String,
    pub task: String,
    pub license: String,
    pub attribution: String,
}

/// Sign `meta` + `sha256` with `signing`, returning the base64 ed25519 signature
/// over the canonical bytes. Reuses `modelpack::canonicalize`.
pub fn sign_manifest(signing: &SigningKey, meta: &PackMeta, sha256: &str) -> String {
    let manifest = Manifest {
        attribution: meta.attribution.clone(),
        engine: meta.engine.clone(),
        engine_min_version: meta.engine_min_version.clone(),
        id: meta.id.clone(),
        license: meta.license.clone(),
        sha256: sha256.to_string(),
        task: meta.task.clone(),
        version: meta.version.clone(),
        signature: String::new(),
    };
    let data = modelpack::canonicalize(&manifest);
    let sig = signing.sign(&data);
    BASE64_STANDARD.encode(sig.to_bytes())
}

/// Build the on-disk manifest JSON value with ALL NINE fields, in a stable order.
/// We build the object explicitly (NOT by serializing `Manifest`, whose
/// `signature` is `#[serde(skip)]`).
pub fn build_manifest_json(meta: &PackMeta, sha256: &str, signature: &str) -> serde_json::Value {
    serde_json::json!({
        "id": meta.id,
        "version": meta.version,
        "engine": meta.engine,
        "engine_min_version": meta.engine_min_version,
        "task": meta.task,
        "license": meta.license,
        "attribution": meta.attribution,
        "sha256": sha256,
        "signature": signature,
    })
}

/// Full `pack` pipeline: zip the model dir, hash, sign, write the pack, then
/// self-verify with the real `modelpack::verify_model_pack` before returning.
pub fn pack(
    model_dir: &Path,
    out_dir: &Path,
    private_pem: &str,
    meta: &PackMeta,
) -> Result<()> {
    let signing =
        SigningKey::from_pkcs8_pem(private_pem).context("loading signing key from PKCS8 PEM")?;

    let zip_bytes = zip_model_dir(model_dir)?;
    let sha256 = sha256_hex(&zip_bytes);
    let signature = sign_manifest(&signing, meta, &sha256);
    let manifest_json = build_manifest_json(meta, &sha256, &signature);

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating pack out-dir {}", out_dir.display()))?;
    let weights_path = out_dir.join("weights.bin");
    let manifest_path = out_dir.join("manifest.json");
    std::fs::write(&weights_path, &zip_bytes)
        .with_context(|| format!("writing {}", weights_path.display()))?;
    let manifest_pretty =
        serde_json::to_string_pretty(&manifest_json).context("serializing manifest json")?;
    std::fs::write(&manifest_path, manifest_pretty.as_bytes())
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    // Self-verify with the real verifier, using the public key derived from the
    // signing key, so we never ship a pack the app would reject.
    let public_pem = signing
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .context("deriving public SPKI PEM for self-check")?;
    let engine_versions = self_check_engine_versions();
    modelpack::verify_model_pack(
        &manifest_json,
        &zip_bytes,
        &PublicKey::SpkiPem(&public_pem),
        &engine_versions,
    )
    .map_err(|reason| anyhow::anyhow!("self-verification failed: {reason}"))?;

    Ok(())
}

/// Verify an existing pack directory against an SPKI public PEM. Returns the
/// attribution on success.
pub fn verify_pack(pack_dir: &Path, public_pem: &str) -> Result<String> {
    let manifest_path = pack_dir.join("manifest.json");
    let weights_path = pack_dir.join("weights.bin");
    let manifest_bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let weights = std::fs::read(&weights_path)
        .with_context(|| format!("reading {}", weights_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).context("parsing manifest.json")?;
    let engine_versions = self_check_engine_versions();
    let verified = modelpack::verify_model_pack(
        &value,
        &weights,
        &PublicKey::SpkiPem(public_pem),
        &engine_versions,
    )
    .map_err(|reason| anyhow::anyhow!("{reason}"))?;
    Ok(verified.attribution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = <repo>/tools/modelpack-sign
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
    }

    fn write_tiny_model(dir: &Path) {
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("model.onnx"), b"fake-weights-aaaa").unwrap();
        std::fs::write(dir.join("sub").join("x.onnx"), b"fake-weights-bbbb").unwrap();
    }

    fn sample_meta() -> PackMeta {
        PackMeta {
            id: "acme/whisper-base-en".to_string(),
            version: "1.2.3".to_string(),
            engine: "whisper.cpp".to_string(),
            engine_min_version: "1.5.0".to_string(),
            task: "asr".to_string(),
            license: "MIT".to_string(),
            attribution: "Acme Speech Labs".to_string(),
        }
    }

    #[test]
    fn round_trip_keygen_pack_verify() {
        let tmp = std::env::temp_dir().join(format!("mps-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let key_dir = tmp.join("keys");
        let model_dir = tmp.join("model");
        let pack_dir = tmp.join("pack");

        let (priv_path, pub_path) = keygen(&key_dir).unwrap();
        write_tiny_model(&model_dir);
        let private_pem = std::fs::read_to_string(&priv_path).unwrap();
        let public_pem = std::fs::read_to_string(&pub_path).unwrap();

        // pack() self-verifies internally; if it returns Ok the verifier accepted it.
        pack(&model_dir, &pack_dir, &private_pem, &sample_meta()).unwrap();

        // And independently re-verify via the public file.
        let attribution = verify_pack(&pack_dir, &public_pem).unwrap();
        assert_eq!(attribution, "Acme Speech Labs");

        // Tampering the weights must make the verifier reject with a hash mismatch.
        let manifest_bytes = std::fs::read(pack_dir.join("manifest.json")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let mut weights = std::fs::read(pack_dir.join("weights.bin")).unwrap();
        weights.push(0xff); // tamper
        let err = modelpack::verify_model_pack(
            &value,
            &weights,
            &PublicKey::SpkiPem(&public_pem),
            &self_check_engine_versions(),
        )
        .unwrap_err();
        assert!(err.contains("weights hash mismatch"), "got: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn keygen_creates_owner_only_private_key() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join(format!("mps-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let (private_path, _) = keygen(&tmp).unwrap();

        let mode = std::fs::metadata(private_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn keygen_refuses_to_overwrite_existing_private_key() {
        let tmp = std::env::temp_dir().join(format!("mps-existing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let private_path = tmp.join("private.pkcs8.pem");
        std::fs::write(&private_path, b"keep-me").unwrap();

        let error = keygen(&tmp).unwrap_err().to_string();

        assert!(error.contains("already exists"), "got: {error}");
        assert_eq!(std::fs::read(&private_path).unwrap(), b"keep-me");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn keygen_refuses_an_existing_output_directory() {
        let tmp = std::env::temp_dir().join(format!("mps-existing-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let error = keygen(&tmp).unwrap_err().to_string();

        assert!(error.contains("already exists"), "got: {error}");
        assert!(!tmp.join("private.pkcs8.pem").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// CROSS-TOOL CONFORMANCE: the Rust signer must reproduce the TS-generated
    /// signature byte-for-byte (ed25519 is deterministic).
    #[test]
    fn cross_tool_conformance_matches_committed_signature() {
        let root = repo_root();
        let manifest_path = root
            .join("fixtures")
            .join("modelpack")
            .join("cases")
            .join("valid")
            .join("manifest.json");
        let key_path = root
            .join("fixtures")
            .join("modelpack")
            .join("keys")
            .join("test-private.pkcs8.pem");

        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let manifest = modelpack::validate_manifest(&value).unwrap();
        let committed_signature = manifest.signature.clone();

        let data = modelpack::canonicalize(&manifest);
        let private_pem = std::fs::read_to_string(&key_path).unwrap();
        let signing = SigningKey::from_pkcs8_pem(&private_pem).unwrap();
        let sig = signing.sign(&data);
        let produced = BASE64_STANDARD.encode(sig.to_bytes());

        assert_eq!(
            produced, committed_signature,
            "Rust signer must reproduce the committed TS signature byte-for-byte"
        );
    }
}
