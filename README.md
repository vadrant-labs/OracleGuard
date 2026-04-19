# OracleGuard

**Policy-governed Cardano treasury disbursements where the live Charli3 oracle sets the releasable amount, a deterministic BFT network authorizes the decision, and any auditor can independently replay why each withdrawal was allowed or denied.**

OracleGuard is a thin adapter that connects Cardano treasury operations to the KZS stack (Katiba / Ziranity / Shadowbox). Given a pre-approved allocation, a fixed anchored policy, and a live Charli3 ADA/USD price, it builds a canonical disbursement intent, submits it to Ziranity BFT consensus — where OracleGuard's three-gate closure runs inside every validator — and on *Authorized*, emits a typed effect that the Cardano settlement layer executes verbatim and nothing else. Every decision produces a canonical evidence bundle that can be replayed offline from this repo's public evaluator, with no access to the private network required.

Built for the Charli3 Oracles Hackathon 2026.

## Why this matters

- **Meaningful oracle integration.** Charli3's ADA/USD price isn't cosmetic; it shifts the active release band, which changes the cap (50 % / 75 % / 100 % of allocation), which changes what consensus authorizes. The oracle is a first-class decision input, not a displayed number.
- **Governance at the consensus layer.** The three-gate closure (anchor → registry → grant) runs inside every Ziranity validator, not on the client. BFT agreement is agreement on the *decision*, not just on a passed message.
- **Independently verifiable.** Evidence bundles are canonical postcard bytes. Any auditor can run `oracleguard-verifier` offline against them and reach the same verdict — no trust in the runtime required.
- **Cardano-native.** Real settlement via Ogmios + pycardano. Real tx_ids show up on Preprod cexplorer. The authorized effect is what lands on chain; nothing else.

## Ecosystem map (what each layer does)

| Layer | Role |
|---|---|
| **Charli3** (pull oracle) | Oracle fact source. ADA/USD on-demand aggregation on Cardano Preprod. |
| **Katiba** (off-system reference) | Policy authority. Policy identity is the 32-byte sha256 of the canonical JSON. |
| **OracleGuard** (this repo) | Canonical intent builder + offline evaluator + settlement adapter + verifier. |
| **Ziranity** (partner BFT network) | Deterministic authoritative evaluator. Runs the three-gate closure inside consensus and emits a typed `AuthorizationResult` under a quorum certificate. |
| **Shadowbox** (verification surface) | Recorded-evidence projection. Out of scope for this hackathon; the offline verifier in this repo is the standalone replay path. |
| **Cardano Preprod** | Settlement layer. Receives the authorized effect as a real transaction; nothing else. |

## Locked MVP scope

Post-approval constitutional disbursement from an already-approved managed pool. A treasury operator has pre-approved a 1 000 ADA allocation to a specific recipient; OracleGuard decides how much of that allocation can be released *right now* based on the current oracle price band, and whether the specific requested amount falls inside or outside the band's cap.

Out of scope: multi-asset (ADA only), multi-authority routing (single disbursement OCU), on-chain Katiba anchoring, Shadowbox registered bundles.

## Repo layout

Four public crates — non-overlapping responsibilities, strict dependency direction.

| Crate | Purpose | Depends on (internal) |
|---|---|---|
| `oracleguard-schemas` | Canonical public semantic types + encoding (DisbursementIntentV1, AuthorizedEffectV1, DisbursementReasonCode, evidence bundle). Integer-only, `#[deny(clippy::float_arithmetic)]`. | — |
| `oracleguard-policy` | Pure deterministic release-cap evaluator + three-gate authorization closure. No I/O. | `oracleguard-schemas` |
| `oracleguard-adapter` | Imperative shell: Kupo fetch, Charli3 pull, Ziranity CLI submit, Cardano settlement via pycardano + Ogmios. Does not redefine semantic meaning. | `oracleguard-schemas`, `oracleguard-policy` |
| `oracleguard-verifier` | Offline evidence bundle verifier. Replays the evaluator against recorded inputs. | `oracleguard-schemas`, `oracleguard-policy` |

## Oracle feed

Charli3 ODV (On-Demand Verification) pull oracle, `ADA/USD` feed on Cardano Preprod.
- Oracle script address: `addr_test1wq3pacs7jcrlwehpuy3ryj8kwvsqzjp9z6dpmx8txnr0vkq6vqeuu`
- Policy id: `886dcb2363e160c944e63cf544ce6f6265b22ef7c4e2478dd975078e`
- Our consumer config: [`deploy/preprod/ada-usd-preprod.example.yml`](deploy/preprod/ada-usd-preprod.example.yml)
- Reference used verbatim from the hackathon-resources repo, with the `wallet.mnemonic` field expanded from `$WALLET_MNEMONIC` at runtime so no secret ever lands on disk.

