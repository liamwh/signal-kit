#!/usr/bin/env bash
# =============================================================================
# check-publish-policy.sh — assert signal-kit's intended publishability
# =============================================================================
#
# signal-kit IS published to crates.io by release-plz. This asserts that the two
# sources of truth agree and that crates.io publication is actually permitted:
#
#   * crates/signal-kit/Cargo.toml — must NOT set publish = false (cargo metadata
#     reports `.publish` as null or a list containing crates-io; an empty list
#     means "blocked from every registry" and fails);
#   * .release-plz.toml — `publish = true` (release-plz must publish).
#
# Fails if crates.io publication is disabled in either file, catching a
# contradiction between Cargo.toml and release-plz before a release silently
# skips crates.io. To switch to a different policy, update BOTH files and this
# assertion together.
#
# Usage: check-publish-policy.sh   (no arguments)
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

require_cmd cargo
require_cmd jq

ROOT="$(repo_root)"
fail=0

# 1. Cargo.toml: the package must be publishable to crates.io.
#    cargo metadata `.publish`: null => publishable everywhere; an array => the
#    allow-list of registries (must include crates-io); [] => publish=false.
PUBLISH_JSON="$(cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml" \
    | jq -r '.packages[] | select(.name=="signal-kit") | .publish | if . == null then "null" else (. | join(",")) end')"
case "$PUBLISH_JSON" in
    null) : ;;  # publishable to all registries (no publish = false) — OK
    *crates-io*) : ;;  # explicitly allowed on crates.io — OK
    *) echo "ERROR: signal-kit Cargo.toml blocks crates.io publication (.publish='$PUBLISH_JSON'); expected publishable." >&2; fail=1 ;;
esac

# 2. release-plz.toml: must request a crates.io publish.
RP="$ROOT/.release-plz.toml"
RP_PUBLISH=""
if [ -f "$RP" ]; then
    RP_PUBLISH="$(grep -E '^[[:space:]]*publish[[:space:]]*=' "$RP" | head -n1 | sed -E 's/.*=[[:space:]]*//' | tr -d '[:space:]\"",')"
fi
if [ "$RP_PUBLISH" != "true" ]; then
    echo "ERROR: .release-plz.toml publish='$RP_PUBLISH'; expected 'true' (crates.io publication)." >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "Publish policy check FAILED — Cargo.toml and release-plz.toml disagree or block crates.io publication." >&2
    exit 1
fi

echo "Publish policy OK: signal-kit is crates.io-publishable (Cargo.toml publishable, release-plz.toml publish = true)."
