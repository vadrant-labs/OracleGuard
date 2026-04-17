# oracleguard-verifier

Offline verifier for OracleGuard evidence bundles.

## Owns

- Offline bundle inspection
- Replay and integrity checks that use public semantics
- Report generation for judge-facing verification

## Does NOT own

- Semantic type definitions or canonical encoding (that is
  `oracleguard-schemas`)
- Release-cap evaluator logic (that is `oracleguard-policy`)
- Fetching oracle data, submitting intents, or Cardano settlement (that is
  `oracleguard-adapter`)
- Any redefinition or "alternate interpretation" of canonical meaning

## Rules

- Depends on `oracleguard-schemas` and `oracleguard-policy` as consumers,
  not as peers that define semantics.
- Must not reimplement canonical encoding or construct a second evaluator.
- Deterministic: same bundle + same public semantics ⇒ byte-identical
  verification output.

See `docs/architecture.md` for the workspace ownership table.
