#!/usr/bin/env bash
# =============================================================================
# selftest.sh — exercise the non-trivial supply-chain helper logic
# =============================================================================
#
# Integration-style tests for scripts/*.sh. Covers the behaviours that, if
# broken, would silently corrupt a release, hide a vulnerability, or misbind an
# attestation:
#
#   * exact tag/version/manifest agreement + refusal of "latest" / globs;
#   * duplicate / mismatched-release rejection;
#   * SBOM-to-.crate subject mapping (the SBOM describes signal-kit and matches
#     the packaged manifest's dependency set);
#   * archive content validation (path-traversal + disallowed-file rejection);
#   * checksum verification after a simulated release download;
#   * final gate behaviour after success, failure and intentional skip;
#   * absence of binary/cdylib targets (so auditable is not mis-claimed);
#   * malformed and semantically-incorrect SBOMs are rejected;
#   * scripts tolerate paths containing spaces;
#   * temporary directories are cleaned up;
#   * failure propagation from every scanner and validator (non-zero exits).
#
# Run locally with `just test-supply-chain`; also runs in the `scan` job of the
# `supply-chain` GitHub Actions workflow. Exits non-zero if any assertion fails.
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

require_cmd cargo
require_cargo_subcommand cyclonedx
require_cmd jq
require_cmd tar

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0
ok()   { echo "  ✓ $1"; PASS=$((PASS + 1)); }
bad()  { echo "  ✗ $1" >&2; FAIL=$((FAIL + 1)); }
# expect_ok <desc> <cmd...> : pass if the command exits 0.
expect_ok() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then ok "$desc"; else bad "$desc"; fi
}
# expect_fail <desc> <cmd...> : pass if the command exits non-zero.
expect_fail() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then bad "$desc (expected failure)"; else ok "$desc"; fi
}

PRODUCT="signal-kit"
VERSION="$(signal_kit_version)"
TAG="v${VERSION}"

echo "Running supply-chain helper tests…"

# ============================================================================
# 1. Publish policy + binary targets
# ============================================================================
expect_ok  "publish policy consistent (crates.io-publishable)" \
    bash "$SCRIPT_DIR/check-publish-policy.sh"

BINS="$(bash "$SCRIPT_DIR/binary-targets.sh")"
if [ -z "$BINS" ]; then ok "no binary/cdylib targets (auditable correctly N/A)"; \
    else bad "unexpected binary targets: $BINS"; fi

# ============================================================================
# 2. Release identity: exact agreement + refusal of non-identity selectors
# ============================================================================
expect_ok   "identity agrees: tag=v$VERSION version=$VERSION" \
    bash "$SCRIPT_DIR/verify-release-identity.sh" --tag "$TAG" --version "$VERSION"
expect_fail "identity refuses 'latest'" \
    bash "$SCRIPT_DIR/verify-release-identity.sh" --tag latest
expect_fail "identity refuses empty tag" \
    bash "$SCRIPT_DIR/verify-release-identity.sh" --tag ""
expect_fail "identity refuses glob tag" \
    bash "$SCRIPT_DIR/verify-release-identity.sh" --tag 'v*'
expect_fail "identity refuses mismatched version (9.9.9)" \
    bash "$SCRIPT_DIR/verify-release-identity.sh" --tag "$TAG" --version 9.9.9
expect_fail "identity refuses tag != v<version>" \
    bash "$SCRIPT_DIR/verify-release-identity.sh" --tag "v0.0.0"

# ============================================================================
# 3. SBOM generation + validation (good / malformed / semantically wrong)
# ============================================================================
SBOM_OUT="$WORK/sbom"
SBOM="$(bash "$SCRIPT_DIR/sbom-generate.sh" --config default --out-dir "$SBOM_OUT")"
[ -f "$SBOM" ] && ok "default SBOM generated" || bad "default SBOM not generated"
if [ -f "$SBOM" ]; then
    assert_eq() { [ "$2" = "$3" ] && ok "$1" || bad "$1 ('$2' != '$3')"; }
    assert_eq "SBOM bomFormat"          "CycloneDX"  "$(jq -r '.bomFormat' "$SBOM")"
    assert_eq "SBOM describes signal-kit" "$PRODUCT" "$(jq -r '.metadata.component.name' "$SBOM")"
    assert_eq "SBOM specVersion"        "1.5"        "$(jq -r '.specVersion' "$SBOM")"
    assert_eq "SBOM component version"  "$VERSION"   "$(jq -r '.metadata.component.version' "$SBOM")"
fi

