# Cluster 3 Closeout — Oracle Fact Normalization and Provenance Separation

This document is the explicit handoff from Cluster 3 to Cluster 4. It
exists to prove that the oracle input surface is stable enough that
release-cap evaluation can proceed without reopening oracle-boundary
ambiguity.

## Cluster 3 mechanical completion standard

| Exit criterion (Cluster 3 overview §6) | Satisfied | Evidence |
|---|---|---|
| `OracleFactEvalV1` contains only fields that may affect authorization | yes | `crates/oracleguard-schemas/src/oracle.rs` — three fixed-width fields (`asset_pair`, `price_microusd`, `source`); destructuring guard `oracle::tests::eval_struct_exposes_exact_fixed_width_fields` catches silent additions |
| `OracleFactProvenanceV1` is structurally separated from evaluation fields | yes | `crates/oracleguard-schemas/src/oracle.rs` — separate struct for `timestamp_unix`, `expiry_unix`, `aggregator_utxo_ref`; identity projection in `encoding::encode_intent_identity` omits the provenance field entirely |
| Charli3 normalization produces integer microusd values only | yes | `crates/oracleguard-adapter/src/charli3.rs::parse_aggstate_datum` + `normalize_aggstate_datum`; `#![deny(clippy::float_arithmetic)]` applied to the module; `charli3::tests::parse_golden_extracts_expected_integers` pins `258_000` against the live-captured golden fixture |
| Asset-pair and source identifiers are canonicalized into fixed representations | yes | `crates/oracleguard-schemas/src/oracle.rs` — `ASSET_PAIR_ADA_USD`, `SOURCE_CHARLI3` constants; `canonical_asset_pair` / `canonical_source` strict helpers; 10 mapping tests covering casing, whitespace, separator, and unknown-label rejection |
| Tests prove provenance-only changes do not alter evaluation-domain outputs | yes | `crates/oracleguard-schemas/tests/provenance_non_interference.rs` (6 tests, integration); `charli3::tests::provenance_only_datum_mutations_do_not_change_eval`, `..._yield_identical_eval_canonical_bytes`, `utxo_ref_does_not_change_eval`; plus the 3 Cluster 2 `intent_id` non-interference tests already in `encoding::tests` |
| Zero-price and malformed-price inputs fail deterministically | yes | `reason::DisbursementReasonCode::OraclePriceZero`; `reason::validate_oracle_fact_eval` rejects zero; `charli3::tests::zero_price_datum_parses_but_validate_rejects_it`, `zero_price_rejection_is_deterministic_across_runs`; `charli3::Charli3ParseError` covers malformed CBOR, wrong constructors, missing and duplicate keys, non-integer values; 9 parse-failure tests in `charli3::tests` |
| No wall-clock values enter authoritative release-cap computation through oracle normalization | yes | `check_freshness(provenance, now_unix_ms)` takes `now` as an explicit `u64` argument (no internal wall-clock read); the pure normalizer produces `OracleFactEvalV1` with no timestamp; `check_freshness_is_pure_wrt_now_argument` locks determinism |
| The intent surface cleanly embeds both eval-domain and provenance-domain oracle data without obscuring which fields are authoritative | yes | `DisbursementIntentV1::new_v1` docstring + doctest explain the split at the call site; `examples/build_intent_from_charli3.rs` compiled example; `docs/oracle-boundary.md` §Intent construction pattern |

## Readiness questions (Cluster 3 → Cluster 4)

### Is `OracleFactEvalV1` fixed and public?

**Yes.** Three fixed-width fields (`asset_pair: [u8;16]`, `price_microusd: u64`,
`source: [u8;16]`). The destructuring-guard test fails to compile if a
field is added or removed. The type round-trips through the pinned
postcard encoding with no runtime allocation.

### Is provenance structurally separated?

