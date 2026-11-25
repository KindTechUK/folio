//! Where `pack` writes, and what it leaves behind when it fails.

use std::path::{Path, PathBuf};

use modelpack_sign::{generate_keypair, pack, verify_pack, PackMeta};

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

fn meta() -> PackMeta {
    PackMeta {
        id: "test/tiny".to_string(),
        version: "1.0.0".to_string(),
        engine: "sherpa-onnx".to_string(),
        engine_min_version: "1.0.0".to_string(),
        task: "asr".to_string(),
        license: "MIT".to_string(),
        attribution: "Test".to_string(),
    }
}

fn model_dir(root: &Path) -> PathBuf {
    let model = root.join("model");
    std::fs::create_dir_all(&model).unwrap();
    std::fs::write(model.join("tokens.txt"), b"a 0\nb 1\n").unwrap();
    model
}

#[test]
fn a_pack_inside_its_own_model_dir_is_refused() {
    let tmp = unique_tmp("out-inside");
    let model = model_dir(&tmp);
    let out = model.join("pack");
    let (private_pem, _) = generate_keypair().unwrap();

    let result = pack(&model, &out, &private_pem, &meta());

    assert!(result.is_err(), "packing into the model dir must be refused");
    assert!(!out.join("weights.bin").exists());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(unix)]
#[test]
fn a_failed_repack_leaves_the_existing_pack_intact() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = unique_tmp("failed-repack");
    let model = model_dir(&tmp);
    let out = tmp.join("pack");
    let (private_pem, public_pem) = generate_keypair().unwrap();
    pack(&model, &out, &private_pem, &meta()).unwrap();
    let unreadable = model.join("z-unreadable.onnx");
    std::fs::write(&unreadable, b"weights").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

    let repack = pack(&model, &out, &private_pem, &meta());

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(repack.is_err(), "a source file that can't be read must fail the pack");
    assert!(verify_pack(&out, &public_pem).is_ok(), "the earlier pack must still verify");
    let _ = std::fs::remove_dir_all(&tmp);
}
