//! Evidence bundle canonical types.
//!
//! Owns: the fixed-width public shape of a disbursement-evidence
//! bundle — the inputs, decisions, and execution outcomes that must be
//! preserved so an offline verifier can inspect the governed-action
//! chain and replay the evaluator without trust in operator narration.
//!
//! Cluster 8 Slice 01 pins the types. Assembly from adapter outputs
//! lives in `oracleguard-adapter` (Slice 02), verification lives in
//! `oracleguard-verifier` (Slice 05), and closeout docs live in
//! `docs/evidence-bundle.md` (Slice 07).
//!
//! Does NOT own: bundle assembly from adapter outputs, bundle
//! verification, storage formats beyond the canonical in-memory
//! representation, or any wall-clock reads (the freshness timestamp is
//! carried as provenance only).
//!
//! ## Semantic roles
//!
//! A [`DisbursementEvidenceV1`] is a single typed record covering one
//! disbursement outcome. Its fields are partitioned into four groups:
//!
//! 1. **Version** — `evidence_version`. Locked at v1; any change is a
//!    canonical-byte version-boundary event.
//! 2. **Request identity** — `intent` and `intent_id`. The full
//!    canonical intent is embedded so the verifier can replay without
//!    extra lookups; `intent_id` is the recorded BLAKE3 identity the
//!    verifier must recompute and match.
//! 3. **Evaluator inputs** — `allocation_basis_lovelace` and
//!    `now_unix_ms`. These are the caller-supplied inputs to the
//!    pure evaluator and the pure freshness check; the verifier
//!    replays both deterministically.
//! 4. **Outcome** — `committed_height`, `authorization`, and
//!    `execution`. The authorization snapshot is the three-gate
//!    closure's verdict; the execution outcome is the typed
//!    fulfillment result (`Settled` with a tx hash, `DeniedUpstream`
//!    copying the authorization denial, or `RejectedAtFulfillment`
//!    with a typed MVP rejection kind).
//!
//! ## What evidence is NOT
//!
//! Evidence is not a debug log. It carries no free-form strings, no
//! timestamps beyond the provenance-domain fields already present in
//! `oracleguard_schemas::oracle::OracleFactProvenanceV1`, no operator
//! narration, and no byte fields whose meaning depends on private
//! runtime state. Every field is typed, fixed-width, and tied back to
//! a public semantic type defined elsewhere in this crate or in
//! `oracleguard_policy`.

use serde::{Deserialize, Serialize};

use crate::effect::AuthorizedEffectV1;
use crate::gate::AuthorizationGate;
use crate::intent::DisbursementIntentV1;
use crate::reason::DisbursementReasonCode;

/// Current supported evidence schema version.
///
/// See `docs/canonical-encoding.md` for the version-boundary rules;
/// any change to the evidence byte surface must bump this constant.
pub const EVIDENCE_VERSION_V1: u16 = 1;

/// Closed set of fulfillment-side rejection kinds recorded in evidence.
///
/// This is the evidence-domain mirror of
/// `oracleguard_adapter::settlement::FulfillmentRejection`. The mirror
/// lives in this crate because schemas must not depend on the adapter;
/// the adapter provides a `From` impl so it can convert its typed
/// outcome into the canonical evidence shape without schemas taking on
/// a forbidden dependency.
///
/// The variant set is frozen for v1. Adding, removing, or reordering
/// variants is a canonical-byte change and therefore a schema-version
/// boundary crossing.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FulfillmentRejectionKindV1 {
    /// The authorized effect's asset was not ADA. The MVP settlement
    /// path supports ADA-only transfers; a non-ADA authorized effect is
    /// a registry misconfiguration surfaced as a typed refusal rather
    /// than silently dropped.
    NonAdaAsset = 0,
    /// The consensus receipt was not in `IntentStatus::Committed`.
    /// Settlement requires a committed decision; pending receipts are
    /// never executed, and the refusal is recorded as evidence.
    ReceiptNotCommitted = 1,
}

