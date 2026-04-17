//! Canonical policy bytes and `policy_ref` derivation.
//!
//! Owns: canonicalization of policy JSON into stable bytes and the
//! deterministic SHA-256-based derivation that turns those bytes into
//! the authoritative 32-byte [`PolicyRef`].
//!
//! Does NOT own: storage, retrieval, or transport of policy documents
//! (those are shell concerns) and does NOT adjudicate policy
//! correctness (Katiba is the policy authority).

use sha2::{Digest, Sha256};

use serde_json::Value;

/// Errors returned by [`canonicalize_policy_json`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyCanonError {
    /// Input was not well-formed JSON.
    InvalidJson,
    /// A JSON number with a fractional part or exponent was encountered.
    /// OracleGuard forbids floats on the authoritative surface.
    FloatForbidden,
    /// Serializing a string or key to canonical JSON failed. Should not
    /// occur for valid `serde_json::Value` inputs; surfaced rather than
    /// panicked.
    EncodeFailure,
}

/// Produce canonical policy bytes from a JSON policy document.
///
/// The canonical form is:
/// 1. strict JSON parse,
/// 2. rejection of any floating-point number,
/// 3. recursive lexicographic sort of object keys by key bytes,
/// 4. compact output with no insignificant whitespace.
///
/// See `docs/policy-canonicalization.md` for the full specification.
pub fn canonicalize_policy_json(raw: &[u8]) -> Result<Vec<u8>, PolicyCanonError> {
    let value: Value = serde_json::from_slice(raw).map_err(|_| PolicyCanonError::InvalidJson)?;
    let mut out: Vec<u8> = Vec::new();
    write_canonical(&value, &mut out)?;
    Ok(out)
}

