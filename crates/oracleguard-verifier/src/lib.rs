//! OracleGuard offline verifier.
//!
//! This crate inspects evidence bundles and replays deterministic
//! checks using the public semantics defined in
//! [`oracleguard_schemas`] and [`oracleguard_policy`]. It must not
//! redefine semantic meaning or introduce alternate interpretations
//! of canonical types.
//!
//! ## Entry points
//!
//! - [`verify_bundle`] — read a bundle from disk and produce a
//!   [`VerifierReport`].
//! - [`verify_evidence`] — run integrity and replay checks on an
//!   already-decoded [`DisbursementEvidenceV1`].
//!
//! ## Check pipeline
//!
//! 1. [`bundle::load_bundle`] — read bytes and decode via the
//!    canonical [`oracleguard_schemas::evidence::decode_evidence`],
//!    rejecting trailing bytes.
//! 2. [`integrity::check_integrity`] — structural consistency:
//!    version pin, intent-id recomputation, authorized-effect
//!    byte-identity against the intent, `gate == reason.gate()`
//!    partition, and the cross-variant authorization/execution
//!    matrix.
//! 3. [`replay::check_replay_equivalence`] — re-run
//!    [`oracleguard_policy::evaluate::evaluate_disbursement`] over
//!    the canonical inputs in the bundle and assert the result
//!    matches the evaluator projection of the recorded
//!    authorization snapshot.
//!
//! Every stage appends typed findings to a
//! [`VerifierReport`]; the pipeline never short-circuits so every
//! inconsistency in a tampered bundle surfaces in one run.
//!
//! ## Determinism contract
//!
//! For a given evidence bundle and a pinned version of the public
//! crates, the verifier MUST produce byte-identical output on every
//! run. Non-determinism is a bug; judges rely on reproducibility to
//! detect tampering.

pub mod bundle;
pub mod integrity;
pub mod replay;
pub mod report;

use std::path::Path;

use oracleguard_schemas::evidence::DisbursementEvidenceV1;

pub use bundle::{decode_bundle, load_bundle, load_bundle_bytes, BundleLoadError};
pub use integrity::check_integrity;
pub use replay::check_replay_equivalence;
pub use report::{
    AuthorizationExecutionInconsistency, AuthorizedEffectField, VerifierFinding, VerifierReport,
};

/// Run the complete integrity + replay pipeline on an in-memory
/// [`DisbursementEvidenceV1`] and return a
/// [`VerifierReport`].
///
/// The report's findings are appended in pipeline order (integrity
/// first, replay second). A clean report
/// ([`VerifierReport::is_ok`]) means the bundle is internally
/// consistent and reproduces the recorded evaluator result.
#[must_use]
pub fn verify_evidence(evidence: &DisbursementEvidenceV1) -> VerifierReport {
    let mut report = VerifierReport::new();
    check_integrity(evidence, &mut report);
    check_replay_equivalence(evidence, &mut report);
    report
}

/// Load an evidence bundle from `path` and run [`verify_evidence`]
/// over the decoded bundle.
///
/// I/O and decode errors are surfaced as
/// [`BundleLoadError`]; integrity/replay failures are surfaced
/// inside the returned report. The two surfaces stay distinct so a
/// clean report on a missing file is not mistakable for success.
pub fn verify_bundle(path: &Path) -> Result<VerifierReport, BundleLoadError> {
    let evidence = load_bundle(path)?;
    Ok(verify_evidence(&evidence))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../fixtures/evidence");
        p.push(name);
        p
    }

    #[test]
    fn verify_bundle_reports_clean_for_every_mvp_fixture() {
        for name in [
            "allow_700_ada_bundle.postcard",
            "deny_900_ada_bundle.postcard",
            "reject_non_ada_bundle.postcard",
            "reject_pending_bundle.postcard",
        ] {
            let report = verify_bundle(&fixture_path(name)).expect("verify");
            assert!(
                report.is_ok(),
                "bundle {name} produced findings: {:?}",
                report.findings
            );
        }
    }

    #[test]
    fn verify_bundle_is_deterministic_across_calls() {
        let path = fixture_path("allow_700_ada_bundle.postcard");
        let a = verify_bundle(&path).expect("a");
        let b = verify_bundle(&path).expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn verify_bundle_surfaces_io_error_without_panicking() {
        let err =
            verify_bundle(Path::new("/definitely/not/a/real/bundle.postcard")).expect_err("io");
        match err {
            BundleLoadError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