/// Evidence-domain mirror of
/// `oracleguard_policy::authorize::AuthorizationResult`.
///
/// Schemas cannot depend on policy, so the canonical shape is pinned
/// here and `oracleguard_policy` supplies a `From<AuthorizationResult>`
/// impl. Every field is copied verbatim: `gate == reason.gate()`
/// remains an invariant; the verifier checks it.
///
/// The `Authorized` variant inlines [`AuthorizedEffectV1`] (~230 bytes)
/// while `Denied` carries ~2 bytes. Clippy's `large_enum_variant` lint
/// is allowed here for the same reason as in
/// `oracleguard_policy::authorize::AuthorizationResult`: authorization
/// is not a hot path, and boxing would force a separate allocation on
/// every allow while breaking `Copy`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuthorizationSnapshotV1 {
    /// Every gate passed. `effect` is byte-identical to the policy
    /// crate's authorized-effect payload.
    Authorized {
        /// Canonical authorized-effect payload.
        effect: AuthorizedEffectV1,
    },
    /// A gate denied the request. `reason` and `gate` are copied
    /// verbatim from the failing gate's output; evidence never
    /// translates denial reasons.
    Denied {
        /// Canonical denial reason.
        reason: DisbursementReasonCode,
        /// Failing gate. Invariant: `gate == reason.gate()`.
        gate: AuthorizationGate,
    },
}

/// Closed set of execution outcomes recorded in evidence.
///
/// This is the evidence-domain mirror of
/// `oracleguard_adapter::settlement::FulfillmentOutcome`. Every
/// outcome of a single `fulfill` call maps to exactly one variant;
/// `Settled` is the only variant that corresponds to a submitted
/// Cardano transaction.
///
/// The `tx_hash` field on `Settled` is the canonical 32-byte Cardano
/// transaction id. It is stored as a raw byte array here (rather than
/// as `oracleguard_adapter::cardano::CardanoTxHashV1`) because schemas
/// must not depend on the adapter; the adapter's typed tx-hash wrapper
/// converts into these bytes verbatim via its `From` impl.
///
/// The variant set is frozen for v1. Adding, removing, or reordering
/// variants is a canonical-byte change and therefore a schema-version
/// boundary crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionOutcomeV1 {
    /// The allow path executed and the settlement backend captured a
    /// Cardano transaction id.
    Settled {
        /// 32-byte Cardano transaction id. Byte-identical to the
        /// adapter's `CardanoTxHashV1.0`.
        tx_hash: [u8; 32],
    },
    /// Authorization was denied upstream. No Cardano transaction was
    /// submitted; `reason` and `gate` are copied verbatim from the
    /// authorization snapshot's `Denied` variant, satisfying the
    /// cross-field invariant
    /// `execution.reason == authorization.reason` when both are
    /// present.
    DeniedUpstream {
        /// Canonical denial reason (matches the authorization
        /// snapshot's `Denied.reason`).
        reason: DisbursementReasonCode,
        /// Failing gate. Invariant: `gate == reason.gate()` and
        /// `gate == authorization.gate` when the snapshot is `Denied`.
        gate: AuthorizationGate,
    },
    /// Authorization was allowed but the fulfillment boundary refused
    /// to submit before the backend was called. No Cardano transaction
    /// exists; the typed `kind` enumerates the MVP refusal causes.
    RejectedAtFulfillment {
        /// Reason the fulfillment boundary refused to submit.
        kind: FulfillmentRejectionKindV1,
    },
}

