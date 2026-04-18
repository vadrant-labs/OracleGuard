#!/usr/bin/env bash
# OracleGuard demo-day preflight check.
#
# Runs every non-interactive prerequisite for a live-loop demo:
#   - Python venv present with pycardano + ogmios + charli3 installed
#   - charli3 CLI binary resolves
#   - ziranity CLI binary resolves (optional — only needed if Ziranity
#     submit is part of the demo path)
#   - Hackathon Ogmios + Kupo endpoints reachable
#   - Operator config file exists
#
# Explicitly does NOT check: mnemonics (operator-only; revealed at
# run time, never persisted). If WALLET_MNEMONIC or POOL_MNEMONIC
# are set in the current shell, that's flagged but not required.
#
# Exit status:
#   0   all checks passed — safe to proceed with the dry run
#   1   one or more required checks failed — do NOT proceed
#   2   optional checks failed — can proceed but with reduced tier
#
# Usage:
#   ./scripts/preflight.sh
#   ./scripts/preflight.sh --verbose     # print full diagnostic output

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_PY="${REPO_ROOT}/.venv/bin/python"
DEPLOY_CONFIG="${REPO_ROOT}/deploy/preprod/ada-usd-preprod.example.yml"
NODE_CONFIG="${REPO_ROOT}/integrations/ziranity/fixtures/oracleguard-node.sample.toml"
OGMIOS_URL="http://35.209.192.203:1337"
KUPO_URL="http://35.209.192.203:1442"

VERBOSE=0
if [[ "${1:-}" == "--verbose" ]]; then
  VERBOSE=1
fi

# --- output helpers ------------------------------------------------

GREEN="\033[32m"
RED="\033[31m"
YELLOW="\033[33m"
RESET="\033[0m"

pass() { printf "  ${GREEN}✓${RESET}  %s\n" "$1"; }
fail() { printf "  ${RED}✗${RESET}  %s\n" "$1"; }
warn() { printf "  ${YELLOW}!${RESET}  %s\n" "$1"; }
info() { printf "      %s\n" "$1"; }

verbose() {
  if [[ "${VERBOSE}" -eq 1 ]]; then
    printf "      %s\n" "$1"
  fi
}

# --- check counters ------------------------------------------------

required_failed=0
optional_failed=0

# ============================================================
# Section 1 — Python venv + operator deps
# ============================================================

echo
echo "== Python venv + deps =="

if [[ -x "${VENV_PY}" ]]; then
  pass "venv present at .venv/"
  verbose "$("${VENV_PY}" --version 2>&1)"
else
  fail "venv missing at .venv/ — run: virtualenv -p python3.10 .venv"
  required_failed=$((required_failed + 1))
fi

if [[ -x "${VENV_PY}" ]]; then
  for pkg in pycardano ogmios; do
    if "${VENV_PY}" -c "import ${pkg}" >/dev/null 2>&1; then
      pass "python package: ${pkg}"
    else
      fail "python package: ${pkg} (run: .venv/bin/pip install ${pkg})"
      required_failed=$((required_failed + 1))
    fi
  done

  if "${VENV_PY}" -c "from pycardano.backend.ogmios_v6 import OgmiosV6ChainContext" >/dev/null 2>&1; then
    pass "pycardano Ogmios v6 backend importable"
  else
    fail "pycardano Ogmios v6 backend import failed"
    required_failed=$((required_failed + 1))
  fi
fi

# ============================================================
# Section 2 — CLI binaries
# ============================================================

echo
echo "== CLI binaries =="

# charli3 can live either in the venv or on system PATH.
if [[ -x "${REPO_ROOT}/.venv/bin/charli3" ]]; then
  pass "charli3 binary: .venv/bin/charli3"
elif command -v charli3 >/dev/null 2>&1; then
  pass "charli3 binary: $(command -v charli3) (system PATH)"
else
  fail "charli3 binary not found (install: .venv/bin/pip install git+https://github.com/Charli3-Official/charli3-pull-oracle-client.git)"
  required_failed=$((required_failed + 1))
fi

if command -v ziranity >/dev/null 2>&1; then
  pass "ziranity binary: $(command -v ziranity)"
else
  warn "ziranity binary not found on PATH — Ziranity-submit demo tier unavailable"
  warn "  (build from the ziranity-v3 repo: cargo build --release -p ziranity_cli)"
  optional_failed=$((optional_failed + 1))
