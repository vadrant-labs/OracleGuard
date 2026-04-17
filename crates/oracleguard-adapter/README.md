# oracleguard-adapter

Imperative shell for OracleGuard: Charli3 fetch, Ziranity CLI submission,
Cardano settlement, and artifact persistence.

## Owns

- Charli3 oracle fetch and normalization shell code
- Ziranity CLI submission shell code
- Cardano settlement shell code
- Artifact persistence helpers

## Does NOT own

- Semantic meaning, canonical types, or canonical encoding (that is
  `oracleguard-schemas`)
- Release-cap evaluator logic or policy-identity derivation (that is
  `oracleguard-policy`)
- Offline bundle verification, replay, or reporting (that is
  `oracleguard-verifier`)

## Rules

- Consumes public semantics from `oracleguard-schemas` and
  `oracleguard-policy`. Must not redefine them.
- Must not introduce hidden semantic meaning in shell code.
- Isolates non-determinism (time, network, randomness) behind explicit
  boundaries so determinism of the core is preserved.

See `docs/architecture.md` for the workspace ownership table.
