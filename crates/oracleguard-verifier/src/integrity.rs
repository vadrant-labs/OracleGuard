//! Bundle integrity checks.
//!
//! Owns: verifying that the bytes and digests in a loaded evidence
//! bundle match the canonical encoding defined in
//! `oracleguard-schemas`.
//!
//! Does NOT own: producing the bundle (that is the adapter), nor
//! re-executing the evaluator (that is `crate::replay` using
//! `oracleguard-policy`). This module only enforces structural
//! consistency rules stated on
//! [`oracleguard_schemas::evidence::DisbursementEvidenceV1`].
//!
//! ## Checks performed
//!
//! 1. Version compatibility — `evidence.evidence_version` and
//!    `evidence.intent.intent_version` must match the v1 constants.
//! 2. Intent-id recomputation — `evidence.intent_id` must equal
//!    `oracleguard_schemas::encoding::intent_id(&evidence.intent)`.
//! 3. Authorization consistency — when the snapshot is `Authorized`,
//!    every identity-bearing effect field (`policy_ref`,
//!    `allocation_id`, `requester_id`, `destination`, `asset`,
//!    `authorized_amount_lovelace`) must match the corresponding
//!    intent field. When the snapshot is `Denied`, the
//!    `gate == reason.gate()` partition must hold.
//! 4. Cross-variant consistency — the `authorization` and `execution`
//!    variant combination must be one of the allowed pairings; see
//!    [`crate::report::AuthorizationExecutionInconsistency`].
//!
//! Every failed check is recorded as a typed
//! [`crate::report::VerifierFinding`] appended to the report; the
//! function never short-circuits so a bundle with multiple
//! inconsistencies surfaces all of them in one run.

use oracleguard_schemas::encoding::intent_id;
use oracleguard_schemas::evidence::{
    AuthorizationSnapshotV1, DisbursementEvidenceV1, ExecutionOutcomeV1, EVIDENCE_VERSION_V1,
};
use oracleguard_schemas::intent::INTENT_VERSION_V1;

use crate::report::{
    AuthorizationExecutionInconsistency, AuthorizedEffectField, VerifierFinding, VerifierReport,
};

/// Run every integrity check on `evidence` and append any findings to
/// `report`.
///
/// Determinism: for identical evidence input, the appended findings
/// are byte-identically reproducible — same variants, same payloads,
/// same order. CI diffs reports across runs; non-determinism here is
/// a verifier bug.
pub fn check_integrity(evidence: &DisbursementEvidenceV1, report: &mut VerifierReport) {
    check_versions(evidence, report);
    check_intent_id(evidence, report);
    check_authorization_consistency(evidence, report);
    check_cross_variant_consistency(evidence, report);
}

fn check_versions(evidence: &DisbursementEvidenceV1, report: &mut VerifierReport) {
    if evidence.evidence_version != EVIDENCE_VERSION_V1 {
        report.push(VerifierFinding::EvidenceVersionUnsupported {
            found: evidence.evidence_version,
            expected: EVIDENCE_VERSION_V1,
        });
    }
    if evidence.intent.intent_version != INTENT_VERSION_V1 {
        report.push(VerifierFinding::IntentVersionUnsupported {
            found: evidence.intent.intent_version,
            expected: INTENT_VERSION_V1,
        });
    }
}

fn check_intent_id(evidence: &DisbursementEvidenceV1, report: &mut VerifierReport) {
    match intent_id(&evidence.intent) {
        Ok(recomputed) => {
            if recomputed != evidence.intent_id {
                report.push(VerifierFinding::IntentIdMismatch {
                    from_intent: recomputed,
                    from_record: evidence.intent_id,
                });
            }
        }
        Err(_) => {
            report.push(VerifierFinding::IntentIdRecomputeFailed);
        }
    }
}

fn check_authorization_consistency(evidence: &DisbursementEvidenceV1, report: &mut VerifierReport) {
    match evidence.authorization {
        AuthorizationSnapshotV1::Authorized { effect } => {
            let intent = &evidence.intent;
            if effect.policy_ref != intent.policy_ref {
                report.push(VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::PolicyRef,
                });
            }
            if effect.allocation_id != intent.allocation_id {
                report.push(VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::AllocationId,
                });
            }
            if effect.requester_id != intent.requester_id {
                report.push(VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::RequesterId,
                });
            }
            if effect.destination != intent.destination {
                report.push(VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::Destination,
                });
            }
            if effect.asset != intent.asset {
                report.push(VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::Asset,
                });
            }
            if effect.authorized_amount_lovelace != intent.requested_amount_lovelace {
                report.push(VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::AuthorizedAmountLovelace,
                });
            }
        }
        AuthorizationSnapshotV1::Denied { reason, gate } => {
            let expected_gate = reason.gate();
            if gate != expected_gate {
                report.push(VerifierFinding::GateInvariantBroken {
                    reason,
                    recorded_gate: gate,
                    expected_gate,
                });
            }
        }
    }
}

