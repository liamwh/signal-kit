#!/usr/bin/env bash
# =============================================================================
# fetch-cyclonedx-cli.sh — install the official CycloneDX CLI (validator)
# =============================================================================
#
# Downloads the official CycloneDX CLI native binary for the current platform
# from the CycloneDX GitHub release over HTTPS, verifies it against a pinned
# SHA-256 checksum, makes it executable and prints its path. No opaque curl-to-
# shell installer, no mutable tag, no unverified binary.
#
# The version and per-platform checksums are pinned HERE (single source of
# truth). Bump the version and update both checksums together, after verifying
# them against the release at:
#   https://github.com/CycloneDX/cyclonedx-cli/releases
#
# Usage: fetch-cyclonedx-cli.sh [--install-dir <dir>]   (default: a temp dir)
# Prints the absolute path of the verified binary.
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

VERSION="0.32.0"
BASE_URL="https://github.com/CycloneDX/cyclonedx-cli/releases/download/v${VERSION}"

# Pinned SHA-256 checksums per release asset (verified against the v0.32.0
# release). Only the platforms this project actually uses are listed; running
# on another platform fails loudly with "no pinned SHA-256" rather than
# downloading an unverified binary. To add a platform, verify its checksum
# against the release and append it here.
declare -A SHA256=(
    ["linux-x64"]="454879e6a4a405c8a13bff49b8982adcb0596f3019b26b0811c66e4d7f0783e1"  # CI (ubuntu-latest)
    ["osx-arm64"]="83be8a9599f1dce1252208bd4d0bb15308eca0546814fb72b48b7246d35e832e"  # local dev (Apple Silicon)
)

INSTALL_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --install-dir) [ $# -ge 2 ] || die "--install-dir requires a value"; INSTALL_DIR="$2"; shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "${BASH_SOURCE[0]}" | sed -n '3,/^=====/p' | head -n -1; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
[ -z "$INSTALL_DIR" ] && INSTALL_DIR="$(mktemp -d)"
mkdir -p "$INSTALL_DIR"

require_cmd curl
require_cmd shasum

# Map runner platform (uname) to a CycloneDX release asset key.
detect_asset_key() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux)  os="linux" ;;
        Darwin) os="osx"   ;;
        *) die "unsupported OS for CycloneDX CLI: $os" ;;
    esac
    case "$arch" in
        x86_64|amd64) arch="x64" ;;
        aarch64|arm64) arch="arm64" ;;
        *) die "unsupported architecture for CycloneDX CLI: $arch" ;;
    esac
    echo "${os}-${arch}"
}

ASSET_KEY="$(detect_asset_key)"
EXPECTED="${SHA256[$ASSET_KEY]:-}"
[ -n "$EXPECTED" ] || die "no pinned SHA-256 for asset '$ASSET_KEY' (add it to $0)"

# detect_asset_key() already rejects non-Linux/Darwin OSes, so every supported
# asset key maps directly to a binary name with no extension.
BINARY="cyclonedx-${ASSET_KEY}"
URL="${BASE_URL}/${BINARY}"
DEST="${INSTALL_DIR}/${BINARY}"

log "downloading CycloneDX CLI v${VERSION} (${ASSET_KEY}) from ${URL}"
curl --fail --location --retry 3 --retry-delay 2 --output "$DEST" "$URL"

# Verify the download against the pinned checksum before executing it.
ACTUAL="$(shasum -a 256 "$DEST" | awk '{print $1}')"
[ "$ACTUAL" = "$EXPECTED" ] \
    || die "CycloneDX CLI checksum mismatch for ${ASSET_KEY}:
  expected: $EXPECTED
  actual:   $ACTUAL
Refusing to execute an unverified binary."

chmod +x "$DEST"

log "CycloneDX CLI v${VERSION} verified and installed: $DEST"
echo "$DEST"