/// Canonical public disbursement-evidence shape for v1.
///
/// A bundle that ties together policy reference (via the embedded
/// intent's `policy_ref`), canonical intent bytes (via the embedded
/// intent and its recorded `intent_id`), evaluator inputs
/// (`allocation_basis_lovelace`, `now_unix_ms`), the three-gate
/// authorization verdict (`authorization`), and the typed execution
/// outcome (`execution`).
///
/// The bundle is postcard-canonical; see [`encode_evidence`] and
/// [`decode_evidence`] for the authoritative byte surface. The type
/// is `Clone` but not `Copy` because it embeds the full intent
/// (~300 bytes); an evidence record is not meant to flow on a hot
/// path.
///
/// ## Cross-field invariants (checked by the verifier)
///
/// - `intent.intent_version == INTENT_VERSION_V1`.
/// - `intent_id == oracleguard_schemas::encoding::intent_id(&intent)`.
/// - When `authorization == Authorized { effect }`:
///   - `effect.policy_ref == intent.policy_ref`
///   - `effect.allocation_id == intent.allocation_id`
///   - `effect.requester_id == intent.requester_id`
///   - `effect.destination == intent.destination`
///   - `effect.asset == intent.asset`
///   - `effect.authorized_amount_lovelace == intent.requested_amount_lovelace`
///   - `execution == Settled { .. } || RejectedAtFulfillment { .. }`
/// - When `authorization == Denied { reason, gate }`:
///   - `gate == reason.gate()`
///   - `execution == DeniedUpstream { reason, gate }` with the same
///     `reason` and `gate` values.
///
/// These invariants are evidence-integrity checks, not new semantic
/// claims. The verifier enforces them so a malformed bundle cannot
/// silently pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisbursementEvidenceV1 {
    /// Schema version. Only `EVIDENCE_VERSION_V1` is accepted.
    pub evidence_version: u16,
    /// Canonical intent this evidence record describes.
    pub intent: DisbursementIntentV1,
    /// 32-byte BLAKE3 `intent_id` recorded at consensus time. The
    /// verifier recomputes `intent_id(&intent)` and matches.
    pub intent_id: [u8; 32],
    /// Allocation-basis input fed to `evaluate_disbursement`. Carried
    /// so the verifier can replay the evaluator deterministically.
    pub allocation_basis_lovelace: u64,
    /// Wall-clock posix-ms value fed to the grant gate's freshness
    /// check. Carried as provenance; the evaluator itself is pure over
    /// `(intent, basis)` and does not consult this field.
    pub now_unix_ms: u64,
    /// Consensus block height at which the decision committed.
    pub committed_height: u64,
    /// Three-gate authorization verdict.
    pub authorization: AuthorizationSnapshotV1,
    /// Typed execution outcome.
    pub execution: ExecutionOutcomeV1,
}

/// Errors produced by [`encode_evidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceEncodeError {
    /// The underlying serializer rejected the value. Should not happen
    /// for well-formed fixed-width types; surfaced rather than panicked.
    SerializerFailure,
}

/// Errors produced by [`decode_evidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceDecodeError {
    /// Input could not be decoded as a valid [`DisbursementEvidenceV1`].
    Malformed,
    /// Input decoded but had unconsumed trailing bytes. Trailing-byte
    /// rejection keeps one byte sequence from meaning two things.
    TrailingBytes,
}

/// Encode a [`DisbursementEvidenceV1`] to its canonical byte form.
///
/// The output bytes are the authoritative identity of the evidence
/// record. Every downstream check — verifier load, fixture golden
/// tests, cross-tool byte comparison — operates on these bytes.
pub fn encode_evidence(evidence: &DisbursementEvidenceV1) -> Result<Vec<u8>, EvidenceEncodeError> {
    postcard::to_allocvec(evidence).map_err(|_| EvidenceEncodeError::SerializerFailure)
}