The demo pulls an aggregation live. If the pull fails (upstream contract state issues are common in the Preprod test environment), the demo consumes the most recent on-chain `AggState` from any aggregator and narrates the fallback explicitly.

## Quick start — run the full demo

Requirements: Rust (stable), a Preprod-funded Cardano wallet for the pool (`POOL_MNEMONIC`), and optionally a second for the Charli3 aggregator (`WALLET_MNEMONIC`). Both mnemonics are loaded via `read -s` — never touch disk, never appear in history. See [`scripts/preflight.sh`](scripts/preflight.sh) for environment checks.

```bash
# Offline rehearsal (no network, no settlement — useful to see the narrative)
scripts/demo.sh --dry --intro

# Full live demo, presenter-paced (ENTER between steps)
scripts/demo.sh -i --rotate --intro
```

Flags: `-i` interactive; `--intro` scope banner; `--rotate` policy-rotation stretch phase; `--dry` skip devnet + settlement.

## Demo scenarios (live, band-aware)

Both scenarios run against the same fixed policy and the same live Charli3 price. Request amounts are **sized to the band the oracle puts us in right now**, so the scenarios are meaningful at any market price.

- **Allow (request ≤ cap).** 80 % of the active-band cap. Expected verdict: `Authorized`. On allow, `scripts/cardano_disburse.py` submits a real Preprod transaction; the receiver's balance increases by the authorized amount; the tx id is printed with a cexplorer URL.
- **Deny (request > cap).** 110 % of the active-band cap. Expected verdict: `Denied(ReleaseCapExceeded, Grant)` — three canonical bytes. No Cardano transaction exists, by design.

Both decisions go through the same 4-node Ziranity BFT devnet; both produce canonical `AuthorizationResult` bytes; both are replayed by the offline verifier and confirmed CLEAN. The evidence bundles are assembled from live data at runtime — no placeholder fields.

## Verification

Any auditor can replay any recorded decision offline, without accessing the Ziranity network:

```bash
# Walk the 4 pre-recorded golden evidence bundles (regression test artifacts)
cargo run -p oracleguard-verifier --example verify_bundles --release

# Replay a single live-assembled bundle (what the demo does for its own runs)
cargo run -p oracleguard-adapter --example verify_live_bundle --release -- --help

# Show policy_ref derivation (rotate the cap, see the hash change)
cargo run -p oracleguard-schemas --example show_policy_ref --release -- --cap-bps 10000
```

Every check is mechanical: intent_id is recomputed from the canonical intent bytes; the gate/reason cross-field invariants are enforced; the evaluator is replayed and the decision bytes must match the recorded `AuthorizationResult` exactly. A clean verifier report means the decision is byte-for-byte reproducible.

## Public / private boundary

**Public (this repo):** canonical semantic types, canonical byte encoding, policy identity derivation, release-cap evaluator, three-gate closure, offline evidence verifier, adapter shell (Kupo fetch, Charli3 pull wrapper, Cardano disburse helper), demo harness, full fixture set.

**Private (Ziranity v1.1.0 handover bundle, tag `v1.1.0`):** runtime wiring of the three-gate closure into Ziranity's consensus hooks, Shadowbox projection wiring, the committed-decision path that turns `DisbursementIntentV1` bytes into committed `AuthorizationResult` bytes under a quorum certificate.

A reader of this repo alone can understand every semantic claim OracleGuard makes. A reader of this repo plus the handover bundle can run the full end-to-end live demo.

## Architecture deep-dive

- [`docs/architecture.md`](docs/architecture.md) — functional-core / imperative-shell, dependency direction, invariants
- [`docs/canonical-encoding.md`](docs/canonical-encoding.md) — postcard canonicalization rules, intent_id derivation
- [`docs/authorization-gates.md`](docs/authorization-gates.md) — three-gate closure, reason → gate partition
- [`docs/evaluator-contract.md`](docs/evaluator-contract.md) — pure evaluator contract, fixture-pinned behavior
- [`docs/verifier.md`](docs/verifier.md) — offline verifier surface, replay equivalence
- [`docs/ziranity-integration.md`](docs/ziranity-integration.md) — public/private boundary narrative

## Build

```
cargo build --workspace
cargo test --workspace
```

## Git hooks

After cloning:

```
git config core.hooksPath .githooks
```

The `pre-commit` hook scans staged content for secrets (mnemonics, Cardano signing keys, raw Ed25519 key bytes, bearer tokens). The `commit-msg` hook enforces commit-message hygiene.

## License

MIT. See [`LICENSE`](LICENSE).
