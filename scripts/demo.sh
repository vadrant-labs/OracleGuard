#!/usr/bin/env bash
# OracleGuard demo orchestrator.
#
# Drives the real end-to-end path, step by step. Every step shows the
# actual command that produces its result; default mode runs through
# automatically, -i/--interactive waits for the presenter to hit
# ENTER before running each command.
#
# Phases (each may be several steps):
#
#   1. policy      — show the policy JSON; derive policy_ref
#   2. balances    — query live Kupo for pool + receiver balances
#   3. oracle      — fetch the Charli3 AggState UTxO via Kupo
#   4. scenarios   — run smoke.sh allow + smoke.sh deny against a
#                    4-node Ziranity devnet
#   5. settle      — if allow passed, submit a real Cardano Preprod
#                    disbursement tx via scripts/cardano_disburse.py,
#                    then re-query the receiver's balance
#   6. verify      — replay recorded evidence bundles through the
#                    offline verifier
#   7. rotate      — (optional) show that raising the cap produces a
#                    new policy_ref
#
# USAGE
#   scripts/demo.sh                        # full live demo, auto-paced
#   scripts/demo.sh -i                     # interactive (wait on ENTER)
#   scripts/demo.sh --dry                  # no devnet, no settlement
#   scripts/demo.sh -i --dry --rotate      # rehearsal narration
#
# PREREQ (for live mode)
#   - ~/.local/opt/ziranity-v1.1.0-oracleguard-linux-x86_64/config/smoke.sh
#   - .venv/bin/python with pycardano   (see scripts/preflight.sh)
#   - POOL_MNEMONIC exported            (needed only for §5 settlement)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SMOKE="$HOME/.local/opt/ziranity-v1.1.0-oracleguard-linux-x86_64/config/smoke.sh"
VENV_PY="$REPO_ROOT/.venv/bin/python"

POOL_ADDR="addr_test1qz4f2vac8nn7tp802mxj3cu40a7xhhzc3agut6spq6rpz5rgtlvyed9yn3ncuv3fgaadfmvn64d7egjn824t7pj99xfs4y58d0"
RECEIVER_ADDR="addr_test1qq8wq0j9kpwkyf0tw9pa903r5wux9x6dneskyxanpt7v2w54ga88n5ff3553ugq29jyflcfmjau9e3qj093fmxw0hp7sht3w87"
RECEIVER_HEX="000ee03e45b05d6225eb7143d2be23a3b8629b4d9e61621bb30afcc53a95474e79d1298d291e200a2c889fe13b97785cc41279629d99cfb87d"
ORACLE_ADDR="addr_test1wq3pacs7jcrlwehpuy3ryj8kwvsqzjp9z6dpmx8txnr0vkq6vqeuu"
C3AS_SUFFIX="43334153"
OGMIOS_URL="http://35.209.192.203:1337"
KUPO_URL="http://35.209.192.203:1442"

INTERACTIVE=0
DRY=0
ROTATE=0
for a in "$@"; do
  case "$a" in
    -i|--interactive) INTERACTIVE=1 ;;
    --dry)            DRY=1 ;;
    --rotate)         ROTATE=1 ;;
    --help|-h)
      sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "unknown flag: $a" >&2; exit 2 ;;
  esac
done

STEP_NUM=0

# step <title> <description> <command>
# Displays the title, description, and verbatim command. In
# interactive mode, waits for ENTER before running and again after
# the output so the presenter can narrate. In auto mode, runs through.
step() {
  local title="$1"
  local desc="$2"
  local cmd="$3"
  STEP_NUM=$((STEP_NUM + 1))
  echo
  printf '═══ STEP %d — %s\n' "$STEP_NUM" "$title"
  printf '    %s\n' "$desc"
  # Print the command verbatim, indented. First line gets '$ ' prefix.
  printf '    $ %s\n' "$cmd" | awk 'NR==1{print; next} {print "        " $0}'
  if [ "$INTERACTIVE" = 1 ]; then
    read -r -p '    [ENTER to run] ' _ < /dev/tty
  fi
  echo   '    ─ output ─'
  set +e
  eval "$cmd" 2>&1 | sed 's/^/    /'
  local rc=${PIPESTATUS[0]}
  set -e
  if [ "$INTERACTIVE" = 1 ]; then
    read -r -p '    [ENTER for next step] ' _ < /dev/tty
  fi
  return $rc
}

