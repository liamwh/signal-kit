#!/usr/bin/env bash
# =============================================================================
# binary-targets.sh — list executable/dynamic-library targets of a package
# =============================================================================
#
# Prints the names of `[[bin]]` and `crate-type = ["cdylib"|"bin"]` targets for
# the signal-kit package, one per line. Output is empty when the package is a
# pure library (the current state of signal-kit).
#
# This drives whether cargo-auditable (which only embeds metadata into
# executables / cdylibs) is applicable to a release. A source-only library ships
# no binary, so auditable is skipped rather than falsely claimed — see SECURITY.md.
#
# Usage: binary-targets.sh [--package <name>]   (default: signal-kit)
# Exit status: 0 always (empty output means "no binary targets").
# =============================================================================
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
. "$SCRIPT_DIR/lib.sh"

PACKAGE="signal-kit"
while [ $# -gt 0 ]; do
    case "$1" in
        --package) [ $# -ge 2 ] || die "--package requires a value"; PACKAGE="$2"; shift 2 ;;
        -h|--help) sed -n 's/^# \{0,1\}//p' "${BASH_SOURCE[0]}" | sed -n '3,/^=====/p' | head -n -1; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done

require_cmd cargo
require_cmd jq

# A target is a *shipped* executable if it is a real [[bin]] (kind == "bin") or a
# cdylib library (crate-type cdylib). Test/example/bench targets also report
# crate_type "bin" but are NOT shipped, so they are excluded via the kind check.
cargo metadata --no-deps --format-version 1 --manifest-path "$(repo_root)/Cargo.toml" \
    | jq -r --arg pkg "$PACKAGE" '
        .packages[]
          | select(.name == $pkg)
          | .targets[]
          | select(
              (any(.kind[]; . == "bin"))
              or (any(.crate_types[]; . == "cdylib") and any(.kind[]; . == "lib"))
            )
          | .name
        ' 2>/dev/null || true
