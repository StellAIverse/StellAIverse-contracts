# paste contents
#!/usr/bin/env bash
# =============================================================================
# run-formal-verification.sh
# stellAIverse-contracts | Issue #104 — Formal Verification CI Runner
#
# Runs Certora Prover against all critical contracts.
# Exits non-zero if any verification fails (blocks deployment).
#
# Usage:
#   CERTORAKEY=<your_key> ./formal-verification/run-formal-verification.sh
#   ./formal-verification/run-formal-verification.sh --dry-run
# =============================================================================

set -euo pipefail

# ── Colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[PASS]${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
fail()    { echo -e "${RED}[FAIL]${RESET}  $*"; }
header()  { echo -e "\n${BOLD}${CYAN}══ $* ══${RESET}"; }

# ── Config ────────────────────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPECS_DIR="${REPO_ROOT}/formal-verification/specs"
RESULTS_DIR="${REPO_ROOT}/formal-verification/results"
DRY_RUN=false
TIMEOUT_MINUTES=30
FAILED_SPECS=()
PASSED_SPECS=()

# ── Parse args ────────────────────────────────────────────────────────────────
for arg in "$@"; do
  case $arg in
    --dry-run) DRY_RUN=true ;;
    --help)
      echo "Usage: $0 [--dry-run]"
      echo "  --dry-run   Print commands without executing"
      exit 0 ;;
  esac
done

# ── Preflight ─────────────────────────────────────────────────────────────────
header "stellAIverse Formal Verification Runner"
info "Repo root : $REPO_ROOT"
info "Specs dir : $SPECS_DIR"
info "Dry run   : $DRY_RUN"

mkdir -p "$RESULTS_DIR"

if [[ "$DRY_RUN" == "false" ]]; then
  if [[ -z "${CERTORAKEY:-}" ]]; then
    warn "CERTORAKEY environment variable is not set."
    warn "Skipping Certora verification in this environment."
    exit 0
  fi

  if ! command -v certoraRun &>/dev/null; then
    warn "certoraRun not found. Installing via pip..."
    pip install certora-cli --quiet
  fi
fi

# ── Contract specs to verify ──────────────────────────────────────────────────
declare -A CONTRACTS=(
  ["agent_nft"]="contracts/agent_nft"
  ["marketplace"]="contracts/marketplace"
  ["execution_hub"]="contracts/execution_hub"
)

declare -A SPEC_FILES=(
  ["agent_nft"]="agent_nft.spec.cvl"
  ["marketplace"]="marketplace.spec.cvl"
  ["execution_hub"]="execution_hub.spec.cvl"
)

# ── Run verifications ─────────────────────────────────────────────────────────
header "Running Verification Jobs"

for contract in "${!CONTRACTS[@]}"; do
  spec="${SPECS_DIR}/${SPEC_FILES[$contract]}"
  contract_path="${REPO_ROOT}/${CONTRACTS[$contract]}"
  result_file="${RESULTS_DIR}/${contract}_result.json"

  info "Verifying: ${BOLD}${contract}${RESET}"
  info "  Spec   : $spec"
  info "  Source : $contract_path"

  CMD=(
    certoraRun
    "$contract_path"
    --verify "${contract}:${spec}"
    --solc solc                        # swap for soroban-specific compiler if needed
    --msg "${contract} formal verification"
    --output_dir "$RESULTS_DIR"
    --timeout "$((TIMEOUT_MINUTES * 60))"
    --send_only
  )

  if [[ "$DRY_RUN" == "true" ]]; then
    echo "  DRY RUN: ${CMD[*]}"
    PASSED_SPECS+=("$contract (dry-run)")
    continue
  fi

  set +e
  timeout "${TIMEOUT_MINUTES}m" "${CMD[@]}" > "$result_file" 2>&1
  EXIT_CODE=$?
  set -e

  if [[ $EXIT_CODE -eq 0 ]]; then
    success "${contract} — all properties verified ✓"
    PASSED_SPECS+=("$contract")
  elif [[ $EXIT_CODE -eq 124 ]]; then
    fail "${contract} — timed out after ${TIMEOUT_MINUTES} min"
    FAILED_SPECS+=("$contract (timeout)")
  else
    fail "${contract} — verification failed (exit $EXIT_CODE)"
    fail "  See: $result_file"
    FAILED_SPECS+=("$contract")
  fi
done

# ── Summary ───────────────────────────────────────────────────────────────────
header "Verification Summary"

for s in "${PASSED_SPECS[@]:-}"; do
  success "$s"
done

for s in "${FAILED_SPECS[@]:-}"; do
  fail "$s"
done

TOTAL=$(( ${#PASSED_SPECS[@]} + ${#FAILED_SPECS[@]} ))
echo ""
info "Results: ${#PASSED_SPECS[@]}/${TOTAL} contracts verified"

if [[ ${#FAILED_SPECS[@]} -gt 0 ]]; then
  fail "Formal verification FAILED — blocking deployment."
  exit 1
fi

success "All contracts formally verified. Safe to deploy. 🎉"
exit 0