skipped() {
  STEP_NUM=$((STEP_NUM + 1))
  echo
  printf '═══ STEP %d — %s  [SKIPPED]\n' "$STEP_NUM" "$1"
  printf '    %s\n' "$2"
}

# ==============================================================
# 1. POLICY
# ==============================================================

step "Show the policy document" \
     "The governance rules this disbursement is bound to." \
     "cat fixtures/policy_v1.json"

step "Derive policy_ref (sha256 over canonical bytes)" \
     "policy_ref is a 32-byte identity — anyone can reproduce it." \
     "sha256sum fixtures/policy_v1.canonical.bytes"

# ==============================================================
# 2. WALLET BALANCES (live Kupo)
# ==============================================================

step "Pool wallet balance (before)" \
     "Sum lovelace across every unspent UTxO at the pool address." \
     "curl -sS '$KUPO_URL/matches/$POOL_ADDR?unspent' \
  | python3 -c 'import json,sys; rs=json.load(sys.stdin); c=sum(r[\"value\"][\"coins\"] for r in rs); print(f\"{c/1_000_000:.6f} tADA  ({c} lovelace, {len(rs)} UTxOs)\")'"

# Snapshot receiver balance for the post-settlement delta.
RECEIVER_BEFORE=$(curl -sS --max-time 10 "$KUPO_URL/matches/$RECEIVER_ADDR?unspent" 2>/dev/null \
  | python3 -c "import json,sys;print(sum(r['value']['coins'] for r in json.load(sys.stdin)))" 2>/dev/null || echo "")

step "Receiver wallet balance (before)" \
     "Same query against the receiver address." \
     "curl -sS '$KUPO_URL/matches/$RECEIVER_ADDR?unspent' \
  | python3 -c 'import json,sys; rs=json.load(sys.stdin); c=sum(r[\"value\"][\"coins\"] for r in rs); print(f\"{c/1_000_000:.6f} tADA  ({c} lovelace, {len(rs)} UTxOs)\")'"

# ==============================================================
# 3. ORACLE (live fetch, narrative)
# ==============================================================

step "Fetch the Charli3 AggState UTxO" \
     "Locate the oracle UTxO holding the C3AS token (ADA/USD feed)." \
     "curl -sS '$KUPO_URL/matches/$ORACLE_ADDR?unspent' \
  | python3 -c 'import json,sys;
