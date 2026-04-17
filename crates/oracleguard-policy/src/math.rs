//! Deterministic integer math used by the evaluator.
//!
//! Owns: pure, total, integer-domain helpers used by the release-cap
//! evaluator. Every function here must be total over its input type
//! and must not panic on any value. Floating-point arithmetic is
//! forbidden at the crate level (`#![deny(clippy::float_arithmetic)]`).
//!
//! Does NOT own: the evaluator's decision flow (see
//! [`crate::evaluate`]), canonical types (those live in
//! `oracleguard-schemas`), or any I/O.

/// Upper threshold for the release-band mapping, in micro-USD.
///
/// At or above this price the evaluator selects [`BAND_HIGH_BPS`].
/// Comparison is inclusive on the lower bound, so
/// `price_microusd == THRESHOLD_HIGH_MICROUSD` selects the high band.
///
/// Pinned by Implementation Plan §9 and mirrored in
/// `docs/evaluator-contract.md` §6.
pub const THRESHOLD_HIGH_MICROUSD: u64 = 500_000;

/// Lower threshold for the release-band mapping, in micro-USD.
///
/// At or above this price (and below [`THRESHOLD_HIGH_MICROUSD`]) the
/// evaluator selects [`BAND_MID_BPS`]. Below this price the evaluator
/// selects [`BAND_LOW_BPS`]. Inclusive on the lower bound.
pub const THRESHOLD_MID_MICROUSD: u64 = 350_000;

/// 100% release band, in basis points.
pub const BAND_HIGH_BPS: u64 = 10_000;

/// 75% release band, in basis points.
pub const BAND_MID_BPS: u64 = 7_500;

/// 50% release band, in basis points.
pub const BAND_LOW_BPS: u64 = 5_000;

/// Basis-point denominator (`10_000 bps == 100%`).
///
/// Used by [`crate::math::compute_max_releasable_lovelace`] in Slice 03
/// as the fixed divisor for the basis-point multiplication.
pub const BASIS_POINT_SCALE: u64 = 10_000;

// Compile-time ordering guards. If the thresholds or band constants are
// ever reordered by accident, these `const` assertions fail to compile
// and the mistake never reaches tests or runtime.
const _: () = assert!(THRESHOLD_HIGH_MICROUSD > THRESHOLD_MID_MICROUSD);
const _: () = assert!(BAND_HIGH_BPS > BAND_MID_BPS);
const _: () = assert!(BAND_MID_BPS > BAND_LOW_BPS);
const _: () = assert!(BAND_HIGH_BPS == BASIS_POINT_SCALE);

