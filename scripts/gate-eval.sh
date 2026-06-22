#!/usr/bin/env bash
# =============================================================================
# gate-eval.sh — evaluate the supply-chain gate from predecessor job results
# =============================================================================
#
# Centralises the gate logic so the workflow's final `gate` job and the local
# self-test use identical rules. Required-branch-protection check name:
#   "Supply chain / Gate (required)".
#
# Rules:
#   * detect must be success;
#   * scan   must be success (the always-on core gate: deny + audit + selftest);
#   * sbom   must be success OR skipped (intentionally skipped when no
#     dependency-relevant files changed).
# Any other combination fails the gate.
#
# Usage: gate-eval.sh --detect <result> --scan <result> --sbom <result>
# Exits 0 on pass, 1 on fail.
# =============================================================================
set -uo pipefail

DETECT="" SCAN="" SBOM=""
while [ $# -gt 0 ]; do
    case "$1" in
        --detect) [ $# -ge 2 ] || die "--detect requires a value"; DETECT="$2"; shift 2 ;;
        --scan)   [ $# -ge 2 ] || die "--scan requires a value";   SCAN="$2";   shift 2 ;;
        --sbom)   [ $# -ge 2 ] || die "--sbom requires a value";   SBOM="$2";   shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "${BASH_SOURCE[0]}" | sed -n '3,/^=====/p' | head -n -1; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

fail=0
reason=""
[ "$DETECT" = "success" ] || { reason="detect ($DETECT)"; fail=1; }
[ "$SCAN" = "success" ]   || { reason="$reason scan ($SCAN)"; fail=1; }
case "$SBOM" in
    success|skipped) ;;
    *) reason="$reason sbom ($SBOM)"; fail=1 ;;
esac

if [ "$fail" -ne 0 ]; then
    echo "::error::Supply-chain gate failed:$reason"
    echo "Gate FAILED:$reason" >&2
    exit 1
fi
echo "Gate passed: detect=$DETECT scan=$SCAN sbom=$SBOM."