fi

if command -v cargo >/dev/null 2>&1; then
  pass "cargo available"
  verbose "$(cargo --version)"
else
  warn "cargo not found — Rust-side unit tests and examples unavailable"
  optional_failed=$((optional_failed + 1))
fi

# ============================================================
# Section 3 — Hackathon endpoints
# ============================================================

echo
echo "== Hackathon endpoints =="

if curl -sS --max-time 5 "${OGMIOS_URL}/health" >/dev/null 2>&1; then
  pass "Ogmios reachable: ${OGMIOS_URL}"
  if [[ "${VERBOSE}" -eq 1 ]]; then
    curl -sS --max-time 5 "${OGMIOS_URL}/health" \
      | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'      version: {d.get(\"version\")} network: {d.get(\"network\")}')" 2>/dev/null || true
  fi
else
  fail "Ogmios unreachable: ${OGMIOS_URL}"
  required_failed=$((required_failed + 1))
fi

if curl -sS --max-time 5 -o /dev/null -w "%{http_code}" "${KUPO_URL}/health" 2>/dev/null | grep -q "^200$"; then
  pass "Kupo reachable: ${KUPO_URL}"
else
  fail "Kupo unreachable: ${KUPO_URL}"
  required_failed=$((required_failed + 1))
fi

# ============================================================
# Section 4 — Config files
# ============================================================

echo
echo "== Config files =="

if [[ -f "${DEPLOY_CONFIG}" ]]; then
  pass "Charli3 config: ${DEPLOY_CONFIG}"
  if grep -q '"\$WALLET_MNEMONIC"' "${DEPLOY_CONFIG}"; then
    pass "  uses \$WALLET_MNEMONIC env-var expansion (no secret in file)"
  else
    warn "  does not use \$WALLET_MNEMONIC expansion — verify no mnemonic inline"
    optional_failed=$((optional_failed + 1))
  fi
else
  fail "Charli3 config missing: ${DEPLOY_CONFIG}"
  required_failed=$((required_failed + 1))
fi

if [[ -f "${NODE_CONFIG}" ]]; then
  pass "Ziranity node config: ${NODE_CONFIG}"
else
  warn "Ziranity node config missing: ${NODE_CONFIG} — Ziranity-submit tier unavailable"
  optional_failed=$((optional_failed + 1))
fi

# ============================================================
# Section 5 — Mnemonic env vars (informational only)
# ============================================================

echo
echo "== Mnemonic env vars (informational) =="

if [[ -n "${WALLET_MNEMONIC:-}" ]]; then
  pass "WALLET_MNEMONIC set ($(echo "${WALLET_MNEMONIC}" | wc -w) words)"
else
  info "WALLET_MNEMONIC not set — reveal from Eternl before running 'charli3 aggregate'"
fi

if [[ -n "${POOL_MNEMONIC:-}" ]]; then
  pass "POOL_MNEMONIC set ($(echo "${POOL_MNEMONIC}" | wc -w) words)"
else
  info "POOL_MNEMONIC not set — reveal from Eternl before running the disburse helper"
fi

if [[ -n "${ZIRANITY_GENESIS_TIME_MS:-}" ]]; then
  pass "ZIRANITY_GENESIS_TIME_MS set (${ZIRANITY_GENESIS_TIME_MS})"
else
  info "ZIRANITY_GENESIS_TIME_MS not set — required by ziranity-node if building with --features oracleguard"
fi

# ============================================================
# Summary
# ============================================================

echo
echo "== Summary =="

if [[ ${required_failed} -eq 0 && ${optional_failed} -eq 0 ]]; then
  printf "  ${GREEN}All checks passed.${RESET} Safe to proceed with the full-loop dry run.\n\n"
  exit 0
fi

if [[ ${required_failed} -eq 0 ]]; then
  printf "  ${YELLOW}Required checks passed; ${optional_failed} optional check(s) failed.${RESET}\n"
  printf "  Safe to proceed at reduced demo tier. See individual messages above.\n\n"
  exit 2
fi

printf "  ${RED}${required_failed} required check(s) failed; ${optional_failed} optional check(s) failed.${RESET}\n"
printf "  Do NOT proceed with the dry run until required checks pass.\n\n"
exit 1
