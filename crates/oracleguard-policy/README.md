# oracleguard-policy

Pure deterministic release-cap evaluation for OracleGuard.

## Owns

- Release-cap evaluation over canonical semantic inputs
- Deterministic decision outputs (allow/deny + reason codes)
- Math operations used by the evaluator

## Does NOT own

- Any I/O, network calls, filesystem access, or async work
- Semantic type definitions or canonical encoding (that is
  `oracleguard-schemas`)
- Fetching oracle data, submitting intents, or Cardano settlement (that is
  `oracleguard-adapter`)
- Bundle verification or replay orchestration (that is
  `oracleguard-verifier`)

## Rules

- Depends only on `oracleguard-schemas`. Must not depend on the adapter or
  verifier crates.
- No floating-point arithmetic in authoritative decision paths.
- No `todo!`, `unimplemented!`, or non-deterministic constructs.
- Same canonical inputs MUST yield byte-identical decision outputs.

See `docs/architecture.md` for the workspace ownership table.