/// Decode canonical bytes back into a [`DisbursementEvidenceV1`].
///
/// Strict: trailing bytes after a successful decode are rejected, so
/// one byte sequence cannot decode to two distinct meanings.
pub fn decode_evidence(bytes: &[u8]) -> Result<DisbursementEvidenceV1, EvidenceDecodeError> {
    let (evidence, remainder) = postcard::take_from_bytes::<DisbursementEvidenceV1>(bytes)
        .map_err(|_| EvidenceDecodeError::Malformed)?;
    if !remainder.is_empty() {
        return Err(EvidenceDecodeError::TrailingBytes);
    }
    Ok(evidence)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use crate::effect::{AssetIdV1, CardanoAddressV1};
    use crate::intent::INTENT_VERSION_V1;
    use crate::oracle::{OracleFactEvalV1, OracleFactProvenanceV1};

    fn sample_intent() -> DisbursementIntentV1 {
        DisbursementIntentV1 {
            intent_version: INTENT_VERSION_V1,
            policy_ref: [0x11; 32],
            allocation_id: [0x22; 32],
            requester_id: [0x33; 32],
            oracle_fact: OracleFactEvalV1 {
                asset_pair: *b"ADA/USD\0\0\0\0\0\0\0\0\0",
                price_microusd: 450_000,
                source: *b"charli3\0\0\0\0\0\0\0\0\0",
            },
            oracle_provenance: OracleFactProvenanceV1 {
                timestamp_unix: 1_713_000_000_000,
                expiry_unix: 1_713_000_300_000,
                aggregator_utxo_ref: [0x44; 32],
            },
            requested_amount_lovelace: 700_000_000,
            destination: CardanoAddressV1 {
                bytes: [0x55; 57],
                length: 57,
            },
            asset: AssetIdV1::ADA,
        }
    }

    fn sample_effect() -> AuthorizedEffectV1 {
        AuthorizedEffectV1 {
            policy_ref: [0x11; 32],
            allocation_id: [0x22; 32],
            requester_id: [0x33; 32],
            destination: CardanoAddressV1 {
                bytes: [0x55; 57],
                length: 57,
            },
            asset: AssetIdV1::ADA,
            authorized_amount_lovelace: 700_000_000,
            release_cap_basis_points: 7_500,
        }
    }

    fn sample_allow_evidence() -> DisbursementEvidenceV1 {
        DisbursementEvidenceV1 {
            evidence_version: EVIDENCE_VERSION_V1,
            intent: sample_intent(),
            intent_id: [0x77; 32],
            allocation_basis_lovelace: 1_000_000_000,
            now_unix_ms: 1_713_000_100_000,
            committed_height: 42,
            authorization: AuthorizationSnapshotV1::Authorized {
                effect: sample_effect(),
            },
            execution: ExecutionOutcomeV1::Settled {
                tx_hash: [0xAB; 32],
            },
        }
    }

    fn sample_deny_evidence() -> DisbursementEvidenceV1 {
        DisbursementEvidenceV1 {
            evidence_version: EVIDENCE_VERSION_V1,
            intent: sample_intent(),
            intent_id: [0x88; 32],
            allocation_basis_lovelace: 1_000_000_000,
            now_unix_ms: 1_713_000_100_000,
            committed_height: 43,
            authorization: AuthorizationSnapshotV1::Denied {
                reason: DisbursementReasonCode::ReleaseCapExceeded,
                gate: AuthorizationGate::Grant,
            },
            execution: ExecutionOutcomeV1::DeniedUpstream {
                reason: DisbursementReasonCode::ReleaseCapExceeded,
                gate: AuthorizationGate::Grant,
            },
        }
    }

    fn sample_refusal_evidence() -> DisbursementEvidenceV1 {
        DisbursementEvidenceV1 {
            evidence_version: EVIDENCE_VERSION_V1,
            intent: sample_intent(),
            intent_id: [0x99; 32],
            allocation_basis_lovelace: 1_000_000_000,
            now_unix_ms: 1_713_000_100_000,
            committed_height: 44,
            authorization: AuthorizationSnapshotV1::Authorized {
                effect: sample_effect(),
            },
            execution: ExecutionOutcomeV1::RejectedAtFulfillment {
                kind: FulfillmentRejectionKindV1::ReceiptNotCommitted,
            },
        }
    }

    // ---- Version pin ----

    #[test]
    fn evidence_version_constant_is_one() {
        assert_eq!(EVIDENCE_VERSION_V1, 1);
    }

    // ---- Destructuring guards ----
    //
    // If a field is added, removed, or reordered, these destructuring
    // matches fail to compile, forcing a canonical-byte
    // version-boundary conversation rather than a silent change.

    #[test]
    fn evidence_struct_exposes_exact_field_list() {
        let DisbursementEvidenceV1 {
            evidence_version,
            intent,
            intent_id,
            allocation_basis_lovelace,
            now_unix_ms,
            committed_height,
            authorization,
            execution,
        } = sample_allow_evidence();
        assert_eq!(evidence_version, EVIDENCE_VERSION_V1);
        assert_eq!(intent.requested_amount_lovelace, 700_000_000);
        assert_eq!(intent_id, [0x77; 32]);
        assert_eq!(allocation_basis_lovelace, 1_000_000_000);
        assert_eq!(now_unix_ms, 1_713_000_100_000);
        assert_eq!(committed_height, 42);
        let _ = match authorization {
            AuthorizationSnapshotV1::Authorized { .. } => 0u8,
            AuthorizationSnapshotV1::Denied { .. } => 1u8,
        };
        let _ = match execution {
            ExecutionOutcomeV1::Settled { .. } => 0u8,
            ExecutionOutcomeV1::DeniedUpstream { .. } => 1u8,
            ExecutionOutcomeV1::RejectedAtFulfillment { .. } => 2u8,
        };
    }

    #[test]
    fn authorization_snapshot_variant_list_is_frozen() {
        let cases = [
            AuthorizationSnapshotV1::Authorized {
                effect: sample_effect(),
            },
            AuthorizationSnapshotV1::Denied {
                reason: DisbursementReasonCode::PolicyNotFound,
                gate: AuthorizationGate::Anchor,
            },
        ];
        for c in cases {
            let _: u8 = match c {
                AuthorizationSnapshotV1::Authorized { .. } => 0,
                AuthorizationSnapshotV1::Denied { .. } => 1,
            };
        }
    }

    #[test]
    fn execution_outcome_variant_list_is_frozen() {
        let cases = [
            ExecutionOutcomeV1::Settled {
                tx_hash: [0x00; 32],
            },
            ExecutionOutcomeV1::DeniedUpstream {
                reason: DisbursementReasonCode::OracleStale,
                gate: AuthorizationGate::Grant,
            },
            ExecutionOutcomeV1::RejectedAtFulfillment {
                kind: FulfillmentRejectionKindV1::NonAdaAsset,
            },
        ];
        for c in cases {
            let _: u8 = match c {
                ExecutionOutcomeV1::Settled { .. } => 0,
                ExecutionOutcomeV1::DeniedUpstream { .. } => 1,
                ExecutionOutcomeV1::RejectedAtFulfillment { .. } => 2,
            };
        }
    }

    #[test]
    fn fulfillment_rejection_kind_variant_list_is_frozen() {
        for k in [
            FulfillmentRejectionKindV1::NonAdaAsset,
            FulfillmentRejectionKindV1::ReceiptNotCommitted,
        ] {
            let _: u8 = match k {
                FulfillmentRejectionKindV1::NonAdaAsset => 0,
                FulfillmentRejectionKindV1::ReceiptNotCommitted => 1,
            };
        }
    }

    #[test]
    fn fulfillment_rejection_kind_discriminants_are_stable() {
        assert_eq!(FulfillmentRejectionKindV1::NonAdaAsset as u8, 0);
        assert_eq!(FulfillmentRejectionKindV1::ReceiptNotCommitted as u8, 1);
    }

    // ---- Postcard round-trips ----

    #[test]
    fn allow_evidence_postcard_round_trips() {
        let original = sample_allow_evidence();
        let bytes = encode_evidence(&original).expect("encode");
        let decoded = decode_evidence(&bytes).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn deny_evidence_postcard_round_trips() {
        let original = sample_deny_evidence();
        let bytes = encode_evidence(&original).expect("encode");
        let decoded = decode_evidence(&bytes).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn refusal_evidence_postcard_round_trips() {
        let original = sample_refusal_evidence();
        let bytes = encode_evidence(&original).expect("encode");
        let decoded = decode_evidence(&bytes).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn authorization_snapshot_postcard_round_trips_both_variants() {
        let authorized = AuthorizationSnapshotV1::Authorized {
            effect: sample_effect(),
        };
        let denied = AuthorizationSnapshotV1::Denied {
            reason: DisbursementReasonCode::AssetMismatch,
            gate: AuthorizationGate::Registry,
        };
        for snap in [authorized, denied] {
            let bytes = postcard::to_allocvec(&snap).expect("encode");
            let (decoded, rest) =
                postcard::take_from_bytes::<AuthorizationSnapshotV1>(&bytes).expect("decode");
            assert!(rest.is_empty());
            assert_eq!(decoded, snap);
        }
    }

    #[test]
    fn execution_outcome_postcard_round_trips_all_variants() {
        let cases = [
            ExecutionOutcomeV1::Settled {
                tx_hash: [0xCD; 32],
            },
            ExecutionOutcomeV1::DeniedUpstream {
                reason: DisbursementReasonCode::AllocationNotFound,
                gate: AuthorizationGate::Registry,
            },
            ExecutionOutcomeV1::RejectedAtFulfillment {
                kind: FulfillmentRejectionKindV1::NonAdaAsset,
            },
            ExecutionOutcomeV1::RejectedAtFulfillment {
                kind: FulfillmentRejectionKindV1::ReceiptNotCommitted,
            },
        ];
        for outcome in cases {
            let bytes = postcard::to_allocvec(&outcome).expect("encode");
            let (decoded, rest) =
                postcard::take_from_bytes::<ExecutionOutcomeV1>(&bytes).expect("decode");
            assert!(rest.is_empty());
            assert_eq!(decoded, outcome);
        }
    }

    // ---- Stability and strict decode boundaries ----

    #[test]
    fn encode_is_stable_across_calls() {
        let a = encode_evidence(&sample_allow_evidence()).expect("a");
        let b = encode_evidence(&sample_allow_evidence()).expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes = encode_evidence(&sample_allow_evidence()).expect("encode");
        bytes.push(0x00);
        let err = decode_evidence(&bytes).expect_err("trailing-byte must reject");
        assert_eq!(err, EvidenceDecodeError::TrailingBytes);
    }

    #[test]
    fn decode_rejects_truncated_input() {
        let bytes = encode_evidence(&sample_allow_evidence()).expect("encode");
        let err = decode_evidence(&bytes[..bytes.len() - 1]).expect_err("truncation must reject");
        assert_eq!(err, EvidenceDecodeError::Malformed);
    }

    #[test]
    fn decode_rejects_empty_input() {
        let err = decode_evidence(&[]).expect_err("empty must reject");
        assert_eq!(err, EvidenceDecodeError::Malformed);
    }

    // ---- Byte-identity sensitivity ----

    #[test]
    fn mutating_intent_id_changes_canonical_bytes() {
        let base = encode_evidence(&sample_allow_evidence()).expect("base");
        let mut mutated = sample_allow_evidence();
        mutated.intent_id = [0xEE; 32];
        let mutated_bytes = encode_evidence(&mutated).expect("mutated");
        assert_ne!(base, mutated_bytes);
    }

    #[test]
    fn mutating_tx_hash_changes_canonical_bytes() {
        let base = encode_evidence(&sample_allow_evidence()).expect("base");
        let mut mutated = sample_allow_evidence();
        mutated.execution = ExecutionOutcomeV1::Settled {
            tx_hash: [0xEE; 32],
        };
        let mutated_bytes = encode_evidence(&mutated).expect("mutated");
        assert_ne!(base, mutated_bytes);
    }

    #[test]
    fn mutating_allocation_basis_changes_canonical_bytes() {
        let base = encode_evidence(&sample_allow_evidence()).expect("base");
        let mut mutated = sample_allow_evidence();
        mutated.allocation_basis_lovelace = 999;
        let mutated_bytes = encode_evidence(&mutated).expect("mutated");
        assert_ne!(base, mutated_bytes);
    }

    #[test]
    fn mutating_committed_height_changes_canonical_bytes() {
        let base = encode_evidence(&sample_allow_evidence()).expect("base");
        let mut mutated = sample_allow_evidence();
        mutated.committed_height = 9999;
        let mutated_bytes = encode_evidence(&mutated).expect("mutated");
        assert_ne!(base, mutated_bytes);
    }

    #[test]
    fn switching_authorization_variant_changes_canonical_bytes() {
        let base = encode_evidence(&sample_allow_evidence()).expect("base");
        let mut mutated = sample_allow_evidence();
        mutated.authorization = AuthorizationSnapshotV1::Denied {
            reason: DisbursementReasonCode::ReleaseCapExceeded,
            gate: AuthorizationGate::Grant,
        };
        let mutated_bytes = encode_evidence(&mutated).expect("mutated");
        assert_ne!(base, mutated_bytes);
    }

    #[test]
    fn switching_execution_variant_changes_canonical_bytes() {
        let base = encode_evidence(&sample_allow_evidence()).expect("base");
        let mut mutated = sample_allow_evidence();
        mutated.execution = ExecutionOutcomeV1::RejectedAtFulfillment {
            kind: FulfillmentRejectionKindV1::NonAdaAsset,
        };
        let mutated_bytes = encode_evidence(&mutated).expect("mutated");
        assert_ne!(base, mutated_bytes);
    }
}
