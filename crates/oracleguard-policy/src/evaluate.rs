//! Release-cap evaluator entry point.
//!
//! Owns: the deterministic function that maps canonical semantic inputs
//! (a [`DisbursementIntentV1`] and a caller-supplied allocation basis)
//! to an [`EvaluationResult`]. The evaluator is pure: no I/O, no
//! floating-point arithmetic, no wall-clock reads, no mutable state.
//!
//! Does NOT own: input collection, anchor-gate / registry-gate lookups
//! (Cluster 5), freshness (freshness is checked in the adapter before
//! the evaluator runs), effect execution, or evidence persistence.
//!
//! See `docs/evaluator-contract.md` for the full public contract.

use oracleguard_schemas::intent::DisbursementIntentV1;
use oracleguard_schemas::reason::DisbursementReasonCode;

use crate::error::EvaluationResult;

/// Evaluate a disbursement intent against a caller-supplied allocation
/// basis and return a typed [`EvaluationResult`].
///
/// This is the single public entry point for the release-cap decision
/// core. It consumes exactly two deterministic inputs:
///
/// - `intent` — a canonical v1 disbursement intent. The caller is
///   responsible for having normalized the intent through
///   [`oracleguard_schemas::intent::validate_intent_version`] before
///   calling; non-v1 intents are a Cluster 5 anchor-gate concern and
///   MUST NOT reach this function in release builds.
/// - `allocation_basis_lovelace` — the approved allocation size, in
///   lovelace, that the requester is drawing from. In Cluster 5 the
///   registry gate resolves this from `intent.allocation_id`; in
///   Cluster 4 it is passed directly so the evaluator can be tested in
///   isolation.
///
/// ## Cluster 4 scope
///
/// Slice 01 lands only the input/output boundary. The grant-gate
/// sequencing (price band → cap → allow/deny) arrives in Slice 05. For
/// now the function handles exactly one branch — a zero requested
/// amount denies with [`DisbursementReasonCode::AmountZero`] — and
/// returns the same deny for every other input. This lets downstream
/// work depend on the signature without waiting for later slices, and
/// the placeholder branch is replaced, not extended, by Slice 05.
///
/// ## What this function will never do
///
/// - Perform I/O or allocate for I/O.
/// - Read a wall clock.
/// - Produce a non-enumerated reason code.
/// - Return `Allow` with an amount different from
///   `intent.requested_amount_lovelace`.
/// - Consult `intent.oracle_provenance` for any authorization purpose.
pub fn evaluate_disbursement(
    intent: &DisbursementIntentV1,
    _allocation_basis_lovelace: u64,
) -> EvaluationResult {
    // Slice 01 provisional body. Replaced in Slice 05 with the full
    // grant-gate sequence; the signature and result shape are stable
    // from this slice forward.
    if intent.requested_amount_lovelace == 0 {
        return EvaluationResult::Deny {
            reason: DisbursementReasonCode::AmountZero,
        };
    }
    EvaluationResult::Deny {
        reason: DisbursementReasonCode::AmountZero,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use oracleguard_schemas::effect::{AssetIdV1, CardanoAddressV1};
    use oracleguard_schemas::intent::INTENT_VERSION_V1;
    use oracleguard_schemas::oracle::{
        OracleFactEvalV1, OracleFactProvenanceV1, ASSET_PAIR_ADA_USD, SOURCE_CHARLI3,
    };

    fn demo_intent(requested: u64) -> DisbursementIntentV1 {
        DisbursementIntentV1 {
            intent_version: INTENT_VERSION_V1,
            policy_ref: [0x11; 32],
            allocation_id: [0x22; 32],
            requester_id: [0x33; 32],
            oracle_fact: OracleFactEvalV1 {
                asset_pair: ASSET_PAIR_ADA_USD,
                price_microusd: 450_000,
                source: SOURCE_CHARLI3,
            },
            oracle_provenance: OracleFactProvenanceV1 {
                timestamp_unix: 1_713_000_000_000,
                expiry_unix: 1_713_000_300_000,
                aggregator_utxo_ref: [0x44; 32],
            },
            requested_amount_lovelace: requested,
            destination: CardanoAddressV1 {
                bytes: [0x55; 57],
                length: 57,
            },
            asset: AssetIdV1::ADA,
        }
    }

    #[test]
    fn zero_amount_denies_with_amount_zero() {
        let r = evaluate_disbursement(&demo_intent(0), 1_000_000_000);
        assert_eq!(
            r,
            EvaluationResult::Deny {
                reason: DisbursementReasonCode::AmountZero
            }
        );
    }

    #[test]
    fn evaluate_is_deterministic_for_identical_inputs() {
        let a = evaluate_disbursement(&demo_intent(0), 1_000_000_000);
        let b = evaluate_disbursement(&demo_intent(0), 1_000_000_000);
        assert_eq!(a, b);
    }

    #[test]
    fn evaluate_result_is_copy_and_equates_by_value() {
        let r = evaluate_disbursement(&demo_intent(0), 1_000_000_000);
        let s = r;
        assert_eq!(r, s);
    }
}
