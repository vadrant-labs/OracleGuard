//! Artifact assembly and persistence helpers.
//!
//! Owns:
//!
//! - The [`From`] impls that convert adapter-domain outcome types
//!   ([`crate::cardano::CardanoTxHashV1`],
//!   [`crate::settlement::FulfillmentRejection`],
//!   [`crate::settlement::FulfillmentOutcome`]) into the canonical
//!   evidence-domain mirrors defined in
//!   [`oracleguard_schemas::evidence`].
//! - [`assemble_disbursement_evidence`] — the pure assembly function
//!   that composes a canonical
//!   [`oracleguard_schemas::evidence::DisbursementEvidenceV1`] from
//!   already-authoritative inputs: the canonical intent, the
//!   consensus receipt, the fulfillment outcome, the evaluator's
//!   allocation-basis input, and the freshness-check clock value.
//!
//! Does NOT own:
//!
//! - The canonical evidence types themselves (those live in
//!   `oracleguard_schemas::evidence`).
//! - Any authorization, evaluation, or settlement logic. This module
//!   consumes typed outputs from the authorization closure and the
//!   fulfillment boundary; it does not recompute them.
//! - The verification logic that later reads the produced bundles
//!   (that is `oracleguard-verifier`).
//!
//! ## Layering
//!
//! Structurally BLUE/GREEN: every function in this module's production
//! source is pure (no I/O, no wall-clock reads, no mutable global
//! state). Persistence to disk happens in test code only; production
//! writes are performed by a higher-level shell path that is out of
//! scope for the canonical surface.

use oracleguard_schemas::encoding::{intent_id, CanonicalEncodeError};
use oracleguard_schemas::evidence::{
    AuthorizationSnapshotV1, DisbursementEvidenceV1, ExecutionOutcomeV1,
    FulfillmentRejectionKindV1, EVIDENCE_VERSION_V1,
};
use oracleguard_schemas::intent::DisbursementIntentV1;

use crate::cardano::CardanoTxHashV1;
use crate::intake::IntentReceiptV1;
use crate::settlement::{FulfillmentOutcome, FulfillmentRejection};

/// Copy the canonical 32-byte Cardano transaction id out of the
/// adapter-domain [`CardanoTxHashV1`] wrapper.
impl From<CardanoTxHashV1> for [u8; 32] {
    fn from(value: CardanoTxHashV1) -> Self {
        value.0
    }
}

/// Map adapter-domain [`FulfillmentRejection`] variants into the
/// canonical evidence-domain [`FulfillmentRejectionKindV1`] variants.
///
/// This is a one-to-one structural mapping; adding a new
/// [`FulfillmentRejection`] variant without updating the evidence-domain
/// mirror is a canonical-byte version-boundary event and is intended to
/// fail the compile.
impl From<FulfillmentRejection> for FulfillmentRejectionKindV1 {
    fn from(value: FulfillmentRejection) -> Self {
        match value {
            FulfillmentRejection::NonAdaAsset => FulfillmentRejectionKindV1::NonAdaAsset,
            FulfillmentRejection::ReceiptNotCommitted => {
                FulfillmentRejectionKindV1::ReceiptNotCommitted
            }
        }
    }
}

/// Map adapter-domain [`FulfillmentOutcome`] variants into the
/// canonical evidence-domain [`ExecutionOutcomeV1`] variants.
///
/// `Settled { tx_hash }` copies the 32 bytes verbatim;
/// `DeniedUpstream { reason, gate }` copies the pair verbatim;
/// `RejectedAtFulfillment { kind }` delegates to the typed kind
/// mapping.
impl From<FulfillmentOutcome> for ExecutionOutcomeV1 {
    fn from(value: FulfillmentOutcome) -> Self {
        match value {
            FulfillmentOutcome::Settled { tx_hash } => ExecutionOutcomeV1::Settled {
                tx_hash: tx_hash.into(),
            },
            FulfillmentOutcome::DeniedUpstream { reason, gate } => {
                ExecutionOutcomeV1::DeniedUpstream { reason, gate }
            }
            FulfillmentOutcome::RejectedAtFulfillment { kind } => {
                ExecutionOutcomeV1::RejectedAtFulfillment { kind: kind.into() }
            }
        }
    }
}

