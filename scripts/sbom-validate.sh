#!/usr/bin/env bash
# =============================================================================
# sbom-validate.sh — validate a CycloneDX JSON document
# =============================================================================
#
# Validates a CycloneDX SBOM in two layers:
#
#   1. Structural (always): the file is valid JSON, bomFormat == "CycloneDX",
#      specVersion is one of 1.3/1.4/1.5, metadata.component is present, and
#      components is an array. This catches gross corruption with zero extra
#      dependencies (only jq).
#
#   2. Schema (when available): if $CYCLONEDX_CLI points to the official
#      CycloneDX CLI binary (installed by scripts/fetch-cyclonedx-cli.sh in CI),
#      it validates the document against the CycloneDX JSON schema with
#      --fail-on-errors. This is the authoritative check.
#
# A malformed or invalid SBOM fails (non-zero exit) — it must never be published.
#
# Usage: sbom-validate.sh <path-to.cdx.json>
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

[ $# -eq 1 ] || die "usage: sbom-validate.sh <path-to.cdx.json>"
SBOM="$1"
[ -f "$SBOM" ] || die "SBOM file not found: $SBOM"
require_cmd jq

errors=()

# ---- layer 1: structural checks via jq --------------------------------------
struct_check() {
    local label="$1" expr="$2" expect="$3"
    local got
    if ! got="$(jq -r "$expr" "$SBOM" 2>/dev/null)"; then
        errors+=("not valid JSON (jq parse failed)")
        return
    fi
    if [ "$got" != "$expect" ]; then
        errors+=("$label: expected '$expect', got '$got'")
    fi
}

struct_check "bomFormat"      '.bomFormat'                 "CycloneDX"
# specVersion allow-list (allow 1.3/1.4/1.5; cargo-cyclonedx 0.5 emits 1.5).
SV="$(jq -r '.specVersion' "$SBOM" 2>/dev/null || echo "")"
case "$SV" in
    1.3|1.4|1.5) ;;
    *) errors+=("specVersion: expected one of 1.3/1.4/1.5, got '$SV'") ;;
esac

# metadata.component must exist and be named.
COMPONENT_NAME="$(jq -r '.metadata.component.name // ""' "$SBOM" 2>/dev/null || echo "")"
[ -n "$COMPONENT_NAME" ] || errors+=("metadata.component.name is missing")

# components must be an array (may be empty for a degenerate BOM, but must exist).
COMP_TYPE="$(jq -r '.components | type' "$SBOM" 2>/dev/null || echo "")"
[ "$COMP_TYPE" = "array" ] || errors+=(".components is not an array (got '$COMP_TYPE')")

# ---- layer 2: authoritative schema validation via CycloneDX CLI -------------
if [ -n "${CYCLONEDX_CLI:-}" ] && [ -x "$CYCLONEDX_CLI" ]; then
    # Map specVersion (e.g. "1.5") to the CLI's input-version token (v1_5).
    INPUT_VERSION="v1_$(printf '%s' "$SV" | cut -d. -f2)"
    log "schema-validating with CycloneDX CLI ($(basename "$CYCLONEDX_CLI")) @ $INPUT_VERSION"
    if ! "$CYCLONEDX_CLI" validate \
            --input-file "$SBOM" \
            --input-format json \
            --input-version "$INPUT_VERSION" \
            --fail-on-errors; then
        errors+=("CycloneDX CLI schema validation reported errors")
    fi
else
    log "CycloneDX CLI not available (set CYCLONEDX_CLI for schema validation); structural checks only"
fi

# ---- report -----------------------------------------------------------------
if [ "${#errors[@]}" -gt 0 ]; then
    echo "SBOM validation FAILED for: $SBOM" >&2
    printf '  - %s\n' "${errors[@]}" >&2
    exit 1
fi

COMPONENTS="$(jq '.components | length' "$SBOM")"
log "SBOM valid: $SBOM (bomFormat=CycloneDX, specVersion=$SV, $COMPONENTS components)"