**Yes.** `OracleFactProvenanceV1` is a distinct struct. The identity
projection in `encoding::intent_id` omits the `oracle_provenance`
field by construction. The diff between full canonical bytes and
identity bytes is exactly the postcard encoding of
`OracleFactProvenanceV1`, proven by
`identity_projection_omits_provenance_bytes_entirely`.

### Is microusd normalization deterministic?

**Yes.** `parse_aggstate_datum` is a pure byte-in / typed-value-out
function with no I/O, no floats, and strict rejection of any CBOR
deviation. The captured golden fixture
(`fixtures/charli3/aggstate_v1_golden.cbor`) reproduces exactly
`price_microusd = 258_000` across runs. The live feed's native unit is
already microusd; no arithmetic conversion happens in the authoritative
path.

### Are invalid inputs rejected deterministically?

**Yes.** Malformed CBOR, wrong constructors, truncated inputs,
trailing bytes, unknown map keys, duplicate keys, and non-integer
values all produce specific typed `Charli3ParseError` variants. Zero
price produces `DisbursementReasonCode::OraclePriceZero`. Stale datums
produce `DisbursementReasonCode::OracleStale` via `check_freshness`.
No best-effort parse path exists.

### Are provenance non-interference expectations proven?

**Yes.** Schema-layer: 3 intent-id tests + 6 integration tests in
`tests/provenance_non_interference.rs`. Adapter-layer: 4 tests on the
normalization path showing eval output is byte-identical across any
provenance-only CBOR or utxo-ref mutation.

### Can Cluster 4 consume oracle inputs without touching raw API payloads or provenance metadata?

**Yes.** The evaluator will consume `OracleFactEvalV1` directly. The
adapter's `normalize_parse_validate` is a single-call pipeline that
returns either `(OracleFactEvalV1, OracleFactProvenanceV1)` or a typed
`NormalizeRejection`. The evaluator never sees raw CBOR, never sees a
timestamp, and never has to distinguish provenance from eval.

## Handoff targets for Cluster 4

Cluster 4 — **Release-Cap Evaluation Logic** — should land its work at
the following explicit locations:

- Release-cap math (integer, overflow-checked) →
  `crates/oracleguard-policy/src/math.rs`
- Evaluator entry point `evaluate_disbursement` →
  `crates/oracleguard-policy/src/evaluate.rs`
- Evaluator error surface (mapped to `DisbursementReasonCode`) →
  `crates/oracleguard-policy/src/error.rs`, consuming
  `oracleguard_schemas::reason::DisbursementReasonCode`
- Eval-input non-interference tests that include `oracle_provenance`
  in the intent-under-test → extend
  `tests/provenance_non_interference.rs`

The release rule is pinned in the Implementation Plan §9. Cluster 4
adds the integer math and gate wiring; it does not re-derive oracle
input semantics.

## What Cluster 4 must NOT do based on Cluster 3 state

- It must not redefine `OracleFactEvalV1` or `OracleFactProvenanceV1`
  in any non-schemas crate. The public shape is in
  `oracleguard-schemas`; the policy crate consumes it.
- It must not reclassify any provenance field as an evaluator input.
  That would reopen Cluster 3 and require updating both
  `docs/oracle-boundary.md` and the non-interference test map.
- It must not bypass `validate_oracle_fact_eval` or `check_freshness`.
  Those are the sanctioned gates for the oracle half of the grant
  gate.
- It must not introduce a second reason-code enum. The authoritative
  surface is `oracleguard_schemas::reason::DisbursementReasonCode` with
  its pinned discriminants.
- It must not perform wall-clock reads in the pure evaluator. Wall
  time enters only through explicit arguments to helpers like
  `check_freshness`.

## Closeout signature

All mechanical criteria in this document are satisfied on the commit
that closes Cluster 3. Cluster 4 begins from a clean CI, a pinned
golden AggState fixture captured from live Preprod, a pinned set of
eight canonical reason codes, and a workspace in which every
remaining ambiguity about which oracle fields may affect authorization
has been answered in writing and in code.
