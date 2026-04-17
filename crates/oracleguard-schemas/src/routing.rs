//! Authority routing for disbursement intents.
//!
//! Owns: the fixed well-known disbursement authority identifier
//! ([`DISBURSEMENT_OCU_ID`]) and the single-authority routing rule
//! ([`route`]) for every `DisbursementIntentV1` in the MVP.
//!
//! Does NOT own: transport, envelope construction, signing, or the
//! consensus decision that follows routing. Those are Ziranity-side
//! responsibilities and remain outside OracleGuard.
//!
//! ## Single-authority MVP rule
//!
//! Every MVP disbursement intent routes to exactly one authoritative
//! object constant unit (OCU). The target does not depend on
//! destination, oracle source, payout request, or any other dynamic
//! field — routing is domain-based, not data-driven. Future sharding
//! (by treasury or policy namespace) is explicitly out of scope for v1
//! and would be a canonical-byte version-boundary change.
//!
//! See `docs/authority-routing.md` for the public explanation of the
//! routing boundary.

use crate::intent::DisbursementIntentV1;

/// BLAKE3 derive-key domain for the disbursement OCU identifier.
///
/// Distinct from [`crate::encoding::ORACLEGUARD_DISBURSEMENT_DOMAIN`]
/// (which domain-separates the *intent identity* hash) so the OCU
/// identifier cannot collide with any intent id even if the inputs
/// coincide.
pub const DISBURSEMENT_OCU_DOMAIN: &str = "ORACLEGUARD_DISBURSEMENT_OCU_V1";

/// Input to the BLAKE3 keyed derivation that produces the OCU id.
///
/// The string is fixed for the MVP. Rotating it is a canonical-byte
/// change and therefore a version-boundary event.
pub const DISBURSEMENT_OCU_SEED: &[u8] = b"OCU/MVP/v1";

/// Fixed 32-byte well-known disbursement authority OCU identifier.
///
/// This is the single authority target for every MVP
/// `DisbursementIntentV1` submission. It is referenced verbatim by:
///
/// - [`route`] — the routing rule in this module
/// - `oracleguard_adapter::ziranity_submit` — the CLI submission wrapper
/// - the private-side Ziranity runtime (via the well-known constant)
/// - `fixtures/routing/disbursement_ocu_id.hex` — the hex fixture
///
/// The constant is derived from [`DISBURSEMENT_OCU_DOMAIN`] and
/// [`DISBURSEMENT_OCU_SEED`] via [`derive_disbursement_ocu_id`]; a
/// non-`#[ignore]` test (`ocu_id_matches_documented_derivation`) pins
/// the baked literal to the derivation so the two cannot drift.
///
/// Non-zero by construction (any keyed BLAKE3 output over a non-empty
/// seed has a negligible probability of being the zero block); the
/// `ocu_id_is_not_zero` test pins that as a checked invariant.
pub const DISBURSEMENT_OCU_ID: [u8; 32] = [
    0xa4, 0x65, 0x60, 0x6b, 0xb1, 0xb1, 0xcd, 0x4a, 0x70, 0x14, 0x55, 0x04, 0x31, 0x1b, 0x67, 0x3b,
    0x40, 0x0e, 0x6c, 0x2d, 0xd2, 0xb0, 0xd0, 0xa8, 0x3c, 0x9f, 0x46, 0xc2, 0xc1, 0x18, 0x67, 0xc4,
];

/// Recompute [`DISBURSEMENT_OCU_ID`] from its public derivation recipe.
///
/// Returns the BLAKE3 keyed hash of [`DISBURSEMENT_OCU_SEED`] under the
/// derive-key [`DISBURSEMENT_OCU_DOMAIN`]. Used by the pinning test and
/// available to downstream consumers (verifier, private runtime) that
/// need to independently verify the constant.
pub fn derive_disbursement_ocu_id() -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(DISBURSEMENT_OCU_DOMAIN);
    hasher.update(DISBURSEMENT_OCU_SEED);
    *hasher.finalize().as_bytes()
}

