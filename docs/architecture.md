# OracleGuard Architecture

This document is the authoritative ownership reference for the public
OracleGuard workspace. Every crate in the workspace MUST match the
responsibilities described here. If the code or an individual crate README
drifts from this document, the architecture is wrong — not the document.

## Design stance

- OracleGuard is a **thin, policy-governed adapter** for Cardano
  disbursements. It does not absorb policy authority, consensus semantics,
  or runtime wiring.
- The public repo is the **authoritative source** of canonical semantic
  meaning, canonical byte encoding, and judge-facing verification logic.
- Core/shell boundaries are explicit. Pure functional code lives in
  `oracleguard-schemas` and `oracleguard-policy`. Shell code lives in
  `oracleguard-adapter`. `oracleguard-verifier` is a deterministic consumer
  of public semantics, not a redefinition of them.
- Private Ziranity integration depends on the public crates; it does not
  redefine any type, evaluator behavior, or canonical encoding.

## Crate ownership table

| Crate                   | Owns                                                                                                                        | Must NOT own                                                                                        |
|-------------------------|-----------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------|
| `oracleguard-schemas`   | Canonical public semantic types; versioned wire/domain structs; reason codes; authorized-effect types; evidence types.      | Shell logic; I/O; evaluator decisions; verifier orchestration.                                      |
| `oracleguard-policy`    | Pure release-cap evaluation; deterministic decision outputs; math over canonical inputs.                                    | I/O; non-determinism; shell orchestration; consumer of adapter or verifier.                         |
| `oracleguard-adapter`   | Charli3 fetch/normalize shell; Ziranity CLI submission shell; Cardano settlement shell; artifact persistence helpers.       | Semantic meaning; evaluator logic; policy identity derivation; verifier orchestration.              |
| `oracleguard-verifier`  | Offline evidence bundle inspection; replay and integrity checks **using** public semantics.                                 | Alternate semantic definitions; evaluator re-implementation; transport or runtime policy decisions. |

## Non-ownership rules (negative space)

These rules are deliberately redundant with the table above. They exist
because the failure mode being prevented — a crate quietly growing into a
responsibility it was never supposed to own — is common enough to warrant
an explicit prohibition.

- The schemas crate MUST NOT contain shell logic, I/O, or evaluator
  behavior. It is a type and encoding authority only.
- The policy crate MUST NOT perform I/O, use non-deterministic constructs,
  or depend on the adapter or verifier crates. Its inputs are canonical
  semantic values; its outputs are deterministic decisions.
- The adapter crate MUST NOT define semantic meaning, own policy-identity
  derivation, or make release-cap decisions. It only transports, persists,
  and normalizes material that the schemas and policy crates have already
  given meaning to.
- The verifier crate MUST NOT reinvent canonical encoding, define a second
  evaluator, or introduce "alternate interpretations" of public semantics.
  It verifies bundles *with* public semantics; it does not author them.
- No crate may introduce a generic `utils` module without a bounded,
  documented purpose. Generic catch-alls are how ownership boundaries rot.

## Private integration posture

Private runtime integration (consensus hooks, runtime wiring,
Shadowbox-side projection) lives **out of tree**. It depends on the public
crates; it does not redefine any semantic type, canonical encoding, or
evaluator behavior. A judge must be able to reach every public semantic
claim without reading private code.
