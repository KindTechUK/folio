use ed25519_dalek::{pkcs8::DecodePublicKey, VerifyingKey};

const DEV_TEST_PUBKEY_SPKI_PEM: &str =
    include_str!("../../fixtures/modelpack/keys/test-public.spki.pem");

pub(crate) fn validate_vendor_key(profile: &str, configured: Option<&str>) -> Result<(), String> {
    if profile != "release" {
        return Ok(());
    }

    let configured = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "release builds require VENDOR_PUBKEY_SPKI_PEM; refusing the development key"
                .to_string()
        })?;
    let configured_key = VerifyingKey::from_public_key_pem(configured).map_err(|error| {
        format!("VENDOR_PUBKEY_SPKI_PEM is not a valid Ed25519 SPKI key: {error}")
    })?;
    let test_key = VerifyingKey::from_public_key_pem(DEV_TEST_PUBKEY_SPKI_PEM)
        .map_err(|error| format!("committed development public key is invalid: {error}"))?;
    if configured_key == test_key {
        return Err(
            "release builds must not trust the committed development/test public key".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTHER_KEY: &str = include_str!("../../fixtures/modelpack/keys/untrusted-public.spki.pem");

    #[test]
    fn release_requires_a_vendor_key() {
        assert!(validate_vendor_key("release", None).is_err());
        assert!(validate_vendor_key("release", Some("   ")).is_err());
    }

    #[test]
    fn release_rejects_the_committed_test_key() {
        assert!(validate_vendor_key("release", Some(DEV_TEST_PUBKEY_SPKI_PEM)).is_err());
        let crlf = DEV_TEST_PUBKEY_SPKI_PEM.replace('\n', "\r\n");
        assert!(validate_vendor_key("release", Some(&crlf)).is_err());
    }

    #[test]
    fn release_accepts_a_non_test_vendor_key() {
        assert_eq!(validate_vendor_key("release", Some(OTHER_KEY)), Ok(()));
    }

    #[test]
    fn release_rejects_base64_that_is_not_an_ed25519_public_key() {
        let invalid = "-----BEGIN PUBLIC KEY-----\nAAECAwQFBgcICQ==\n-----END PUBLIC KEY-----";
        assert!(validate_vendor_key("release", Some(invalid)).is_err());
    }

    #[test]
    fn debug_builds_may_use_the_fixture_key() {
        assert_eq!(validate_vendor_key("debug", None), Ok(()));
        assert_eq!(
            validate_vendor_key("debug", Some(DEV_TEST_PUBKEY_SPKI_PEM)),
            Ok(())
        );
    }
}
