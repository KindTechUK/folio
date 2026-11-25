//! `modelpack-sign pack --task llm` produces a pack the real verifier (the
//! `modelpack` crate the app uses) accepts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use modelpack::{verify_model_pack_reader, PublicKey};

fn signer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_modelpack-sign"))
}

fn unique_tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mps-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn task_llm_pack_is_accepted_by_the_real_verifier() {
    let tmp = unique_tmp("llm");
    let keys = tmp.join("keys");
    let model = tmp.join("model");
    let out = tmp.join("pack");
    std::fs::create_dir_all(&model).unwrap();
    std::fs::write(model.join("tiny.gguf"), b"GGUF tiny test weights").unwrap();

    let keygen = signer()
        .args(["keygen", "--out-dir"])
        .arg(&keys)
        .output()
        .unwrap();
    assert!(keygen.status.success(), "keygen: {}", String::from_utf8_lossy(&keygen.stderr));

    let pack = signer()
        .arg("pack")
        .arg("--model-dir")
        .arg(&model)
        .arg("--out")
        .arg(&out)
        .arg("--key")
        .arg(keys.join("private.pkcs8.pem"))
        .args(["--id", "test/qwen3-4b-instruct-2507"])
        .args(["--version", "1.0.0"])
        .args(["--engine", "llama.cpp"])
        .args(["--engine-min-version", "0.6.0"])
        .args(["--task", "llm"])
        .args(["--license", "Apache-2.0"])
        .args(["--attribution", "Qwen Team (test)"])
        .output()
        .unwrap();
    assert!(pack.status.success(), "pack: {}", String::from_utf8_lossy(&pack.stderr));

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    let public_pem = std::fs::read_to_string(keys.join("public.spki.pem")).unwrap();
    let weights = std::fs::File::open(out.join("weights.bin")).unwrap();
    let mut engines = BTreeMap::new();
    engines.insert("llama.cpp".to_string(), "0.6.0".to_string());

    let verified =
        verify_model_pack_reader(&manifest, weights, &PublicKey::SpkiPem(&public_pem), &engines)
            .expect("the signed llm pack must verify");

    assert_eq!(verified.manifest.task, "llm");
    assert_eq!(verified.manifest.engine, "llama.cpp");
    assert_eq!(verified.attribution, "Qwen Team (test)");

    let _ = std::fs::remove_dir_all(&tmp);
}
