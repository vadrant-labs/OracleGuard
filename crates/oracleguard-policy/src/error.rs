//! Evaluator result types.
//!
//! Owns: the closed set of structured values the evaluator returns. An
//! [`EvaluationResult`] is either an `Allow` carrying the authorized
//! amount and the basis-point band that authorized it, or a `Deny`
//! carrying exactly one of the canonical reason codes defined in
//! [`oracleguard_schemas::reason::DisbursementReasonCode`].
//!
//! Does NOT own: the decision logic that selects a reason (see
//! [`crate::evaluate`]), the reason-code enum itself (that lives in
//! `oracleguard-schemas`), or any human-readable rendering beyond the
//! `Debug` impl.

use oracleguard_schemas::reason::DisbursementReasonCode;
use serde::{Deserialize, Serialize};

/// Closed result surface for [`crate::evaluate::evaluate_disbursement`].
///
/// The variant set is frozen by Cluster 4 Slice 01. Adding, removing,
/// or reordering variants is a canonical-byte change for the golden
/// fixtures in Slice 06 and therefore a version-boundary crossing.
///
/// ## Variants
///
/// - `Allow { authorized_amount_lovelace, release_cap_basis_points }` —
///   the evaluator authorizes the exact requested amount. The
///   `release_cap_basis_points` field records which policy band (from
///   [`crate::math`]) produced the authorization, so downstream evidence
///   can cite the band without re-deriving it.
/// - `Deny { reason }` — the evaluator denies the request. The reason
///   is a single [`DisbursementReasonCode`] from the eight pinned
///   variants; the evaluator MUST NOT emit any other deny value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvaluationResult {
    /// The request is authorized at the exact requested amount.
    Allow {
        /// Exact amount (in lovelace) that the grant gate authorizes.
        /// Equals `intent.requested_amount_lovelace` for every allow
        /// outcome: the evaluator does not silently reduce the request.
        authorized_amount_lovelace: u64,
        /// The basis-point band that authorized the request. One of
        /// `BAND_LOW_BPS`, `BAND_MID_BPS`, `BAND_HIGH_BPS` from
        /// [`crate::math`].
        release_cap_basis_points: u64,
    },
    /// The request is denied with a canonical reason code.
    Deny {
        /// The typed reason for denial, drawn from the frozen set in
        /// [`DisbursementReasonCode`].
        reason: DisbursementReasonCode,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // Destructuring guard: this match must stay exhaustive. Adding a
    // variant without updating the match (and Slice 06 fixtures) is a
    // canonical-byte change and must not pass unnoticed.
    #[test]
    fn evaluation_result_variant_set_is_frozen() {
        let cases = [
            EvaluationResult::Allow {
                authorized_amount_lovelace: 1,
                release_cap_basis_points: 7_500,
            },
            EvaluationResult::Deny {
                reason: DisbursementReasonCode::AmountZero,
            },
        ];
        for r in cases {
            let _: u8 = match r {
                EvaluationResult::Allow { .. } => 0,
                EvaluationResult::Deny { .. } => 1,
            };
        }
    }

    #[test]
    fn allow_equality_is_field_wise() {
        let a = EvaluationResult::Allow {
            authorized_amount_lovelace: 700_000_000,
            release_cap_basis_points: 7_500,
        };
        let b = EvaluationResult::Allow {
            authorized_amount_lovelace: 700_000_000,
            release_cap_basis_points: 7_500,
        };
        let c = EvaluationResult::Allow {
            authorized_amount_lovelace: 700_000_000,
            release_cap_basis_points: 10_000,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn deny_equality_is_reason_wise() {
        let a = EvaluationResult::Deny {
            reason: DisbursementReasonCode::AmountZero,
        };
        let b = EvaluationResult::Deny {
            reason: DisbursementReasonCode::AmountZero,
        };
        let c = EvaluationResult::Deny {
            reason: DisbursementReasonCode::ReleaseCapExceeded,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
