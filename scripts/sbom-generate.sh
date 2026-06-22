#!/usr/bin/env bash
# =============================================================================
# sbom-generate.sh — generate a deterministic CycloneDX 1.4 SBOM for signal-kit
# =============================================================================
#
# Generates a CycloneDX JSON SBOM using the official `cargo-cyclonedx` tooling,
# with the SAME feature selection, target scope, lockfile and revision as the
# corresponding build, and byte-reproducible for a given commit (SOURCE_DATE_EPOCH
# → cargo-cyclonedx emits a stable document with no random serial number).
#
# For releases, pass --manifest-path pointing at the EXTRACTED packaged crate's
# Cargo.toml (with the repo's Cargo.lock copied in). This makes the packaged
# .crate — not the workspace checkout — the source of truth for the SBOM, so the
# SBOM corresponds to exactly what users download. Locally/CI-PR the default
# (workspace member manifest) is used.
#
# Output filename (deterministic, unambiguous):
#     signal-kit-<version>-<config>.cdx.json
#
# Usage:
#   sbom-generate.sh --config <name>
#       [--manifest-path <path>]   (default: crates/signal-kit/Cargo.toml)
#       [--out-dir <dir>]          (default: target/sbom)
#       [--features "<f1 f2>"] | [--all-features] | [--no-default-features]
#       [--target <triple|all>]    (default: host target)
#
# Prints the absolute path of the generated SBOM. Exits non-zero on failure.
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

PRODUCT="signal-kit"

CONFIG=""
MANIFEST_PATH="$(repo_root)/crates/signal-kit/Cargo.toml"
OUT_DIR="$(repo_root)/target/sbom"
FEATURES_FLAG=()
TARGET_FLAG=()

usage() { sed -n 's/^# \{0,1\}//p' "${BASH_SOURCE[0]}" | sed -n '3,/^=====/p' | head -n -1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --config)        [ $# -ge 2 ] || die "--config requires a value"; CONFIG="$2"; shift 2 ;;
        --manifest-path) [ $# -ge 2 ] || die "--manifest-path requires a value"; MANIFEST_PATH="$2"; shift 2 ;;
        --out-dir)       [ $# -ge 2 ] || die "--out-dir requires a value"; OUT_DIR="$2"; shift 2 ;;
        --features)      [ $# -ge 2 ] || die "--features requires a value"; FEATURES_FLAG=(--features "$2"); shift 2 ;;
        --all-features)        FEATURES_FLAG=(--all-features); shift ;;
        --no-default-features) FEATURES_FLAG=(--no-default-features); shift ;;
        --target)        [ $# -ge 2 ] || die "--target requires a value"; TARGET_FLAG=(--target "$2"); shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done

[ -n "$CONFIG" ] || die "--config <name> is required (used in the output filename)"
echo "$CONFIG" | grep -Eq '^[a-z0-9][a-z0-9-]*$' \
    || die "--config must be lowercase kebab/digits (got: '$CONFIG')"
[ -f "$MANIFEST_PATH" ] || die "manifest not found: $MANIFEST_PATH"

require_cmd cargo
require_cargo_subcommand cyclonedx
require_cmd jq

CRATE_DIR="$(cd "$(dirname "$MANIFEST_PATH")" && pwd)"
VERSION="$(cargo metadata --no-deps --format-version 1 --manifest-path "$MANIFEST_PATH" \
            | jq -r '.packages[] | select(.name=="signal-kit") | .version')"
[ -n "$VERSION" ] || die "could not resolve signal-kit version from $MANIFEST_PATH"

BASE="${PRODUCT}-${VERSION}-${CONFIG}"
STAGING_PATH="${CRATE_DIR}/${BASE}.cdx.json"
mkdir -p "$OUT_DIR"
OUT_PATH="$(cd "$OUT_DIR" && pwd)/${BASE}.cdx.json"

rm -f "$STAGING_PATH"
trap 'rm -f "$STAGING_PATH"' EXIT

log "generating CycloneDX 1.4 SBOM: ${BASE} (features=${FEATURES_FLAG[*]:-default}, target=${TARGET_FLAG[*]:-host}, manifest=$MANIFEST_PATH)"

SOURCE_DATE_EPOCH="$(commit_epoch)" \
    cargo cyclonedx \
        --manifest-path "$MANIFEST_PATH" \
        --format json \
        --spec-version 1.4 \
        --override-filename "${BASE}.cdx" \
        "${FEATURES_FLAG[@]}" \
        "${TARGET_FLAG[@]}" \
        -q

[ -f "$STAGING_PATH" ] || die "expected SBOM was not produced at $STAGING_PATH"
jq empty "$STAGING_PATH" || die "generated SBOM is not valid JSON: $STAGING_PATH"

mv "$STAGING_PATH" "$OUT_PATH"
trap - EXIT

DESCRIBED="$(jq -r '.metadata.component.name' "$OUT_PATH")"
[ "$DESCRIBED" = "$PRODUCT" ] \
    || die "SBOM metadata.component.name ('$DESCRIBED') != '$PRODUCT'"

COMPONENTS="$(jq '.components | length' "$OUT_PATH")"
log "wrote $OUT_PATH ($COMPONENTS components)"
echo "$OUT_PATH"
