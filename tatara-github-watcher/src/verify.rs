//! HMAC-SHA256 verification of GitHub's `X-Hub-Signature-256` header.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Reasons a signature can fail.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    /// Header missing or empty.
    #[error("missing signature header")]
    MissingHeader,
    /// Header doesn't start with `sha256=`.
    #[error("malformed signature header (expected sha256= prefix)")]
    BadPrefix,
    /// Header isn't valid hex.
    #[error("signature is not valid hex")]
    BadHex,
    /// HMAC didn't match.
    #[error("signature mismatch")]
    Mismatch,
}

/// Verify the `X-Hub-Signature-256` against the secret + body.
///
/// `header_value` is the raw header value (e.g.,
/// `"sha256=abc123..."`). `body` is the raw HTTP body bytes.
/// `secret` is the shared webhook secret (operator-supplied).
///
/// Constant-time comparison. Pure.
pub fn verify_signature(header_value: &str, body: &[u8], secret: &[u8]) -> Result<(), VerifyError> {
    if header_value.is_empty() {
        return Err(VerifyError::MissingHeader);
    }
    let hex_part = header_value
        .strip_prefix("sha256=")
        .ok_or(VerifyError::BadPrefix)?;
    let supplied = hex::decode(hex_part).map_err(|_| VerifyError::BadHex)?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(body);
    let computed = mac.finalize().into_bytes();

    if computed.as_slice().ct_eq(supplied.as_slice()).into() {
        Ok(())
    } else {
        Err(VerifyError::Mismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signature(body: &[u8], secret: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(body);
        let bytes = mac.finalize().into_bytes();
        format!("sha256={}", hex::encode(bytes))
    }

    #[test]
    fn round_trip_matches() {
        let body = b"hello-world";
        let secret = b"super-secret";
        let sig = make_signature(body, secret);
        assert!(verify_signature(&sig, body, secret).is_ok());
    }

    #[test]
    fn empty_header_is_missing() {
        assert_eq!(
            verify_signature("", b"any", b"any"),
            Err(VerifyError::MissingHeader)
        );
    }

    #[test]
    fn wrong_prefix_rejected() {
        assert_eq!(
            verify_signature("md5=abc", b"any", b"any"),
            Err(VerifyError::BadPrefix)
        );
    }

    #[test]
    fn non_hex_rejected() {
        assert_eq!(
            verify_signature("sha256=not-hex!!", b"any", b"any"),
            Err(VerifyError::BadHex)
        );
    }

    #[test]
    fn tampered_body_mismatches() {
        let body = b"original";
        let secret = b"k";
        let sig = make_signature(body, secret);
        assert_eq!(
            verify_signature(&sig, b"tampered", secret),
            Err(VerifyError::Mismatch)
        );
    }

    #[test]
    fn different_secret_mismatches() {
        let body = b"x";
        let sig = make_signature(body, b"secret-a");
        assert_eq!(
            verify_signature(&sig, body, b"secret-b"),
            Err(VerifyError::Mismatch)
        );
    }

    #[test]
    fn empty_secret_still_works_for_known_pair() {
        // Edge case: HMAC accepts any key length including empty.
        let body = b"x";
        let sig = make_signature(body, b"");
        assert!(verify_signature(&sig, body, b"").is_ok());
    }
}