/// Errors returned by [`assemble_disbursement_evidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssembleError {
    /// The canonical intent-id hash could not be computed. Should not
    /// happen for well-formed intents; surfaced rather than panicked
    /// to keep assembly infallibly observable at the type level.
    IntentIdFailure,
    /// The consensus receipt's `intent_id` disagreed with the id
    /// computed from the canonical intent. Either the receipt was
    /// produced from a different intent or the intent bytes were
    /// mutated after submission; evidence must not paper over this.
    IntentIdMismatch {
        /// Id computed from the canonical intent bytes.
        from_intent: [u8; 32],
        /// Id carried by the receipt.
        from_receipt: [u8; 32],
    },
}

impl From<CanonicalEncodeError> for AssembleError {
    fn from(_: CanonicalEncodeError) -> Self {
        AssembleError::IntentIdFailure
    }
}

/// Assemble a canonical
/// [`DisbursementEvidenceV1`] from already-authoritative inputs.
///
/// This is a pure, deterministic function. Every field on the
/// returned evidence record is copied or converted verbatim:
///
/// - `intent` — byte-identical to the caller-supplied intent.
/// - `intent_id` — recomputed via [`intent_id`] and cross-checked
///   against `receipt.intent_id`; mismatches return
///   [`AssembleError::IntentIdMismatch`].
/// - `allocation_basis_lovelace` — caller-supplied evaluator input
///   (the value the registry gate returned at evaluation time).
/// - `now_unix_ms` — caller-supplied freshness-check input.
/// - `committed_height` — copied from the receipt.
/// - `authorization` — converted from
///   [`receipt.output`](crate::intake::IntentReceiptV1::output) via
///   the policy-side [`From`] impl.
/// - `execution` — converted from the caller-supplied
///   [`FulfillmentOutcome`] via the adapter-side [`From`] impl.
///
/// The function does not consult the wall clock, re-derive the
/// evaluator, or re-run authorization. All such state must be
/// passed in by the caller.
pub fn assemble_disbursement_evidence(
    intent: &DisbursementIntentV1,
    receipt: &IntentReceiptV1,
    outcome: FulfillmentOutcome,
    allocation_basis_lovelace: u64,
    now_unix_ms: u64,
) -> Result<DisbursementEvidenceV1, AssembleError> {
    let derived_intent_id = intent_id(intent)?;
    if derived_intent_id != receipt.intent_id {
        return Err(AssembleError::IntentIdMismatch {
            from_intent: derived_intent_id,
            from_receipt: receipt.intent_id,
        });
    }
    let authorization: AuthorizationSnapshotV1 = receipt.output.into();
    let execution: ExecutionOutcomeV1 = outcome.into();
    Ok(DisbursementEvidenceV1 {
        evidence_version: EVIDENCE_VERSION_V1,
        intent: *intent,
        intent_id: derived_intent_id,
        allocation_basis_lovelace,
        now_unix_ms,
        committed_height: receipt.committed_height,
        authorization,
        execution,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use oracleguard_policy::authorize::AuthorizationResult;
    use oracleguard_schemas::effect::{AssetIdV1, AuthorizedEffectV1, CardanoAddressV1};
    use oracleguard_schemas::evidence::{decode_evidence, encode_evidence};
    use oracleguard_schemas::gate::AuthorizationGate;
    use oracleguard_schemas::intent::INTENT_VERSION_V1;
    use oracleguard_schemas::oracle::{
        OracleFactEvalV1, OracleFactProvenanceV1, ASSET_PAIR_ADA_USD, SOURCE_CHARLI3,
    };
    use oracleguard_schemas::reason::DisbursementReasonCode;

    use crate::intake::IntentStatus;

    // ---- Shared demo constants (aligned with Cluster 4/5 fixtures) ----

    const DEMO_POLICY_REF: [u8; 32] = [0x11; 32];
    const DEMO_ALLOCATION_ID: [u8; 32] = [0x22; 32];
    const DEMO_REQUESTER_ID: [u8; 32] = [0x33; 32];
    const DEMO_BASIS_LOVELACE: u64 = 1_000_000_000;
    const DEMO_REQUEST_LOVELACE_ALLOW: u64 = 700_000_000;
    const DEMO_PRICE_MICROUSD: u64 = 450_000;
    const DEMO_CREATION_MS: u64 = 1_713_000_000_000;
    const DEMO_EXPIRY_MS: u64 = 1_713_000_300_000;
    const DEMO_NOW_MS: u64 = 1_713_000_100_000;
    const DEMO_COMMITTED_HEIGHT: u64 = 10_042;
    const DEMO_RELEASE_CAP_BPS: u64 = 7_500;
    const DEMO_TX_HASH_HEX: &str =
        "abababababababababababababababababababababababababababababababab";

    fn demo_destination() -> CardanoAddressV1 {
        CardanoAddressV1 {
            bytes: [0x55; 57],
            length: 57,
        }
    }

    fn demo_intent(requested_amount_lovelace: u64) -> DisbursementIntentV1 {
        DisbursementIntentV1 {
            intent_version: INTENT_VERSION_V1,
            policy_ref: DEMO_POLICY_REF,
            allocation_id: DEMO_ALLOCATION_ID,
            requester_id: DEMO_REQUESTER_ID,
            oracle_fact: OracleFactEvalV1 {
                asset_pair: ASSET_PAIR_ADA_USD,
                price_microusd: DEMO_PRICE_MICROUSD,
                source: SOURCE_CHARLI3,
            },
            oracle_provenance: OracleFactProvenanceV1 {
                timestamp_unix: DEMO_CREATION_MS,
                expiry_unix: DEMO_EXPIRY_MS,
                aggregator_utxo_ref: [0x44; 32],
            },
            requested_amount_lovelace,
            destination: demo_destination(),
            asset: AssetIdV1::ADA,
        }
    }

    fn demo_effect(authorized_amount_lovelace: u64) -> AuthorizedEffectV1 {
        AuthorizedEffectV1 {
            policy_ref: DEMO_POLICY_REF,
            allocation_id: DEMO_ALLOCATION_ID,
            requester_id: DEMO_REQUESTER_ID,
            destination: demo_destination(),
            asset: AssetIdV1::ADA,
            authorized_amount_lovelace,
            release_cap_basis_points: DEMO_RELEASE_CAP_BPS,
        }
    }

    fn demo_allow_receipt(intent: &DisbursementIntentV1) -> IntentReceiptV1 {
        IntentReceiptV1 {
            intent_id: intent_id(intent).expect("intent id"),
            status: IntentStatus::Committed,
            committed_height: DEMO_COMMITTED_HEIGHT,
            output: AuthorizationResult::Authorized {
                effect: demo_effect(intent.requested_amount_lovelace),
            },
        }
    }

    fn demo_tx_hash() -> CardanoTxHashV1 {
        CardanoTxHashV1::from_hex(DEMO_TX_HASH_HEX).expect("hex parse")
    }

    // ---- Conversion impls ----

    #[test]
    fn cardano_tx_hash_unwraps_to_raw_bytes() {
        let bytes: [u8; 32] = demo_tx_hash().into();
        assert_eq!(bytes, [0xab; 32]);
    }

    #[test]
    fn fulfillment_rejection_maps_every_variant() {
        assert_eq!(
            FulfillmentRejectionKindV1::from(FulfillmentRejection::NonAdaAsset),
            FulfillmentRejectionKindV1::NonAdaAsset,
        );
        assert_eq!(
            FulfillmentRejectionKindV1::from(FulfillmentRejection::ReceiptNotCommitted),
            FulfillmentRejectionKindV1::ReceiptNotCommitted,
        );
    }

    #[test]
    fn fulfillment_outcome_settled_copies_tx_hash_bytes() {
        let converted: ExecutionOutcomeV1 = FulfillmentOutcome::Settled {
            tx_hash: demo_tx_hash(),
        }
        .into();
        assert_eq!(
            converted,
            ExecutionOutcomeV1::Settled {
                tx_hash: [0xab; 32],
            }
        );
    }

    #[test]
    fn fulfillment_outcome_denied_upstream_copies_reason_and_gate() {
        let converted: ExecutionOutcomeV1 = FulfillmentOutcome::DeniedUpstream {
            reason: DisbursementReasonCode::ReleaseCapExceeded,
            gate: AuthorizationGate::Grant,
        }
        .into();
        assert_eq!(
            converted,
            ExecutionOutcomeV1::DeniedUpstream {
                reason: DisbursementReasonCode::ReleaseCapExceeded,
                gate: AuthorizationGate::Grant,
            }
        );
    }

    #[test]
    fn fulfillment_outcome_rejected_maps_kind() {
        let converted: ExecutionOutcomeV1 = FulfillmentOutcome::RejectedAtFulfillment {
            kind: FulfillmentRejection::NonAdaAsset,
        }
        .into();
        assert_eq!(
            converted,
            ExecutionOutcomeV1::RejectedAtFulfillment {
                kind: FulfillmentRejectionKindV1::NonAdaAsset,
            }
        );
    }

    // ---- Assembly function ----

    #[test]
    fn assemble_allow_path_produces_expected_evidence() {
        let intent = demo_intent(DEMO_REQUEST_LOVELACE_ALLOW);
        let receipt = demo_allow_receipt(&intent);
        let outcome = FulfillmentOutcome::Settled {
            tx_hash: demo_tx_hash(),
        };
        let evidence = assemble_disbursement_evidence(
            &intent,
            &receipt,
            outcome,
            DEMO_BASIS_LOVELACE,
            DEMO_NOW_MS,
        )
        .expect("assemble");

        assert_eq!(evidence.evidence_version, EVIDENCE_VERSION_V1);
        assert_eq!(evidence.intent, intent);
        assert_eq!(evidence.intent_id, receipt.intent_id);
        assert_eq!(evidence.allocation_basis_lovelace, DEMO_BASIS_LOVELACE);
        assert_eq!(evidence.now_unix_ms, DEMO_NOW_MS);
        assert_eq!(evidence.committed_height, DEMO_COMMITTED_HEIGHT);
        assert_eq!(
            evidence.authorization,
            AuthorizationSnapshotV1::Authorized {
                effect: demo_effect(DEMO_REQUEST_LOVELACE_ALLOW),
            }
        );
        assert_eq!(
            evidence.execution,
            ExecutionOutcomeV1::Settled {
                tx_hash: [0xab; 32],
            }
        );
    }

    #[test]
    fn assemble_rejects_intent_id_mismatch() {
        let intent = demo_intent(DEMO_REQUEST_LOVELACE_ALLOW);
        let mut receipt = demo_allow_receipt(&intent);
        receipt.intent_id = [0xFF; 32];
        let err = assemble_disbursement_evidence(
            &intent,
            &receipt,
            FulfillmentOutcome::Settled {
                tx_hash: demo_tx_hash(),
            },
            DEMO_BASIS_LOVELACE,
            DEMO_NOW_MS,
        )
        .expect_err("mismatch must reject");
        match err {
            AssembleError::IntentIdMismatch { from_receipt, .. } => {
                assert_eq!(from_receipt, [0xFF; 32]);
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    #[test]
    fn assemble_is_deterministic() {
        let intent = demo_intent(DEMO_REQUEST_LOVELACE_ALLOW);
        let receipt = demo_allow_receipt(&intent);
        let a = assemble_disbursement_evidence(
            &intent,
            &receipt,
            FulfillmentOutcome::Settled {
                tx_hash: demo_tx_hash(),
            },
            DEMO_BASIS_LOVELACE,
            DEMO_NOW_MS,
        )
        .expect("a");
        let b = assemble_disbursement_evidence(
            &intent,
            &receipt,
            FulfillmentOutcome::Settled {
                tx_hash: demo_tx_hash(),
            },
            DEMO_BASIS_LOVELACE,
            DEMO_NOW_MS,
        )
        .expect("b");
        assert_eq!(a, b);
        let a_bytes = encode_evidence(&a).expect("encode a");
        let b_bytes = encode_evidence(&b).expect("encode b");
        assert_eq!(a_bytes, b_bytes);
    }

    // ---- Golden allow-path fixture ----
    //
    // The 700 ADA allow path is the reference MVP success scenario.
    // The fixture pins the canonical evidence bytes so the verifier,
    // CI gates, and external judges all inspect the same record.

    const ALLOW_700_ADA_GOLDEN: &[u8] =
        include_bytes!("../../../fixtures/evidence/allow_700_ada_bundle.postcard");

    fn golden_allow_evidence() -> DisbursementEvidenceV1 {
        let intent = demo_intent(DEMO_REQUEST_LOVELACE_ALLOW);
        let receipt = demo_allow_receipt(&intent);
        assemble_disbursement_evidence(
            &intent,
            &receipt,
            FulfillmentOutcome::Settled {
                tx_hash: demo_tx_hash(),
            },
            DEMO_BASIS_LOVELACE,
            DEMO_NOW_MS,
        )
        .expect("assemble allow")
    }

    #[test]
    fn allow_700_ada_golden_decodes_to_expected_evidence() {
        let expected = golden_allow_evidence();
        let decoded = decode_evidence(ALLOW_700_ADA_GOLDEN).expect("decode golden");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn allow_700_ada_golden_encode_round_trips() {
        let expected = golden_allow_evidence();
        let bytes = encode_evidence(&expected).expect("encode");
        assert_eq!(bytes.as_slice(), ALLOW_700_ADA_GOLDEN);
    }

    #[test]
    fn allow_700_ada_golden_covers_full_governed_action_chain() {
        let decoded = decode_evidence(ALLOW_700_ADA_GOLDEN).expect("decode golden");

        // Policy reference: links evidence to the Katiba-anchored
        // policy identity.
        assert_eq!(decoded.intent.policy_ref, DEMO_POLICY_REF);

        // Oracle fact: participates in authorization.
        assert_eq!(
            decoded.intent.oracle_fact.price_microusd,
            DEMO_PRICE_MICROUSD
        );
        assert_eq!(decoded.intent.oracle_fact.asset_pair, ASSET_PAIR_ADA_USD);
        assert_eq!(decoded.intent.oracle_fact.source, SOURCE_CHARLI3);

        // Request identity: intent-id recomputation matches.
        let recomputed = intent_id(&decoded.intent).expect("intent id from decoded fixture");
        assert_eq!(decoded.intent_id, recomputed);

        // Decision: allow, under the mid band, at the requested amount.
        match decoded.authorization {
            AuthorizationSnapshotV1::Authorized { effect } => {
                assert_eq!(
                    effect.authorized_amount_lovelace,
                    DEMO_REQUEST_LOVELACE_ALLOW,
                );
                assert_eq!(effect.release_cap_basis_points, DEMO_RELEASE_CAP_BPS);
                assert_eq!(effect.destination, demo_destination());
                assert_eq!(effect.asset, AssetIdV1::ADA);
            }
            other => panic!("expected Authorized, got {other:?}"),
        }

        // Execution: Settled with the demo tx hash.
        assert_eq!(
            decoded.execution,
            ExecutionOutcomeV1::Settled {
                tx_hash: [0xab; 32],
            }
        );
    }

    /// Regenerate `fixtures/evidence/allow_700_ada_bundle.postcard`
    /// when the fixture shape itself changes deliberately. Not run in
    /// CI:
    ///
    /// ```text
    /// cargo test -p oracleguard-adapter --lib \
    ///   -- --ignored regenerate_allow_700_ada_golden_fixture
    /// ```
    #[test]
    #[ignore = "regenerates golden fixture; run manually only"]
    fn regenerate_allow_700_ada_golden_fixture() {
        let evidence = golden_allow_evidence();
        let bytes = encode_evidence(&evidence).expect("encode");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/evidence/allow_700_ada_bundle.postcard"
        );
        std::fs::write(path, bytes).expect("write golden fixture");
    }
}
