mod build_support;

fn main() {
    println!("cargo:rerun-if-env-changed=VENDOR_PUBKEY_SPKI_PEM");
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let vendor_key = std::env::var("VENDOR_PUBKEY_SPKI_PEM").ok();
    if let Err(reason) = build_support::validate_vendor_key(&profile, vendor_key.as_deref()) {
        panic!("{reason}");
    }
    tauri_build::build();
}
