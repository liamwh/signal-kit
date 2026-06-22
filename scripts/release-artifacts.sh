#!/usr/bin/env bash
# =============================================================================
# release-artifacts.sh — assemble & checksum signal-kit release artefacts
# =============================================================================
#
# The packaged .crate is the single source of truth for a release. This script:
#
#   1. packages the exact source users download with `cargo package --locked`
#      (the lockfile is the release dependency resolution and must not change);
#   2. extracts the .crate into a clean temporary directory and validates its
#      contents (name, version, no private/unexpected/repository-only files);
#   3. runs `cargo metadata` and compiles the extracted package independently
#      with the release lockfile (auditable build only if a binary/cdylib target
#      exists — a source-only library has nothing to embed);
#   4. generates the CycloneDX SBOMs FROM the packaged manifest + release
#      lockfile, so the SBOM corresponds to exactly what is shipped;
#   5. writes SHA256SUMS over every asset and verifies it.
#
# Nothing is rebuilt after it is checksummed — the file this script produces is
# the file that gets uploaded, attested and (in CI) downloaded back to verify.
#
# Usage: release-artifacts.sh --out-dir <dir>
#   [--sbom-config <name>:<feature-args> ...]   (see default set below)
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

OUT_DIR=""
# SBOM configurations: the default-feature reference resolution, and the
# maximal (all-features, all-targets) superset. Each entry is
# "<config-name>:<extra args to sbom-generate.sh>".
SBOM_CONFIGS=(
    "default:"
    "all-features-all-targets:--all-features --target all"
)

while [ $# -gt 0 ]; do
    case "$1" in
        --out-dir)    [ $# -ge 2 ] || die "--out-dir requires a value"; OUT_DIR="$2"; shift 2 ;;
        --sbom-config) [ $# -ge 2 ] || die "--sbom-config requires a value"; SBOM_CONFIGS+=("$2"); shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "${BASH_SOURCE[0]}" | sed -n '3,/^=====/p' | head -n -1; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
[ -n "$OUT_DIR" ] || die "--out-dir <dir> is required"

require_cmd cargo
require_cmd jq
require_cmd tar
require_cmd curl
require_cargo_subcommand cyclonedx

VERSION="$(signal_kit_version)"
[ -n "$VERSION" ] || die "could not resolve signal-kit version"
PRODUCT="signal-kit"
ROOT="$(repo_root)"
LOCKFILE="$ROOT/Cargo.lock"
[ -f "$LOCKFILE" ] || die "Cargo.lock not found at $LOCKFILE (release resolution is required)"

# Binary/cdylib targets drive whether cargo-auditable applies.
BINARY_TARGETS="$(bash "$SCRIPT_DIR/binary-targets.sh")"
if [ -n "$BINARY_TARGETS" ]; then
    AUDITABLE=1
    require_cargo_subcommand auditable
    log "binary targets present ($(echo "$BINARY_TARGETS" | tr '\n' ' ')); using cargo-auditable"
else
    AUDITABLE=0
    log "no binary/cdylib targets — cargo-auditable is not applicable (source-only library)"
fi

mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

verify_sha256sums() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum -c SHA256SUMS
    else shasum -a 256 -c SHA256SUMS; fi
}

# ----------------------------------------------------------------------------
# 1. Package the exact source with the release lockfile (--locked).
# ----------------------------------------------------------------------------
log "packaging ${PRODUCT}-${VERSION}.crate (--locked)"
cargo package -p "$PRODUCT" --locked --no-verify --target-dir "$OUT_DIR/pkgtmp" \
    || die "cargo package failed (is the working tree clean?)"
CRATE_TARBALL="${OUT_DIR}/${PRODUCT}-${VERSION}.crate"
mv "$OUT_DIR/pkgtmp/package/${PRODUCT}-${VERSION}.crate" "$CRATE_TARBALL"
rm -rf "$OUT_DIR/pkgtmp"
[ -f "$CRATE_TARBALL" ] || die "expected crate tarball was not produced"

# ----------------------------------------------------------------------------
# 2. Extract into a clean temp dir and validate contents.
# ----------------------------------------------------------------------------
EXTRACT="$(mktemp -d)"
trap 'rm -rf "$EXTRACT" "$OUT_DIR/pkgtmp"' EXIT
PKG_DIR="${PRODUCT}-${VERSION}"

# Refuse archives whose entries escape the package directory (path traversal).
if tar -tzf "$CRATE_TARBALL" | grep -Ev "^${PKG_DIR}/" | grep -Eq .; then
    die "packaged crate contains entries outside ${PKG_DIR}/ (path traversal)"
fi
tar -xzf "$CRATE_TARBALL" -C "$EXTRACT"
EXTRACTED="${EXTRACT}/${PKG_DIR}"
[ -d "$EXTRACTED" ] || die "extracted package directory missing: $EXTRACTED"

