# oracleguard-schemas

Canonical public semantic types for OracleGuard. This crate is the sole
public owner of semantic meaning.

## Owns

- Canonical public semantic types
- Versioned wire and domain structs
- Reason codes
- Authorized-effect types
- Evidence types
- Canonical byte encoding

## Does NOT own

- Shell logic or I/O of any kind
- Release-cap evaluator decisions (that is `oracleguard-policy`)
- Shell orchestration, transport, or settlement (that is
  `oracleguard-adapter`)
- Bundle inspection, replay, or reporting (that is
  `oracleguard-verifier`)

## Rules

- No dependency on any other OracleGuard crate.
- No I/O, no async, no floating-point in authoritative paths.
- Breaking changes to canonical types require a version bump on the
  affected `*V<n>` struct.

See `docs/architecture.md` for the workspace ownership table.