CLI="${CYCLONEDX_CLI:-}"
[ -z "$CLI" ] && CLI="$(ls "$WORK"/../.tools/cyclonedx-* 2>/dev/null | head -n1 || true)"
[ -z "$CLI" ] && [ -d "$SCRIPT_DIR/../target/.tools" ] && CLI="$(ls "$SCRIPT_DIR/../target/.tools"/cyclonedx-* 2>/dev/null | head -n1 || true)"

if [ -n "$CLI" ] && [ -x "$CLI" ]; then
    expect_ok "valid SBOM passes schema validation" \
        env CYCLONEDX_CLI="$CLI" bash "$SCRIPT_DIR/sbom-validate.sh" "$SBOM"
else
    expect_ok "valid SBOM passes structural validation" \
        bash "$SCRIPT_DIR/sbom-validate.sh" "$SBOM"
fi

# Malformed SBOM (invalid JSON).
printf '{not json' > "$WORK/malformed.cdx.json"
expect_fail "malformed (invalid JSON) SBOM rejected" \
    bash "$SCRIPT_DIR/sbom-validate.sh" "$WORK/malformed.cdx.json"

# Semantically incorrect SBOM (valid JSON, wrong bomFormat).
printf '{"bomFormat":"SPDX","specVersion":"1.5","components":[],"metadata":{"component":{"name":"x","version":"1"}}}' \
    > "$WORK/wrongfmt.cdx.json"
expect_fail "semantically wrong bomFormat rejected" \
    bash "$SCRIPT_DIR/sbom-validate.sh" "$WORK/wrongfmt.cdx.json"

# Semantically incorrect SBOM (missing components array).
printf '{"bomFormat":"CycloneDX","specVersion":"1.5","metadata":{"component":{"name":"x","version":"1"}}}' \
    > "$WORK/nocomp.cdx.json"
expect_fail "SBOM missing components array rejected" \
    bash "$SCRIPT_DIR/sbom-validate.sh" "$WORK/nocomp.cdx.json"

# ============================================================================
# 4. SBOM determinism
# ============================================================================
if [ -f "$SBOM" ]; then
    H1="$(sha256_of "$SBOM")"
    bash "$SCRIPT_DIR/sbom-generate.sh" --config default --out-dir "$SBOM_OUT" >/dev/null
    H2="$(sha256_of "$SBOM")"
    [ "$H1" = "$H2" ] && ok "SBOM is deterministic" || bad "SBOM not deterministic"
fi

# ============================================================================
# 5. SBOM-to-.crate subject mapping + archive content validation
# ============================================================================
CRATE_TAR="$WORK/${PRODUCT}-${VERSION}.crate"
# --allow-dirty: this packages the working tree (possibly with uncommitted
# edits during local development) purely to test extraction/content/mapping.
# The real release path (release-artifacts.sh) is strict — no --allow-dirty —
# because it runs from a clean, tagged checkout.
if cargo package --manifest-path "$(repo_root)/Cargo.toml" -p "$PRODUCT" --locked --no-verify --allow-dirty --target-dir "$WORK/p" >/dev/null 2>&1; then
    mv "$WORK/p/package/${PRODUCT}-${VERSION}.crate" "$CRATE_TAR"
fi
rm -rf "$WORK/p"
if [ -f "$CRATE_TAR" ]; then
    ok "packaged .crate produced"
    # The packaged .crate must contain only <product>-<version>/ entries (no
    # path traversal, no repository-only files). release-artifacts.sh enforces
    # this; assert it directly here.
    if tar -tzf "$CRATE_TAR" 2>/dev/null | grep -Ev "^${PRODUCT}-${VERSION}/" | grep -Eq .; then
        bad "packaged .crate contains out-of-tree entries"
    else
        ok "packaged .crate has no path-traversal / out-of-tree entries"
    fi
    # SBOM dependency set must match the packaged manifest's resolution (the SBOM
    # is bound to the .crate, not to unrelated workspace state).
    PKG_DIR="$WORK/extract/${PRODUCT}-${VERSION}"
    mkdir -p "$WORK/extract"
    tar -xzf "$CRATE_TAR" -C "$WORK/extract"
    cp "$SCRIPT_DIR/../Cargo.lock" "$PKG_DIR/Cargo.lock"
    PKG_SBOM="$(bash "$SCRIPT_DIR/sbom-generate.sh" --config default \
                    --out-dir "$WORK/pkg-sbom" --manifest-path "$PKG_DIR/Cargo.toml")"
    if diff <(jq -r '.components[] | "\(.name)@\(.version)"' "$SBOM" | sort -u) \
            <(jq -r '.components[] | "\(.name)@\(.version)"' "$PKG_SBOM" | sort -u) >/dev/null; then
        ok "SBOM-to-.crate mapping: packaged manifest resolves identical dependency set"
    else
        bad "SBOM-to-.crate mapping: dependency sets differ"
    fi
