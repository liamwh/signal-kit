#!/usr/bin/env bash
# =============================================================================
# verify-release-identity.sh — prove a release run operates on one exact release
# =============================================================================
#
# After checking out a commit, verify that the checked-out state is EXACTLY the
# intended release: one tag, one version, one commit SHA, all mutually agreeing.
# This replaces any "latest release" / creation-order / timestamp selection.
#
# Assertions (all must hold; the script fails on the first violation):
#   * --tag is non-empty and is NOT a sentinel/glob ("latest", "*", etc.);
#   * the checked-out commit SHA equals --sha (when provided);
#   * the signal-kit version resolved from the manifest equals --version (when
#     provided);
#   * the tag name equals the release-plz tag derived from .release-plz.toml
#     (tag_prefix + version; default "v<version>");
#   * the tag points at the checked-out commit (via gh, when GH_TOKEN is set);
#   * a GitHub Release named <tag> exists at that commit (via gh, when set).
#
# The gh-backed checks are skipped locally (no GH_TOKEN); in CI they run and bind
# the tag/release to the exact checked-out commit.
#
# Usage:
#   verify-release-identity.sh --tag <tag> [--version <ver>] [--sha <sha>]
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

TAG=""
EXPECT_VERSION=""
EXPECT_SHA=""
while [ $# -gt 0 ]; do
    case "$1" in
        --tag)    [ $# -ge 2 ] || die "--tag requires a value"; TAG="$2"; shift 2 ;;
        --version) [ $# -ge 2 ] || die "--version requires a value"; EXPECT_VERSION="$2"; shift 2 ;;
        --sha)    [ $# -ge 2 ] || die "--sha requires a value"; EXPECT_SHA="$2"; shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "${BASH_SOURCE[0]}" | sed -n '3,/^=====/p' | head -n -1; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
[ -n "$TAG" ] || die "--tag is required"

require_cmd cargo
require_cmd jq

# --- reject sentinel / glob / "latest" selectors -----------------------------
# A release tag must be a concrete tag name. "latest", order-based, or wildcard
# selectors are explicitly forbidden — they select by rank, not by identity.
case "$TAG" in
    latest|head|tip|"") die "refusing non-identity tag selector: '$TAG'" ;;
    *\**|*\?*)          die "refusing glob/wildcard tag selector: '$TAG'" ;;
esac

# --- checked-out commit SHA --------------------------------------------------
CHECKED_SHA="$(git -C "$(repo_root)" rev-parse HEAD 2>/dev/null || echo "")"
[ -n "$CHECKED_SHA" ] || die "could not resolve checked-out commit SHA"
if [ -n "$EXPECT_SHA" ]; then
    [ "$CHECKED_SHA" = "$EXPECT_SHA" ] \
        || die "checked-out SHA ($CHECKED_SHA) != expected ($EXPECT_SHA)"
fi

# --- resolved version --------------------------------------------------------
RESOLVED_VERSION="$(signal_kit_version)"
[ -n "$RESOLVED_VERSION" ] || die "could not resolve signal-kit version"
if [ -n "$EXPECT_VERSION" ]; then
    [ "$RESOLVED_VERSION" = "$EXPECT_VERSION" ] \
        || die "resolved version ($RESOLVED_VERSION) != expected ($EXPECT_VERSION)"
fi

# --- tag name convention: derived from release-plz config --------------------
# Expected tag is derived from .release-plz.toml (tag_prefix) + version, not
# blindly assumed. signal-kit is a single workspace member, so the tag is
# "<tag_prefix>v<version>" (default tag_prefix => "v<version>").
EXPECTED_TAG="$(expected_release_tag "$RESOLVED_VERSION")"
[ "$TAG" = "$EXPECTED_TAG" ] \
    || die "tag ('$TAG') != expected release tag ('$EXPECTED_TAG' from release-plz config)"

# --- gh-backed binding (CI only) ---------------------------------------------
# When GH_TOKEN is present (CI), bind the tag and the GitHub Release to the exact
# checked-out commit. Skipped locally so the pure-identity checks remain testable.
REPO="${GITHUB_REPOSITORY:-}"
if [ -n "${GH_TOKEN:-}${GITHUB_TOKEN:-}" ] && [ -n "$REPO" ]; then
    require_cmd gh
    TAG_SHA="$(gh api "repos/${REPO}/git/ref/tags/${TAG}" --jq '.object.sha' 2>/dev/null || echo "")"
    [ -n "$TAG_SHA" ] || die "tag '$TAG' not found in ${REPO}"
    # Dereference annotated tags to the commit they point at.
    if gh api "repos/${REPO}/git/tags/${TAG_SHA}" >/dev/null 2>&1; then
        TAG_SHA="$(gh api "repos/${REPO}/git/tags/${TAG_SHA}" --jq '.object.sha')"
    fi
    [ "$TAG_SHA" = "$CHECKED_SHA" ] \
        || die "tag '$TAG' points at $TAG_SHA, but the checked-out commit is $CHECKED_SHA"
    gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1 \
        || die "GitHub Release '$TAG' not found in ${REPO}"
    log "gh binding OK: tag '$TAG' and its Release point at $CHECKED_SHA"
fi

log "release identity verified: tag=$TAG version=$RESOLVED_VERSION sha=$CHECKED_SHA"
echo "tag=${TAG}"
echo "version=${RESOLVED_VERSION}"
echo "sha=${CHECKED_SHA}"