for r in json.load(sys.stdin):
    if any(k.endswith(\".$C3AS_SUFFIX\") for k in r[\"value\"][\"assets\"]):
        print(\"tx_id      :\", r[\"transaction_id\"])
        print(\"output_idx :\", r[\"output_index\"])
        print(\"datum_hash :\", r[\"datum_hash\"])
        break
else:
    print(\"no AggState UTxO found\")'"

# ==============================================================
# 4. SCENARIOS (Ziranity devnet consensus via smoke.sh)
# ==============================================================

ALLOW_OK=0
DENY_OK=0

if [ "$DRY" = 1 ] || [ ! -x "$SMOKE" ]; then
  reason="--dry flag"
  [ ! -x "$SMOKE" ] && reason="smoke.sh not found at $SMOKE"
  skipped "smoke.sh allow" "Ziranity devnet submit + byte-identity diff ($reason)"
  skipped "smoke.sh deny"  "Ziranity devnet submit + byte-identity diff ($reason)"
else
  if step "Scenario: allow (within cap)" \
          "Submit allow_700_ada fixture to a 4-node Ziranity devnet; expect PASS (committed output bytes match the recorded fixture)." \
          "'$SMOKE' allow"; then
    ALLOW_OK=1
  fi
  if step "Scenario: deny (over cap)" \
          "Submit deny_900_ada fixture; expect PASS with the 3-byte Denied(ReleaseCapExceeded) envelope." \
          "'$SMOKE' deny"; then
    DENY_OK=1
  fi
fi

# ==============================================================
# 5. CARDANO SETTLEMENT (allow path only)
# ==============================================================

TX_ID=""
if [ "$DRY" = 1 ] || [ "$ALLOW_OK" = 0 ]; then
  reason="--dry flag"
  [ "$DRY" = 0 ] && reason="allow scenario did not pass"
  skipped "Cardano settlement" "700 ADA disbursement tx ($reason)"
elif [ -z "${POOL_MNEMONIC:-}" ]; then
  skipped "Cardano settlement" "POOL_MNEMONIC not exported"
elif [ ! -x "$VENV_PY" ]; then
  skipped "Cardano settlement" ".venv/bin/python not found"
else
  step "Settle 700 ADA on Cardano Preprod" \
       "Shell to scripts/cardano_disburse.py; prints the tx_id on success." \
       "$VENV_PY scripts/cardano_disburse.py \\
  --ogmios-url $OGMIOS_URL \\
  --pool-address $POOL_ADDR \\
  --destination-bytes-hex $RECEIVER_HEX \\
  --destination-length 57 \\
  --amount-lovelace 700000000 \\
  --intent-id $(printf 'dd%.0s' {1..32}) \\
  | tee /tmp/og-settle.out | tail -1"
  TX_ID=$(tail -1 /tmp/og-settle.out 2>/dev/null || echo "")
  if ! [[ "$TX_ID" =~ ^[0-9a-f]{64}$ ]]; then
    TX_ID=""
  fi

  if [ -n "$TX_ID" ]; then
    step "Wait for Preprod confirmation" \
         "Rollback-safe depth is 2+ blocks (~40 s); we wait 25 s for one." \
         "sleep 25 && echo waited 25s"

    step "Receiver wallet balance (after)" \
         "Re-query the receiver's UTxOs — balance should be +700 tADA." \
         "curl -sS '$KUPO_URL/matches/$RECEIVER_ADDR?unspent' \
  | python3 -c 'import json,sys; rs=json.load(sys.stdin); c=sum(r[\"value\"][\"coins\"] for r in rs); print(f\"{c/1_000_000:.6f} tADA  ({c} lovelace, {len(rs)} UTxOs)\")'"

    if [ -n "$RECEIVER_BEFORE" ]; then
      RECEIVER_AFTER=$(curl -sS --max-time 10 "$KUPO_URL/matches/$RECEIVER_ADDR?unspent" 2>/dev/null \
        | python3 -c "import json,sys;print(sum(r['value']['coins'] for r in json.load(sys.stdin)))" 2>/dev/null || echo "")
      if [ -n "$RECEIVER_AFTER" ]; then
        DELTA=$(( RECEIVER_AFTER - RECEIVER_BEFORE ))
        step "Delta vs pre-settlement snapshot" \
             "Difference in lovelace between before and after." \
             "echo '+$((DELTA / 1000000)).$(printf '%06d' $((DELTA % 1000000))) tADA  ($DELTA lovelace)'"
      fi
    fi
  fi
fi

# ==============================================================
# 6. OFFLINE VERIFIER
# ==============================================================

step "Replay all recorded evidence bundles through the verifier" \
     "CLEAN means every recorded decision reproduces byte-for-byte." \
     "cargo test -p oracleguard-verifier verify_bundle_reports_clean --quiet 2>&1 | tail -10"

# ==============================================================
# 7. POLICY ROTATION (optional)
# ==============================================================

if [ "$ROTATE" = 1 ]; then
  step "Rotated policy_ref (cap 7500 → 10000)" \
       "Re-canonicalize the policy with a raised cap; a policy change is observable via a new 32-byte policy_ref." \
       "python3 -c '
import json, hashlib
rotated = {
    \"schema\": \"oracleguard.policy.v1\",
    \"policy_version\": 1,
    \"anchored_commitment\": \"katiba://policy/constitutional-release/v1\",
    \"release_cap_basis_points\": 10000,
    \"allowed_assets\": [\"ADA\"],
}
canon = json.dumps(rotated, sort_keys=True, separators=(\",\", \":\")).encode()
print(\"rotated canonical bytes:\", len(canon))
print(\"rotated policy_ref     :\", hashlib.sha256(canon).hexdigest())
print(\"original policy_ref    :\", open(\"fixtures/policy_v1.canonical.bytes\",\"rb\").read() and hashlib.sha256(open(\"fixtures/policy_v1.canonical.bytes\",\"rb\").read()).hexdigest())
'"
fi

# ==============================================================
# SUMMARY
# ==============================================================

echo
echo '═══ SUMMARY'
if [ "$DRY" = 1 ]; then
  echo "    mode        : DRY (no devnet, no settlement)"
else
  echo "    allow smoke : $([ "$ALLOW_OK" = 1 ] && echo PASS || echo 'FAIL/SKIPPED')"
  echo "    deny  smoke : $([ "$DENY_OK"  = 1 ] && echo PASS || echo 'FAIL/SKIPPED')"
  [ -n "$TX_ID" ] && echo "    cardano tx  : $TX_ID" && \
                     echo "    cexplorer   : https://preprod.cexplorer.io/tx/$TX_ID"
fi
echo "    verifier    : CLEAN"
echo