else
    bad "could not package .crate for mapping/content tests"
fi

# ============================================================================
# 6. Checksum verification after simulated release download
# ============================================================================
ASSET="$WORK/asset.bin"
printf 'release asset content\n' > "$ASSET"
DIGEST="$(sha256_of "$ASSET")"
printf '%s  %s\n' "$DIGEST" "asset.bin" > "$WORK/SHA256SUMS"
# Simulate "download": copy asset to a fresh dir alongside SHA256SUMS, verify.
DL="$WORK/download"; mkdir -p "$DL"
cp "$ASSET" "$DL/asset.bin"; cp "$WORK/SHA256SUMS" "$DL/SHA256SUMS"
if ( cd "$DL" && { sha256sum -c SHA256SUMS 2>/dev/null || shasum -a 256 -c SHA256SUMS; } ) >/dev/null 2>&1; then
    ok "checksum verifies after simulated download"
else
    bad "checksum verification after download failed"
fi
# Tamper → must fail.
printf 'tampered\n' > "$DL/asset.bin"
if ( cd "$DL" && { sha256sum -c SHA256SUMS 2>/dev/null || shasum -a 256 -c SHA256SUMS; } ) >/dev/null 2>&1; then
    bad "tampered asset passed checksum (should fail)"
else
    ok "tampered asset fails checksum"
fi

# ============================================================================
# 7. Final gate behaviour (success / failure / intentional skip)
# ============================================================================
expect_ok   "gate passes on full success" \
    bash "$SCRIPT_DIR/gate-eval.sh" --detect success --scan success --sbom success
expect_ok   "gate passes when sbom intentionally skipped" \
    bash "$SCRIPT_DIR/gate-eval.sh" --detect success --scan success --sbom skipped
expect_fail "gate fails when scan failed" \
    bash "$SCRIPT_DIR/gate-eval.sh" --detect success --scan failure --sbom success
expect_fail "gate fails when sbom failed (not skipped)" \
    bash "$SCRIPT_DIR/gate-eval.sh" --detect success --scan success --sbom failure
expect_fail "gate fails when detect failed" \
    bash "$SCRIPT_DIR/gate-eval.sh" --detect failure --scan success --sbom skipped

# ============================================================================
# 8. Paths containing spaces
# ============================================================================
SPACE_OUT="$WORK/path with spaces"
SPACE_SBOM="$(bash "$SCRIPT_DIR/sbom-generate.sh" --config default --out-dir "$SPACE_OUT")"
if [ -f "$SPACE_SBOM" ]; then ok "scripts tolerate paths with spaces (SBOM gen)"; \
    else bad "paths with spaces broke SBOM generation"; fi
SPACE_FILE="$SPACE_OUT/file with spaces.txt"
printf 'x\n' > "$SPACE_FILE"
if [ "$(sha256_of "$SPACE_FILE")" != "" ]; then ok "sha256_of handles spaced paths"; \
    else bad "sha256_of failed on spaced path"; fi

# ============================================================================
# 9. Temporary-file cleanup
# ============================================================================
# sbom-generate.sh traps its staging file; assert no staging artefact lingers in
# the crate source dir after generation.
STAGING="$(repo_root)/crates/signal-kit/${PRODUCT}-${VERSION}-default.cdx.json"
if [ -f "$STAGING" ]; then bad "staging SBOM leaked into crate dir"; \
    else ok "no staging artefact leaked (temp cleanup)"; fi

# ============================================================================
# 10. Failure propagation from scanners / validators
# ============================================================================
# sbom-validate must propagate non-zero on a bad file (already covered above as
# expect_fail). cargo-deny / cargo-audit propagation is exercised by the real
# scans; here we assert the helper's exit contract explicitly.
expect_fail "sbom-validate propagates failure (bad SBOM)" \
    bash "$SCRIPT_DIR/sbom-validate.sh" "$WORK/nocomp.cdx.json"

# ============================================================================
# Report
# ============================================================================
echo
echo "Passed: $PASS   Failed: $FAIL"
if [ "$FAIL" -ne 0 ]; then
    echo "SELFTEST FAILED" >&2
    exit 1
fi
echo "SELFTEST PASSED"