fn write_canonical(value: &Value, out: &mut Vec<u8>) -> Result<(), PolicyCanonError> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => {
            if n.is_f64() {
                return Err(PolicyCanonError::FloatForbidden);
            }
            let text = n.to_string();
            out.extend_from_slice(text.as_bytes());
        }
        Value::String(s) => {
            let encoded = serde_json::to_string(s).map_err(|_| PolicyCanonError::EncodeFailure)?;
            out.extend_from_slice(encoded.as_bytes());
        }
        Value::Array(arr) => {
            out.push(b'[');
            let mut first = true;
            for item in arr {
                if !first {
                    out.push(b',');
                }
                first = false;
                write_canonical(item, out)?;
            }
            out.push(b']');
        }
        Value::Object(obj) => {
            let mut entries: Vec<(&String, &Value)> = obj.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            out.push(b'{');
            let mut first = true;
            for (k, v) in entries {
                if !first {
                    out.push(b',');
                }
                first = false;
                let encoded_key =
                    serde_json::to_string(k).map_err(|_| PolicyCanonError::EncodeFailure)?;
                out.extend_from_slice(encoded_key.as_bytes());
                out.push(b':');
                write_canonical(v, out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

/// Canonical 32-byte policy identity.
///
/// `PolicyRef` is the authoritative public identity of a governing
/// policy. It is produced by hashing the canonical policy bytes (see
/// [`canonicalize_policy_json`]) and nothing else. Every component of
/// OracleGuard that refers to a policy — intents, evaluator decisions,
/// evidence records — does so through a `PolicyRef` value.
///
/// The byte layout is fixed: 32 bytes, in the order SHA-256 produces
/// them. Reversing, truncating, or re-encoding those bytes is not
/// `PolicyRef` and is not a policy identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyRef(pub [u8; 32]);

impl PolicyRef {
    /// Return the raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Derive the canonical [`PolicyRef`] from canonical policy bytes.
///
/// Callers MUST pass bytes produced by [`canonicalize_policy_json`].
/// Hashing pretty-printed or editor-formatted policy bytes yields a
/// different value that is NOT a valid OracleGuard policy identity.
pub fn derive_policy_ref(canonical_bytes: &[u8]) -> PolicyRef {
    let mut hasher = Sha256::new();
    hasher.update(canonical_bytes);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    PolicyRef(out)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]
mod tests {
    use super::*;

    const FIXTURE_PRETTY: &[u8] = include_bytes!("../../../fixtures/policy_v1.json");
    const FIXTURE_CANONICAL: &[u8] = include_bytes!("../../../fixtures/policy_v1.canonical.bytes");

    fn canonicalize(raw: &[u8]) -> Vec<u8> {
        match canonicalize_policy_json(raw) {
            Ok(bytes) => bytes,
            Err(e) => panic!("canonicalize failed: {e:?}"),
        }
    }

    #[test]
    fn canonical_bytes_stable_across_loads() {
        let first = canonicalize(FIXTURE_PRETTY);
        let second = canonicalize(FIXTURE_PRETTY);
        assert_eq!(first, second);
    }

    #[test]
    fn canonical_bytes_match_golden() {
        let bytes = canonicalize(FIXTURE_PRETTY);
        assert_eq!(bytes.as_slice(), FIXTURE_CANONICAL);
    }

    #[test]
    fn whitespace_and_key_order_do_not_affect_canonical_bytes() {
        let a = br#"{"schema":"oracleguard.policy.v1","policy_version":1,"anchored_commitment":"katiba://policy/constitutional-release/v1","release_cap_basis_points":7500,"allowed_assets":["ADA"]}"#;
        let b = br#"
            {
                "allowed_assets" : [ "ADA" ],
                "release_cap_basis_points" : 7500,
                "policy_version"          :1,
                "anchored_commitment":"katiba://policy/constitutional-release/v1",
                "schema": "oracleguard.policy.v1"
            }
        "#;
        let ca = canonicalize(a);
        let cb = canonicalize(b);
        assert_eq!(ca, cb);
    }

    #[test]
    fn nested_objects_have_keys_sorted_at_every_depth() {
        let input = br#"{"outer_z":{"b":1,"a":2},"outer_a":[{"z":1,"a":2}]}"#;
        let bytes = canonicalize(input);
        let expected = br#"{"outer_a":[{"a":2,"z":1}],"outer_z":{"a":2,"b":1}}"#;
        assert_eq!(bytes.as_slice(), &expected[..]);
    }

    #[test]
    fn float_value_is_rejected() {
        let input = br#"{"release_cap_basis_points":75.0}"#;
        let err = canonicalize_policy_json(input).expect_err("expected float rejection");
        assert_eq!(err, PolicyCanonError::FloatForbidden);
    }

    #[test]
    fn float_with_exponent_is_rejected() {
        let input = br#"{"x":1e2}"#;
        let err = canonicalize_policy_json(input).expect_err("expected float rejection");
        assert_eq!(err, PolicyCanonError::FloatForbidden);
    }

    #[test]
    fn malformed_json_is_rejected() {
        let input = br#"{"unterminated": "#;
        let err = canonicalize_policy_json(input).expect_err("expected parse error");
        assert_eq!(err, PolicyCanonError::InvalidJson);
    }

    #[test]
    fn mutation_of_semantic_bytes_changes_canonical_output() {
        let original = canonicalize(FIXTURE_PRETTY);
        let mutated = br#"{"schema":"oracleguard.policy.v1","policy_version":1,"anchored_commitment":"katiba://policy/constitutional-release/v1","release_cap_basis_points":5000,"allowed_assets":["ADA"]}"#;
        let mutated_bytes = canonicalize(mutated);
        assert_ne!(original, mutated_bytes);
    }

    #[test]
    fn non_canonical_raw_bytes_differ_from_canonical_bytes() {
        // The pretty-printed fixture itself must not be confused with its
        // canonical form. A reader who skips canonicalization and hashes
        // the pretty bytes would get a different value.
        assert_ne!(FIXTURE_PRETTY, FIXTURE_CANONICAL);
    }

    // Golden `policy_ref` for `fixtures/policy_v1.canonical.bytes`,
    // verifiable by hand: `sha256sum fixtures/policy_v1.canonical.bytes`.
    const FIXTURE_POLICY_REF: [u8; 32] = [
        0x56, 0xa7, 0xbb, 0x97, 0x93, 0xe4, 0x0a, 0xa5, 0x44, 0x02, 0xce, 0x67, 0xfc, 0xbc, 0xe1,
        0x7d, 0xee, 0x93, 0xb6, 0x71, 0x3c, 0x76, 0xcc, 0xba, 0x6c, 0x02, 0xc1, 0x1f, 0x74, 0x99,
        0x68, 0xc2,
    ];

    #[test]
    fn policy_ref_matches_golden_sha256() {
        let r = derive_policy_ref(FIXTURE_CANONICAL);
        assert_eq!(r.as_bytes(), &FIXTURE_POLICY_REF);
    }

    #[test]
    fn policy_ref_stable_across_derivations() {
        let a = derive_policy_ref(FIXTURE_CANONICAL);
        let b = derive_policy_ref(FIXTURE_CANONICAL);
        assert_eq!(a, b);
    }

    #[test]
    fn policy_ref_changes_with_canonical_bytes() {
        let original = derive_policy_ref(FIXTURE_CANONICAL);
        let mutated = canonicalize(br#"{"schema":"oracleguard.policy.v1","policy_version":1,"anchored_commitment":"katiba://policy/constitutional-release/v1","release_cap_basis_points":5000,"allowed_assets":["ADA"]}"#);
        let mutated_ref = derive_policy_ref(&mutated);
        assert_ne!(original, mutated_ref);
    }

    #[test]
    fn non_canonical_hash_differs_from_policy_ref() {
        // Hashing the pretty-printed fixture directly is NOT `policy_ref`.
        // This test documents and protects that distinction.
        let canonical_ref = derive_policy_ref(FIXTURE_CANONICAL);
        let mut hasher = Sha256::new();
        hasher.update(FIXTURE_PRETTY);
        let pretty = hasher.finalize();
        assert_ne!(canonical_ref.as_bytes()[..], pretty[..]);
    }

    #[test]
    fn derive_from_canonicalize_roundtrip_matches_golden() {
        let canonical = canonicalize(FIXTURE_PRETTY);
        let r = derive_policy_ref(&canonical);
        assert_eq!(r.as_bytes(), &FIXTURE_POLICY_REF);
    }
}