/// Map an oracle price in micro-USD to the release-cap band in basis
/// points.
///
/// The function is total over `u64`: every price selects exactly one
/// band. Comparisons are inclusive on the lower bound, matching the
/// prose in Implementation Plan §9 and the contract doc.
///
/// `validate_oracle_fact_eval` in `oracleguard-schemas::reason` rejects
/// `price_microusd == 0` before the evaluator runs, so the zero-price
/// codepath of the low band is unreachable in practice. It remains
/// defined here so replay is well-defined across the whole `u64` domain.
///
/// ```
/// use oracleguard_policy::math::{
///     select_release_band_bps, BAND_HIGH_BPS, BAND_LOW_BPS, BAND_MID_BPS,
///     THRESHOLD_HIGH_MICROUSD, THRESHOLD_MID_MICROUSD,
/// };
/// assert_eq!(select_release_band_bps(THRESHOLD_HIGH_MICROUSD), BAND_HIGH_BPS);
/// assert_eq!(select_release_band_bps(THRESHOLD_MID_MICROUSD), BAND_MID_BPS);
/// assert_eq!(select_release_band_bps(THRESHOLD_MID_MICROUSD - 1), BAND_LOW_BPS);
/// ```
#[must_use]
pub const fn select_release_band_bps(price_microusd: u64) -> u64 {
    if price_microusd >= THRESHOLD_HIGH_MICROUSD {
        BAND_HIGH_BPS
    } else if price_microusd >= THRESHOLD_MID_MICROUSD {
        BAND_MID_BPS
    } else {
        BAND_LOW_BPS
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn band_constants_match_policy_table() {
        assert_eq!(THRESHOLD_HIGH_MICROUSD, 500_000);
        assert_eq!(THRESHOLD_MID_MICROUSD, 350_000);
        assert_eq!(BAND_HIGH_BPS, 10_000);
        assert_eq!(BAND_MID_BPS, 7_500);
        assert_eq!(BAND_LOW_BPS, 5_000);
        assert_eq!(BASIS_POINT_SCALE, 10_000);
    }

    // Ordering between thresholds and bands is enforced at compile
    // time by `const _: () = assert!(...)` in the module scope, not
    // here — a runtime `assert!` on two `const`s would be optimized
    // out and clippy rejects it.

    // ---- High-band boundary ----

    #[test]
    fn price_at_high_threshold_selects_high_band() {
        assert_eq!(select_release_band_bps(500_000), BAND_HIGH_BPS);
    }

    #[test]
    fn price_just_above_high_threshold_selects_high_band() {
        assert_eq!(select_release_band_bps(500_001), BAND_HIGH_BPS);
    }

    #[test]
    fn price_just_below_high_threshold_selects_mid_band() {
        assert_eq!(select_release_band_bps(499_999), BAND_MID_BPS);
    }

    // ---- Mid-band boundary ----

    #[test]
    fn price_at_mid_threshold_selects_mid_band() {
        assert_eq!(select_release_band_bps(350_000), BAND_MID_BPS);
    }

    #[test]
    fn price_just_above_mid_threshold_selects_mid_band() {
        assert_eq!(select_release_band_bps(350_001), BAND_MID_BPS);
    }

    #[test]
    fn price_just_below_mid_threshold_selects_low_band() {
        assert_eq!(select_release_band_bps(349_999), BAND_LOW_BPS);
    }

    // ---- Domain extremes ----

    #[test]
    fn zero_price_selects_low_band() {
        // The evaluator rejects zero-price intents before calling this
        // helper; the behavior is defined only to keep the function
        // total across the u64 domain.
        assert_eq!(select_release_band_bps(0), BAND_LOW_BPS);
    }

    #[test]
    fn one_microusd_selects_low_band() {
        assert_eq!(select_release_band_bps(1), BAND_LOW_BPS);
    }

    #[test]
    fn u64_max_price_selects_high_band() {
        assert_eq!(select_release_band_bps(u64::MAX), BAND_HIGH_BPS);
    }

    // ---- Demo anchor ----

    #[test]
    fn demo_price_450k_selects_mid_band() {
        // Implementation Plan §16 demo price: $0.45 → 75% band.
        assert_eq!(select_release_band_bps(450_000), BAND_MID_BPS);
    }

    #[test]
    fn charli3_golden_258k_selects_low_band() {
        // Live Preprod AggState golden (Cluster 3) captured at
        // 258_000 micro-USD — below the 350_000 mid threshold.
        assert_eq!(select_release_band_bps(258_000), BAND_LOW_BPS);
    }

    // ---- Determinism ----

    #[test]
    fn band_selection_is_deterministic() {
        for price in [0u64, 1, 349_999, 350_000, 499_999, 500_000, u64::MAX] {
            let a = select_release_band_bps(price);
            let b = select_release_band_bps(price);
            assert_eq!(a, b, "price {price} varied across calls");
        }
    }

    #[test]
    fn every_band_output_is_from_the_frozen_set() {
        // Exhaustive sweep over a dense set of boundary and interior
        // points; every output must be one of the three pinned band
        // constants.
        let sample_prices = [
            0u64,
            1,
            100_000,
            349_998,
            349_999,
            350_000,
            350_001,
            450_000,
            499_999,
            500_000,
            500_001,
            1_000_000,
            u64::MAX - 1,
            u64::MAX,
        ];
        for price in sample_prices {
            let bps = select_release_band_bps(price);
            assert!(
                bps == BAND_LOW_BPS || bps == BAND_MID_BPS || bps == BAND_HIGH_BPS,
                "price {price} produced non-band value {bps}"
            );
        }
    }
}
