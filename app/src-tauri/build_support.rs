const DEV_TEST_PUBKEY_SPKI_PEM: &str =
    include_str!("../../fixtures/modelpack/keys/test-public.spki.pem");

fn public_key_payload(pem: &str) -> Option<String> {
    const BEGIN: &str = "-----BEGIN PUBLIC KEY-----";
    const END: &str = "-----END PUBLIC KEY-----";

    let mut inside = false;
    let mut payload = String::new();
    for raw_line in pem.lines() {
        let line = raw_line.trim();
        if !inside {
            if line.is_empty() {
                continue;
            }
            if line == BEGIN {
                inside = true;
                continue;
            }
            return None;
        }
        if line == END {
            return (!payload.is_empty()).then_some(payload);
        }
        if line.is_empty()
            || !line
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
        {
            return None;
        }
        payload.push_str(line);
    }
    None
}

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
    let configured_payload = public_key_payload(configured)
        .ok_or_else(|| "VENDOR_PUBKEY_SPKI_PEM is not a valid public-key PEM".to_string())?;
    if Some(configured_payload) == public_key_payload(DEV_TEST_PUBKEY_SPKI_PEM) {
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
    fn debug_builds_may_use_the_fixture_key() {
        assert_eq!(validate_vendor_key("debug", None), Ok(()));
        assert_eq!(
            validate_vendor_key("debug", Some(DEV_TEST_PUBKEY_SPKI_PEM)),
            Ok(())
        );
    }
}
