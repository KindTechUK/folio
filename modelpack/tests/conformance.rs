//! Cross-language model-pack conformance test (Rust side).
//!
//! Loads the SAME shared fixtures (`fixtures/modelpack/cases/*`) that the TS
//! oracle test (`core/test/modelpack.conformance.test.ts`) runs, and asserts
//! the Rust verifier reaches IDENTICAL outcomes. The `valid` case verifying OK
//! in Rust proves canonicalization + ed25519 agree byte-for-byte with the
//! TS-produced signature; the reject cases prove the fail-closed categories match.
//!
//! Each case verifies twice — once with the SPKI-PEM pubkey, once with the raw
//! 32-byte pubkey — so both key-acceptance paths are exercised on real fixtures.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use modelpack::{validate_manifest, verify_model_pack, PublicKey};
use serde_json::Value;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/modelpack
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("modelpack has a parent dir")
        .to_path_buf()
}

struct Context {
    pubkey_spki_pem: String,
    pubkey_raw: Vec<u8>,
    engine_versions: BTreeMap<String, String>,
}

fn load_context(fixtures: &Path) -> Context {
    let v: Value =
        serde_json::from_str(&fs::read_to_string(fixtures.join("context.json")).unwrap()).unwrap();
    let pubkey_spki_pem = v["pubkeySpkiPem"].as_str().unwrap().to_string();
    let raw_hex = v["pubkeyRawHex"].as_str().unwrap();
    let pubkey_raw = hex_decode(raw_hex);
    let mut engine_versions = BTreeMap::new();
    for (k, val) in v["engineVersions"].as_object().unwrap() {
        engine_versions.insert(k.clone(), val.as_str().unwrap().to_string());
    }
    Context { pubkey_spki_pem, pubkey_raw, engine_versions }
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn cross_language_conformance() {
    let fixtures = repo_root().join("fixtures/modelpack");
    let cases_dir = fixtures.join("cases");
    let ctx = load_context(&fixtures);

    let mut case_names: Vec<String> = fs::read_dir(&cases_dir)
        .expect("cases dir exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    case_names.sort();
    assert!(!case_names.is_empty(), "expected at least one fixture case");

    let pubkeys = [
        PublicKey::SpkiPem(&ctx.pubkey_spki_pem),
        PublicKey::Raw(&ctx.pubkey_raw),
    ];

    for name in &case_names {
        let dir = cases_dir.join(name);
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap_or_else(|e| panic!("case {name}: manifest.json parse: {e}"));
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
        let outcome = expected["outcome"].as_str().unwrap();
        let reason_contains = expected["reasonContains"].as_str();

        let weights_path = dir.join("weights.bin");
        if !weights_path.exists() {
            // Malformed-only case: structural validation must reject before any
            // hash/signature step — same oracle the TS conformance test uses.
            let res = validate_manifest(&manifest);
            assert_eq!(outcome, "reject", "case {name}: malformed cases are always reject");
            let err = res.expect_err(&format!("case {name}: expected validate_manifest to reject"));
            if let Some(sub) = reason_contains {
                assert!(
                    err.contains(sub),
                    "case {name}: reason {err:?} does not contain {sub:?}"
                );
            }
            continue;
        }

        let weights = fs::read(&weights_path).unwrap();

        // Verify under BOTH key encodings — they must agree.
        for pubkey in &pubkeys {
            let res = verify_model_pack(&manifest, &weights, pubkey, &ctx.engine_versions);
            match outcome {
                "ok" => {
                    assert!(
                        res.is_ok(),
                        "case {name}: expected OK, got reject: {:?}",
                        res.err()
                    );
                }
                "reject" => {
                    let err = res.expect_err(&format!("case {name}: expected reject, got OK"));
                    if let Some(sub) = reason_contains {
                        assert!(
                            err.contains(sub),
                            "case {name}: reason {err:?} does not contain {sub:?}"
                        );
                    }
                }
                other => panic!("case {name}: unknown outcome {other:?}"),
            }
        }
    }
}
