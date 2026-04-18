//! Evidence bundle loading.
//!
//! Owns: parsing an on-disk evidence bundle into canonical in-memory
//! values using the types defined in `oracleguard-schemas`.
//!
//! Does NOT own: alternate canonical encodings, a second evidence type
//! definition, or any step that happens before the bundle is on disk
//! (that is the adapter).

use std::fs;
use std::io;
use std::path::Path;

use oracleguard_schemas::evidence::{decode_evidence, DisbursementEvidenceV1, EvidenceDecodeError};

/// Errors produced by [`load_bundle_bytes`] and [`load_bundle`].
///
/// The variants are mutually exclusive and distinguish an I/O failure
/// (file unreadable, missing, permission denied) from a canonical
/// decode failure (bytes did not match the evidence schema).
#[derive(Debug)]
pub enum BundleLoadError {
    /// The filesystem could not produce the bytes. Wrapped so the
    /// caller sees the underlying `io::Error` for operational
    /// diagnostics without losing the structural distinction from
    /// decode failures.
    Io(io::Error),
    /// The bytes were retrieved but did not decode as a canonical
    /// [`DisbursementEvidenceV1`]. Either the file was truncated,
    /// not an evidence bundle, or carried trailing bytes.
    Decode(EvidenceDecodeError),
}

impl From<io::Error> for BundleLoadError {
    fn from(err: io::Error) -> Self {
        BundleLoadError::Io(err)
    }
}

impl From<EvidenceDecodeError> for BundleLoadError {
    fn from(err: EvidenceDecodeError) -> Self {
        BundleLoadError::Decode(err)
    }
}

/// Read raw bytes from `path` without decoding.
///
/// Exposed so callers that hold the bytes independently (e.g. a CLI
/// that echoes a hash or a test harness that mutates the bytes before
/// decoding) can share the I/O path with [`load_bundle`].
pub fn load_bundle_bytes(path: &Path) -> Result<Vec<u8>, BundleLoadError> {
    Ok(fs::read(path)?)
}

/// Decode canonical evidence bytes into a typed
/// [`DisbursementEvidenceV1`].
///
/// This is a thin wrapper around
/// [`oracleguard_schemas::evidence::decode_evidence`] so the verifier
/// has a single canonical decode path. Trailing bytes are rejected at
/// the schemas boundary — the verifier inherits that strictness and
/// surfaces it verbatim as [`BundleLoadError::Decode`].
pub fn decode_bundle(bytes: &[u8]) -> Result<DisbursementEvidenceV1, BundleLoadError> {
    Ok(decode_evidence(bytes)?)
}

/// Read and decode an evidence bundle from `path`.
///
/// Equivalent to `load_bundle_bytes` followed by `decode_bundle`.
/// I/O errors surface as [`BundleLoadError::Io`]; canonical-decode
/// errors surface as [`BundleLoadError::Decode`]. The two surfaces
/// stay distinct so diagnostics never conflate "file missing" with
/// "file corrupted".
pub fn load_bundle(path: &Path) -> Result<DisbursementEvidenceV1, BundleLoadError> {
    let bytes = load_bundle_bytes(path)?;
    decode_bundle(&bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use oracleguard_schemas::evidence::encode_evidence;

    fn fixture_path(name: &str) -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../fixtures/evidence");
        p.push(name);
        p
    }

    #[test]
    fn load_bundle_decodes_allow_fixture() {
        let path = fixture_path("allow_700_ada_bundle.postcard");
        let evidence = load_bundle(&path).expect("load");
        assert_eq!(
            evidence.evidence_version,
            oracleguard_schemas::evidence::EVIDENCE_VERSION_V1
        );
    }

    #[test]
    fn load_bundle_decodes_deny_fixture() {
        let path = fixture_path("deny_900_ada_bundle.postcard");
        let evidence = load_bundle(&path).expect("load");
        assert_eq!(
            evidence.evidence_version,
            oracleguard_schemas::evidence::EVIDENCE_VERSION_V1
        );
    }

    #[test]
    fn load_bundle_decodes_refusal_fixtures() {
        for name in [
            "reject_non_ada_bundle.postcard",
            "reject_pending_bundle.postcard",
        ] {
            let path = fixture_path(name);
            let evidence = load_bundle(&path).expect("load");
            assert_eq!(
                evidence.evidence_version,
                oracleguard_schemas::evidence::EVIDENCE_VERSION_V1,
                "fixture {name} did not decode to v1 evidence",
            );
        }
    }

    #[test]
    fn decode_bundle_rejects_trailing_bytes() {
        let bytes =
            load_bundle_bytes(&fixture_path("allow_700_ada_bundle.postcard")).expect("read bytes");
        let mut padded = bytes.clone();
        padded.push(0x00);
        match decode_bundle(&padded) {
            Err(BundleLoadError::Decode(EvidenceDecodeError::TrailingBytes)) => {}
            other => panic!("expected TrailingBytes, got {other:?}"),
        }
    }

    #[test]
    fn decode_bundle_rejects_empty_bytes() {
        match decode_bundle(&[]) {
            Err(BundleLoadError::Decode(EvidenceDecodeError::Malformed)) => {}
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn load_bundle_surfaces_io_error_for_missing_path() {
        let missing = PathBuf::from("/this/path/does/not/exist/ever_ever.postcard");
        match load_bundle(&missing) {
            Err(BundleLoadError::Io(_)) => {}
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn load_bundle_is_deterministic_across_calls() {
        let path = fixture_path("allow_700_ada_bundle.postcard");
        let a = load_bundle(&path).expect("a");
        let b = load_bundle(&path).expect("b");
        assert_eq!(a, b);
        let a_bytes = encode_evidence(&a).expect("encode a");
        let b_bytes = encode_evidence(&b).expect("encode b");
        assert_eq!(a_bytes, b_bytes);
    }
}