/// Return the authority OCU for a given disbursement intent.
///
/// Under the MVP single-authority rule this function always returns
/// [`DISBURSEMENT_OCU_ID`] regardless of any field on `intent`. The
/// intent is taken by reference so call-sites read as "this intent
/// routes to …", but the body deliberately does not inspect its
/// fields: routing is domain-based, not data-driven, and tests pin
/// that invariance under mutation of every non-authority field.
///
/// Future sharding would change this signature (not just the body)
/// because the return type would need to encode the sharding decision
/// — that is explicitly out of scope for v1.
pub fn route(intent: &DisbursementIntentV1) -> [u8; 32] {
    // Intent is taken by reference so the call-site reads correctly;
    // body ignores it to make the single-authority invariant mechanical.
    let _ = intent;
    DISBURSEMENT_OCU_ID
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn ocu_id_matches_documented_derivation() {
        // Pins the baked constant to its public recipe. If this fails,
        // either the literal drifted or the recipe changed — both are
        // canonical-byte events and must be handled as version-boundary
        // changes.
        assert_eq!(DISBURSEMENT_OCU_ID, derive_disbursement_ocu_id());
    }

    #[test]
    fn ocu_id_is_not_zero() {
        // An all-zero OCU would be structurally indistinguishable from
        // an unset 32-byte field and defeat the purpose of the constant.
        assert_ne!(DISBURSEMENT_OCU_ID, [0u8; 32]);
    }

    #[test]
    fn ocu_id_derivation_is_stable_across_calls() {
        let a = derive_disbursement_ocu_id();
        let b = derive_disbursement_ocu_id();
        assert_eq!(a, b);
    }

    #[test]
    fn ocu_domain_is_not_intent_id_domain() {
        // Guard against copy-paste: the OCU-identity domain separator
        // must differ from the intent-identity one so the two hash
        // spaces cannot collide even on identical inputs.
        assert_ne!(
            DISBURSEMENT_OCU_DOMAIN,
            crate::encoding::ORACLEGUARD_DISBURSEMENT_DOMAIN,
        );
    }

    #[test]
    fn ocu_seed_length_is_pinned() {
        // Pins the seed length so an accidental whitespace edit to the
        // byte literal can't silently change the derivation. The
        // derivation test catches content drift; this test catches
        // length drift independently.
        assert_eq!(DISBURSEMENT_OCU_SEED.len(), "OCU/MVP/v1".len());
    }

    #[test]
    fn ocu_id_hex_fixture_matches_constant() {
        // Fixtures/routing/disbursement_ocu_id.hex is the operator-
        // facing textual form of the constant. Drift between the hex
        // file and the Rust literal would confuse reviewers, so we
        // pin them together.
        const HEX: &str = include_str!("../../../fixtures/routing/disbursement_ocu_id.hex");
        let hex = HEX.trim();
        let mut expected = String::with_capacity(64);
        for byte in DISBURSEMENT_OCU_ID {
            let hi = byte >> 4;
            let lo = byte & 0x0F;
            expected.push(nibble_to_hex(hi));
            expected.push(nibble_to_hex(lo));
        }
        assert_eq!(hex, expected);
    }

    fn nibble_to_hex(n: u8) -> char {
        match n {
            0..=9 => char::from(b'0' + n),
            10..=15 => char::from(b'a' + n - 10),
            _ => unreachable!(),
        }
    }

    // ---- Slice 02 — routing rule invariance ----

    use crate::effect::{AssetIdV1, CardanoAddressV1};
    use crate::intent::INTENT_VERSION_V1;
    use crate::oracle::{
        OracleFactEvalV1, OracleFactProvenanceV1, ASSET_PAIR_ADA_USD, SOURCE_CHARLI3,
    };

    fn sample_intent() -> DisbursementIntentV1 {
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
            requested_amount_lovelace: 700_000_000,
            destination: CardanoAddressV1 {
                bytes: [0x55; 57],
                length: 57,
            },
            asset: AssetIdV1::ADA,
        }
    }

    #[test]
    fn route_returns_ocu_id_for_sample_intent() {
        assert_eq!(route(&sample_intent()), DISBURSEMENT_OCU_ID);
    }

    #[test]
    fn route_is_deterministic_across_calls() {
        let intent = sample_intent();
        let a = route(&intent);
        let b = route(&intent);
        assert_eq!(a, b);
    }

    #[test]
    fn route_is_invariant_under_destination_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.destination = CardanoAddressV1 {
            bytes: [0xAB; 57],
            length: 29,
        };
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_oracle_source_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.oracle_fact.source = *b"otherfeed\0\0\0\0\0\0\0";
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_oracle_price_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.oracle_fact.price_microusd = 1;
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_oracle_asset_pair_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.oracle_fact.asset_pair = *b"ETH/USD\0\0\0\0\0\0\0\0\0";
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_provenance_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.oracle_provenance.timestamp_unix = 0;
        m.oracle_provenance.expiry_unix = 0;
        m.oracle_provenance.aggregator_utxo_ref = [0xFF; 32];
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_requested_amount_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.requested_amount_lovelace = 1;
        assert_eq!(route(&m), base);
        m.requested_amount_lovelace = u64::MAX;
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_asset_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.asset = AssetIdV1 {
            policy_id: [0xAB; 28],
            asset_name: [0u8; 32],
            asset_name_len: 0,
        };
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_requester_id_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.requester_id = [0xAA; 32];
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_allocation_id_change() {
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.allocation_id = [0xBB; 32];
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_is_invariant_under_policy_ref_change() {
        // policy_ref is an authorization input, not a routing key. The
        // anchor gate consumes it; routing must not.
        let base = route(&sample_intent());
        let mut m = sample_intent();
        m.policy_ref = [0xCC; 32];
        assert_eq!(route(&m), base);
    }

    #[test]
    fn route_sweep_over_many_mutations_yields_single_target() {
        // Compose multiple non-authority field changes at once. The
        // sweep pins that no combination of field variation reaches a
        // second routing target.
        let base = route(&sample_intent());
        for (i, _) in (0u64..64).enumerate() {
            let mut m = sample_intent();
            m.destination.bytes[0] = i as u8;
            m.oracle_fact.price_microusd = i as u64 + 1;
            m.oracle_provenance.timestamp_unix = i as u64;
            m.requested_amount_lovelace = (i as u64).wrapping_add(1);
            m.requester_id[0] = i as u8;
            m.allocation_id[0] = i as u8;
            assert_eq!(route(&m), base);
        }
    }
}