# Name + version from the packaged manifest must match.
PKG_NAME="$(jq -r '.package[0].name' "$EXTRACTED/Cargo.toml" 2>/dev/null || \
            sed -n 's/^name *= *"\(.*\)"/\1/p' "$EXTRACTED/Cargo.toml" | head -n1)"
PKG_VER="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$EXTRACTED/Cargo.toml" | head -n1)"
[ "$PKG_NAME" = "$PRODUCT" ] || die "packaged name ('$PKG_NAME') != '$PRODUCT'"
[ "$PKG_VER" = "$VERSION" ] || die "packaged version ('$PKG_VER') != '$VERSION'"

# Reject unexpected/private/repository-only files. Allow only the crate's source
# and standard manifest files; flag anything that looks secret- or repo-specific.
DISALLOWED_RE='(^|/)(\.git|\.gitignore|target|deny\.toml|\.github|scripts|SECURITY\.md|.*\.key|.*\.pem|.*\.env|.*\.env\..*|lcov\.info|.*\.profraw)$'
if find "$EXTRACTED" -type f | sed "s|^$EXTRACTED/||" | grep -Eq "$DISALLOWED_RE"; then
    bad="$(find "$EXTRACTED" -type f | sed "s|^$EXTRACTED/||" | grep -E "$DISALLOWED_RE" | head -n5)"
    die "packaged crate contains unexpected/private/repo-only files:
$bad"
fi
log "validated package contents (name=$PKG_NAME version=$PKG_VER)"

# ----------------------------------------------------------------------------
# 3. Compile the extracted package independently with the release lockfile.
# ----------------------------------------------------------------------------
log "independent build of packaged crate with release lockfile (--release --locked)"
cp "$LOCKFILE" "$EXTRACTED/Cargo.lock"
( cd "$EXTRACTED" && cargo metadata --locked --format-version 1 >/dev/null ) \
    || die "cargo metadata failed on the extracted package"
BUILD_CMD=(cargo build --release --locked)
[ "$AUDITABLE" -eq 1 ] && BUILD_CMD=(cargo auditable build --release --locked)
( cd "$EXTRACTED" && "${BUILD_CMD[@]}" ) \
    || die "independent build of the packaged crate failed"

if [ "$AUDITABLE" -eq 1 ]; then
    # Verify auditable metadata is embedded in each built binary.
    require_cargo_subcommand audit
    while IFS= read -r binname; do
        bin="$(cd "$EXTRACTED" && cargo metadata --format-version 1 \
                | jq -r --arg t "$binname" '.packages[0].target_directory + "/" + ([.packages[0].targets[] | select(.name==$t)][0].name)')"
        # Fall back to locating the binary by name under the release dir.
        [ -f "$bin" ] || bin="$(find "$EXTRACTED/target/release" -maxdepth 1 -name "$binname" -type f | head -n1)"
        [ -n "$bin" ] && [ -f "$bin" ] || die "built binary '$binname' not found for auditable inspection"
        ( cd "$ROOT" && cargo audit bin "$bin" >/dev/null ) \
            || die "cargo audit bin failed on '$bin' (auditable metadata missing or vulnerabilities found)"
        log "verified auditable metadata in binary: $binname"
    done <<<"$BINARY_TARGETS"
fi

# ----------------------------------------------------------------------------
# 4. Generate the CycloneDX SBOMs FROM the packaged manifest + release lockfile.
# ----------------------------------------------------------------------------
ASSETS=("$CRATE_TARBALL")
for entry in "${SBOM_CONFIGS[@]}"; do
    name="${entry%%:*}"
    extra="${entry#*:}"
    # shellcheck disable=SC2086
    SBOM_PATH="$(bash "$SCRIPT_DIR/sbom-generate.sh" \
                    --config "$name" --out-dir "$OUT_DIR" \
                    --manifest-path "$EXTRACTED/Cargo.toml" $extra)"
    ASSETS+=("$SBOM_PATH")
done

# ----------------------------------------------------------------------------
# 5. SHA256SUMS over every asset, verified in place.
# ----------------------------------------------------------------------------
SUMS_FILE="${OUT_DIR}/SHA256SUMS"
: > "$SUMS_FILE"
for asset in "${ASSETS[@]}"; do
    printf '%s  %s\n' "$(sha256_of "$asset")" "$(basename "$asset")" >> "$SUMS_FILE"
done
log "verifying SHA256SUMS"
( cd "$OUT_DIR" && verify_sha256sums )

# ----------------------------------------------------------------------------
# 6. Machine-readable inventory (path<TAB>sha256) for downstream steps.
# ----------------------------------------------------------------------------
log "release artefacts assembled in $OUT_DIR"
for asset in "${ASSETS[@]}" "$SUMS_FILE"; do
    printf '%s\t%s\n' "$asset" "$(sha256_of "$asset")"
done
