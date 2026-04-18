//! Verifier report generation.
//!
//! Owns: the closed set of structured verifier findings and the
//! [`VerifierReport`] aggregate that carries them back to callers.
//! Findings are typed — never free-form strings — so reports are
//! byte-identically reproducible across runs and can be diffed
//! mechanically by CI.
//!
//! Does NOT own: rendering reports to external systems or terminals
//! beyond the minimal human-facing helpers declared here.

use oracleguard_policy::error::EvaluationResult;
use oracleguard_schemas::gate::AuthorizationGate;
use oracleguard_schemas::reason::DisbursementReasonCode;

/// Closed set of verifier findings.
///
/// Each variant names a specific consistency or replay failure. The
/// set is frozen for v1; adding, removing, or reordering variants is
/// a verifier-surface change that CI gates must notice.
///
/// `Debug`-printing a finding is stable: no timestamps, no paths, no
/// free-form strings. Judges rely on this determinism to diff reports
/// across runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VerifierFinding {
    /// `evidence.evidence_version` is not the version this verifier
    /// understands.
    EvidenceVersionUnsupported {
        /// Version carried by the bundle.
        found: u16,
        /// Version this verifier accepts.
        expected: u16,
    },
    /// `evidence.intent.intent_version` is not the version this
    /// verifier understands.
    IntentVersionUnsupported {
        /// Version carried by the intent.
        found: u16,
        /// Version this verifier accepts.
        expected: u16,
    },
    /// The canonical BLAKE3 intent id recomputed over
    /// `evidence.intent` disagrees with `evidence.intent_id`. Either
    /// the bundle intent was mutated after submission or the recorded
    /// id was forged; evidence integrity is broken either way.
    IntentIdMismatch {
        /// Id recomputed from `evidence.intent`.
        from_intent: [u8; 32],
        /// Id carried in `evidence.intent_id`.
        from_record: [u8; 32],
    },
    /// The `intent_id` canonical encoding path failed. Should not
    /// happen for a well-formed intent; surfaced rather than panicked.
    IntentIdRecomputeFailed,
    /// An `Authorized` snapshot's `effect` field disagrees with the
    /// canonical intent on one of the identity-bearing fields.
    AuthorizedEffectMismatch {
        /// Which field failed the byte-identity check.
        field: AuthorizedEffectField,
    },
    /// A `Denied` snapshot's `gate` does not equal `reason.gate()`.
    GateInvariantBroken {
        /// The recorded denial reason.
        reason: DisbursementReasonCode,
        /// The recorded failing gate.
        recorded_gate: AuthorizationGate,
        /// The gate required by the canonical reason-to-gate
        /// partition in `oracleguard_schemas::reason`.
        expected_gate: AuthorizationGate,
    },
    /// The `authorization` and `execution` variants are inconsistent.
    /// See [`AuthorizationExecutionInconsistency`] for the enumerated
    /// cases.
    AuthorizationExecutionInconsistent {
        /// The specific cross-variant inconsistency detected.
        kind: AuthorizationExecutionInconsistency,
    },
    /// Replaying the pure evaluator over `evidence.intent` and
    /// `evidence.allocation_basis_lovelace` produced a result that
    /// disagrees with the evaluator projection of the recorded
    /// authorization snapshot.
    ReplayDiverged {
        /// Expected evaluator result, projected from the authorization
        /// snapshot recorded in the bundle.
        expected: EvaluationResult,
        /// Actual evaluator result produced by the public evaluator
        /// against the canonical inputs in the bundle.
        actual: EvaluationResult,
    },
}

/// Which field of an `Authorized` snapshot failed the byte-identity
/// check against the intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizedEffectField {
    PolicyRef,
    AllocationId,
    RequesterId,
    Destination,
    Asset,
    AuthorizedAmountLovelace,
}

/// Enumerated cross-variant inconsistencies between `authorization`
/// and `execution`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationExecutionInconsistency {
    /// Authorization was `Authorized` but execution was
    /// `DeniedUpstream` — denial requires an upstream denial.
    AuthorizedButDeniedExecution,
    /// Authorization was `Denied` but execution was `Settled` — a
    /// Cardano transaction must not exist when authorization denied
    /// the request.
    DeniedButSettledExecution,
    /// Authorization was `Denied` but execution was
    /// `RejectedAtFulfillment` — fulfillment-side refusal is only
    /// reachable when authorization allowed the effect.
    DeniedButFulfillmentRejection,
    /// Both sides are denials but their `reason` values disagree.
    DeniedReasonMismatch,
    /// Both sides are denials but their `gate` values disagree.
    DeniedGateMismatch,
}

/// Aggregate of all [`VerifierFinding`]s produced for one evidence
/// bundle.
///
/// A bundle passes verification when `findings.is_empty()`. The
/// report is otherwise a structured list that the caller can
/// serialize, diff against a prior run, or render to a CLI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifierReport {
    /// Findings recorded for this bundle, in the fixed check order
    /// (integrity → cross-variant → replay). The order is
    /// deterministic so diffs across runs are stable.
    pub findings: Vec<VerifierFinding>,
}

impl VerifierReport {
    /// Construct an empty report. Equivalent to `Default`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
        }
    }

    /// `true` when there are no findings — evidence is internally
    /// consistent and replay matched.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.findings.is_empty()
    }

    /// Append a finding to the report.
    pub fn push(&mut self, finding: VerifierFinding) {
        self.findings.push(finding);
    }
}
