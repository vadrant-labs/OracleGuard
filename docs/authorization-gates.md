# Authorization Gates

This document is the authoritative public description of OracleGuard's
three-gate authorization closure. If the code or a docstring drifts
from this document, the drift is the bug — not the document.

## Gate order

Gates run in a fixed order:

1. **Anchor gate** — validates policy identity and intent schema
   version.
2. **Registry gate** — validates allocation identity, subject
   authorization, asset match, and destination approval.
3. **Grant gate** — validates oracle freshness and delegates the
   release-cap decision to the pure evaluator
   `oracleguard_policy::evaluate::evaluate_disbursement`.

The first failing gate produces the canonical denial reason. Later
gates MUST NOT be consulted once an earlier gate denies. This rule is
mechanical, not stylistic: runtime ordering must not leak into
authorization meaning.

## Reason-to-gate partition

Every `DisbursementReasonCode` is produced by exactly one gate. The
partition is mirrored by the const method
`DisbursementReasonCode::gate()`:

| Reason code            | Gate      |
|------------------------|-----------|
| `PolicyNotFound`       | anchor    |
| `AllocationNotFound`   | registry  |
| `SubjectNotAuthorized` | registry  |
| `AssetMismatch`        | registry  |
| `OracleStale`          | grant     |
| `OraclePriceZero`      | grant     |
| `AmountZero`           | grant     |
| `ReleaseCapExceeded`   | grant     |

## Within-gate check order

Each gate evaluates checks in a documented order. When multiple
conditions are invalid, the check that runs first produces the denial.
This within-gate precedence is just as mechanical as the cross-gate
order.

### Anchor gate

1. `intent.intent_version != INTENT_VERSION_V1` → `PolicyNotFound`.
2. `!anchor.is_policy_registered(&intent.policy_ref)` → `PolicyNotFound`.

Both conditions map to the same reason code because v1 treats
"unsupported schema version" and "unregistered policy identity" as
facets of the same anchor-gate failure: the intent cannot be anchored
to a known policy at all.

### Registry gate

1. `registry.lookup(&intent.allocation_id) == None` →
   `AllocationNotFound`.
2. `intent.requester_id` not in `authorized_subjects` →
   `SubjectNotAuthorized`.
3. `intent.asset != approved_asset` (byte-identical comparison) →
   `AssetMismatch`.
4. `intent.destination` not in `approved_destinations` (byte-identical
   comparison including `length`) → `SubjectNotAuthorized`.

**v1 destination-mapping compatibility.** Step 4 uses
`SubjectNotAuthorized` for destination mismatch because the closed
eight-variant `DisbursementReasonCode` set has no dedicated
destination code. Semantically the check is "the subject is not
authorized to direct this allocation to that destination." This is a
deliberate v1 compatibility mapping, not the ideal long-run taxonomy;
a dedicated destination reason code would be a canonical-byte
version-boundary change and requires Katiba sign-off. Consumers MUST
treat `SubjectNotAuthorized` as covering both sub-causes without
distinguishing them.

### Grant gate

1. `is_oracle_fresh(&intent.oracle_provenance, now_unix_ms) == Err(_)`
   → `OracleStale`.
2. `intent.requested_amount_lovelace == 0` → `AmountZero`.
3. `validate_oracle_fact_eval(&intent.oracle_fact) != Ok(())` →
   `OraclePriceZero`.
4. `evaluate_disbursement(intent, basis_lovelace)`:
   - `Allow { authorized_amount_lovelace, release_cap_basis_points }`
     → produce the authorized effect.
   - `Deny { reason }` → propagate `reason` (one of `AmountZero`,
     `OraclePriceZero`, `ReleaseCapExceeded`; by this point steps 2
     and 3 have already decided on the first two, so the reachable
     evaluator deny is `ReleaseCapExceeded`).

The `AmountZero → OraclePriceZero → ReleaseCapExceeded` suffix is the
same order the evaluator already runs internally; the grant gate
inherits it by construction. Freshness runs first so a stale datum
cannot feed the evaluator.

The grant gate MUST NOT duplicate release-cap arithmetic. All band
selection, cap computation, and cap comparison are the evaluator's
responsibility (`oracleguard_policy::math` + `evaluate_disbursement`).

## Deny/no-effect closure

Only a successful three-gate authorization may produce an
`AuthorizedEffectV1`. A denial carries a reason code and a gate tag;
it carries no effect payload. This is structurally enforced by the
shape of `AuthorizationResult` — the `Denied` variant has no effect
field, so emitting a denied result with an effect is not expressible.
Cluster 5 Slice 06 pins the behavioral tests for this closure.

Downstream layers (routing, submission, settlement, evidence) MUST
consume `AuthorizationResult::Authorized { effect }` as the single
source of truth. They MUST NOT re-run any gate, re-derive any reason,
or "fix up" an incomplete authorization decision. The closure ends at
the authorization surface.

## Determinism

Every gate is pure with respect to its inputs:

- Anchor: `(intent, anchor view)` → `Result<(), reason>`.
- Registry: `(intent, registry view)` → `Result<basis_lovelace, reason>`.
- Grant: `(intent, basis_lovelace, now_unix_ms)` →
  `Result<AuthorizedEffectV1, reason>`.

None of them perform I/O, read a wall clock, or allocate for I/O.
Identical inputs produce byte-identical outputs across runs. The
top-level `authorize_disbursement` composes these three pure gates.

## Cross-references

- Reason codes: `oracleguard_schemas::reason::DisbursementReasonCode`.
- Gates: `oracleguard_schemas::gate::AuthorizationGate`.
- Authorized effect: `oracleguard_schemas::effect::AuthorizedEffectV1`.
- Result type and traits:
  `oracleguard_policy::authorize::{AuthorizationResult, PolicyAnchorView,
  AllocationRegistryView, AllocationFacts}`.
- Evaluator contract: `docs/evaluator-contract.md`.
- Canonical byte encoding: `docs/canonical-encoding.md`.