fn check_cross_variant_consistency(evidence: &DisbursementEvidenceV1, report: &mut VerifierReport) {
    use AuthorizationExecutionInconsistency as X;

    match (&evidence.authorization, &evidence.execution) {
        // Allowed combinations — no finding.
        (AuthorizationSnapshotV1::Authorized { .. }, ExecutionOutcomeV1::Settled { .. }) => {}
        (
            AuthorizationSnapshotV1::Authorized { .. },
            ExecutionOutcomeV1::RejectedAtFulfillment { .. },
        ) => {}

        // Disallowed: authorized but execution records an upstream deny.
        (AuthorizationSnapshotV1::Authorized { .. }, ExecutionOutcomeV1::DeniedUpstream { .. }) => {
            report.push(VerifierFinding::AuthorizationExecutionInconsistent {
                kind: X::AuthorizedButDeniedExecution,
            });
        }

        // Disallowed: denied but execution records a settled tx.
        (AuthorizationSnapshotV1::Denied { .. }, ExecutionOutcomeV1::Settled { .. }) => {
            report.push(VerifierFinding::AuthorizationExecutionInconsistent {
                kind: X::DeniedButSettledExecution,
            });
        }

        // Disallowed: denied but execution records a fulfillment-side refusal.
        (
            AuthorizationSnapshotV1::Denied { .. },
            ExecutionOutcomeV1::RejectedAtFulfillment { .. },
        ) => {
            report.push(VerifierFinding::AuthorizationExecutionInconsistent {
                kind: X::DeniedButFulfillmentRejection,
            });
        }

        // Both denied — reason and gate must match.
        (
            AuthorizationSnapshotV1::Denied {
                reason: r_auth,
                gate: g_auth,
            },
            ExecutionOutcomeV1::DeniedUpstream {
                reason: r_exec,
                gate: g_exec,
            },
        ) => {
            if r_auth != r_exec {
                report.push(VerifierFinding::AuthorizationExecutionInconsistent {
                    kind: X::DeniedReasonMismatch,
                });
            }
            if g_auth != g_exec {
                report.push(VerifierFinding::AuthorizationExecutionInconsistent {
                    kind: X::DeniedGateMismatch,
                });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use oracleguard_schemas::effect::AssetIdV1;
    use oracleguard_schemas::evidence::FulfillmentRejectionKindV1;
    use oracleguard_schemas::gate::AuthorizationGate;
    use oracleguard_schemas::reason::DisbursementReasonCode;

    use crate::bundle::load_bundle;

    fn fixture(name: &str) -> DisbursementEvidenceV1 {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../fixtures/evidence");
        p.push(name);
        load_bundle(&p).expect("load fixture")
    }

    fn run(evidence: &DisbursementEvidenceV1) -> VerifierReport {
        let mut report = VerifierReport::new();
        check_integrity(evidence, &mut report);
        report
    }

    #[test]
    fn allow_fixture_passes_integrity_check() {
        let report = run(&fixture("allow_700_ada_bundle.postcard"));
        assert!(
            report.is_ok(),
            "allow fixture had integrity findings: {:?}",
            report.findings
        );
    }

    #[test]
    fn deny_fixture_passes_integrity_check() {
        let report = run(&fixture("deny_900_ada_bundle.postcard"));
        assert!(
            report.is_ok(),
            "deny fixture had integrity findings: {:?}",
            report.findings
        );
    }

    #[test]
    fn refusal_fixtures_pass_integrity_check() {
        for name in [
            "reject_non_ada_bundle.postcard",
            "reject_pending_bundle.postcard",
        ] {
            let report = run(&fixture(name));
            assert!(
                report.is_ok(),
                "refusal fixture {name} had integrity findings: {:?}",
                report.findings
            );
        }
    }

    #[test]
    fn version_mismatch_is_flagged() {
        let mut evidence = fixture("allow_700_ada_bundle.postcard");
        evidence.evidence_version = 99;
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::EvidenceVersionUnsupported {
                found: 99,
                expected: EVIDENCE_VERSION_V1,
            }));
    }

    #[test]
    fn intent_version_mismatch_is_flagged() {
        let mut evidence = fixture("allow_700_ada_bundle.postcard");
        evidence.intent.intent_version = 7;
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::IntentVersionUnsupported {
                found: 7,
                expected: INTENT_VERSION_V1,
            }));
    }

    #[test]
    fn intent_id_mismatch_is_flagged() {
        let mut evidence = fixture("allow_700_ada_bundle.postcard");
        evidence.intent_id = [0xFF; 32];
        let report = run(&evidence);
        let recomputed = intent_id(&evidence.intent).expect("intent id");
        assert!(report
            .findings
            .contains(&VerifierFinding::IntentIdMismatch {
                from_intent: recomputed,
                from_record: [0xFF; 32],
            }));
    }

    #[test]
    fn authorized_effect_policy_ref_mismatch_is_flagged() {
        let mut evidence = fixture("allow_700_ada_bundle.postcard");
        if let AuthorizationSnapshotV1::Authorized { ref mut effect } = evidence.authorization {
            effect.policy_ref = [0xEE; 32];
        }
        let report = run(&evidence);
        assert!(
            report.findings.iter().any(|f| matches!(
                f,
                VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::PolicyRef,
                }
            )),
            "missing PolicyRef finding: {:?}",
            report.findings
        );
    }

    #[test]
    fn authorized_effect_amount_mismatch_is_flagged() {
        let mut evidence = fixture("allow_700_ada_bundle.postcard");
        if let AuthorizationSnapshotV1::Authorized { ref mut effect } = evidence.authorization {
            effect.authorized_amount_lovelace = 1;
        }
        let report = run(&evidence);
        assert!(
            report.findings.iter().any(|f| matches!(
                f,
                VerifierFinding::AuthorizedEffectMismatch {
                    field: AuthorizedEffectField::AuthorizedAmountLovelace,
                }
            )),
            "missing AuthorizedAmountLovelace finding: {:?}",
            report.findings
        );
    }

    #[test]
    fn denied_gate_invariant_break_is_flagged() {
        let mut evidence = fixture("deny_900_ada_bundle.postcard");
        if let AuthorizationSnapshotV1::Denied { ref mut gate, .. } = evidence.authorization {
            *gate = AuthorizationGate::Anchor; // ReleaseCapExceeded is a Grant-gate reason
        }
        let report = run(&evidence);
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f, VerifierFinding::GateInvariantBroken { .. })),
            "missing GateInvariantBroken finding: {:?}",
            report.findings
        );
    }

    #[test]
    fn authorized_but_denied_execution_is_flagged() {
        let mut evidence = fixture("allow_700_ada_bundle.postcard");
        evidence.execution = ExecutionOutcomeV1::DeniedUpstream {
            reason: DisbursementReasonCode::ReleaseCapExceeded,
            gate: AuthorizationGate::Grant,
        };
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::AuthorizationExecutionInconsistent {
                kind: AuthorizationExecutionInconsistency::AuthorizedButDeniedExecution,
            }));
    }

    #[test]
    fn denied_but_settled_execution_is_flagged() {
        let mut evidence = fixture("deny_900_ada_bundle.postcard");
        evidence.execution = ExecutionOutcomeV1::Settled {
            tx_hash: [0xab; 32],
        };
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::AuthorizationExecutionInconsistent {
                kind: AuthorizationExecutionInconsistency::DeniedButSettledExecution,
            }));
    }

    #[test]
    fn denied_but_fulfillment_rejection_is_flagged() {
        let mut evidence = fixture("deny_900_ada_bundle.postcard");
        evidence.execution = ExecutionOutcomeV1::RejectedAtFulfillment {
            kind: FulfillmentRejectionKindV1::NonAdaAsset,
        };
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::AuthorizationExecutionInconsistent {
                kind: AuthorizationExecutionInconsistency::DeniedButFulfillmentRejection,
            }));
    }

    #[test]
    fn denied_reason_mismatch_is_flagged() {
        let mut evidence = fixture("deny_900_ada_bundle.postcard");
        evidence.execution = ExecutionOutcomeV1::DeniedUpstream {
            reason: DisbursementReasonCode::AmountZero,
            gate: AuthorizationGate::Grant,
        };
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::AuthorizationExecutionInconsistent {
                kind: AuthorizationExecutionInconsistency::DeniedReasonMismatch,
            }));
    }

    #[test]
    fn denied_gate_mismatch_between_snapshot_and_execution_is_flagged() {
        // Keep snapshot at (ReleaseCapExceeded, Grant); change
        // execution to (ReleaseCapExceeded, Registry) — violates the
        // cross-field gate equality but not the reason.gate()
        // partition inside the snapshot.
        let mut evidence = fixture("deny_900_ada_bundle.postcard");
        evidence.execution = ExecutionOutcomeV1::DeniedUpstream {
            reason: DisbursementReasonCode::ReleaseCapExceeded,
            gate: AuthorizationGate::Registry,
        };
        let report = run(&evidence);
        assert!(report
            .findings
            .contains(&VerifierFinding::AuthorizationExecutionInconsistent {
                kind: AuthorizationExecutionInconsistency::DeniedGateMismatch,
            }));
    }

    #[test]
    fn non_ada_refusal_is_allowed_variant_pairing() {
        // Refusal with NonAdaAsset: authorization = Authorized (with
        // non-ADA asset), execution = RejectedAtFulfillment. The
        // cross-variant rule permits this pairing; the authorized-
        // effect byte-identity check still passes because the
        // intent's asset and the effect's asset are both the non-ADA
        // sentinel.
        let evidence = fixture("reject_non_ada_bundle.postcard");
        assert_ne!(evidence.intent.asset, AssetIdV1::ADA);
        let report = run(&evidence);
        assert!(
            report.is_ok(),
            "refusal bundle flagged: {:?}",
            report.findings
        );
    }
}
