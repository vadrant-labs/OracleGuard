# OracleGuard

An independently verifiable, policy-governed Cardano disbursement adapter.

## Public Crates

This repository is a Cargo workspace. The public API surface lives in four
crates, each with a single, non-overlapping responsibility:

- **`oracleguard-schemas`** — canonical public semantic types (versioned wire
  and domain structs, reason codes, authorized-effect types, evidence types).
- **`oracleguard-policy`** — pure deterministic release-cap evaluation over
  the semantic types defined in `oracleguard-schemas`. No I/O.
- **`oracleguard-adapter`** — imperative shell for Charli3 oracle fetch,
  Ziranity CLI submission, Cardano settlement, and artifact persistence.
  Consumes public semantics; does not redefine them.
- **`oracleguard-verifier`** — offline evidence bundle verifier. Uses the
  public semantics; does not introduce alternate interpretations.

The public repo is the authoritative source of OracleGuard semantic meaning,
canonical bytes, and judge-facing verification logic. Private integration
depends on these crates and must not redefine their semantics.

## Build

```
cargo build --workspace
```

## Git Hooks

Enable the repository's managed hooks once after cloning:

```
git config core.hooksPath .githooks
```

The `commit-msg` hook rejects AI attribution. The `pre-commit` hook blocks
staged secrets (mnemonics, signing keys, bearer tokens, `.skey`/`.key`
files).

## License

MIT. See `LICENSE`.
