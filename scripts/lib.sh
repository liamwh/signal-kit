#!/usr/bin/env bash
# =============================================================================
# lib.sh — shared helpers for signal-kit supply-chain scripts
# =============================================================================
#
# This file is *sourced* by the other scripts in this directory; it is not
# executed directly. It provides strict, portable helpers for error handling,
# environment guards, version resolution, reproducible-build timestamps and
# GitHub job-summary output.
#
# Conventions every script in this directory follows:
#   - `set -euo pipefail` (fail fast on errors, unset vars, pipe failures).
#   - Guard clauses validate arguments and required environment up front.
#   - JSON is parsed with `jq`, never with grep/sed/awk.
#   - Every function has a documentation comment above it.
# =============================================================================

# die <message...>
#   Print an error to stderr and exit with status 1.
die() {
    echo "ERROR: $*" >&2
    exit 1
}

# log <message...>
#   Print an informational line tagged with a supply-chain prefix to STDERR,
#   so STDOUT is reserved for machine-readable output (file paths, checksums).
log() {
    echo "[supply-chain] $*" >&2
}

# require_var <NAME>
#   Exit if the environment variable NAME is unset or empty.
require_var() {
    local name="$1"
    local value
    value="${!name:-}"
    [ -n "$value" ] || die "required environment variable $name is not set"
}

# require_cmd <command>
#   Exit if command is not on PATH.
require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command '$1' not found on PATH"
}

# require_cargo_subcommand <name>
#   Verify a cargo subcommand (e.g. `cyclonedx`, `auditable`) is available.
#   Cargo resolves subcommands via PATH AND $CARGO_HOME/bin, so the check must
#   go through cargo itself rather than the bare binary name.
require_cargo_subcommand() {
    local sub="$1"
    cargo "$sub" --version >/dev/null 2>&1 \
        || die "required cargo subcommand 'cargo $sub' is not available (install: cargo install --locked cargo-$sub)"
}

# repo_root
#   Print the absolute path of the git/repository root (this file's parent dir).
repo_root() {
    local dir
    dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    echo "$dir"
}

# signal_kit_version
#   Print the resolved version of the signal-kit crate from its manifest.
#   Uses cargo metadata so it reflects the exact lockfile/manifest state.
signal_kit_version() {
    require_cmd cargo
    require_cmd jq
    cargo metadata --no-deps --format-version 1 --manifest-path "$(repo_root)/Cargo.toml" \
        | jq -r '.packages[] | select(.name=="signal-kit") | .version'
}

# commit_epoch
#   Print the SOURCE_DATE_EPOCH to use for reproducible builds:
#     1. an already-set SOURCE_DATE_EPOCH;
#     2. else the UNIX timestamp of the HEAD commit;
#     3. else 0 (fully deterministic, epoch).
#   Never fails: reproducibility always wins over timestamp freshness.
commit_epoch() {
    if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
        echo "$SOURCE_DATE_EPOCH"
    elif git -C "$(repo_root)" rev-parse HEAD >/dev/null 2>&1; then
        git -C "$(repo_root)" log -1 --pretty=%ct HEAD 2>/dev/null || echo 0
    else
        echo 0
    fi
}

# summary <markdown...>
#   Append a line to the GitHub Actions job summary ($GITHUB_STEP_SUMMARY) when
#   running inside Actions, otherwise echo it to stdout. Lets the same script
#   produce human-readable output locally and a rendered summary in CI.
summary() {
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        echo "$*" >> "$GITHUB_STEP_SUMMARY"
    else
        echo "$*"
    fi
}

# sha256_of <file>
#   Print the SHA-256 checksum of a file using the host's available tool.
#   Works on both GNU (sha256sum) and BSD/macOS (shasum -a 256) userland.
sha256_of() {
    local file="$1"
    [ -f "$file" ] || die "checksum target does not exist: $file"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    else
        shasum -a 256 "$file" | awk '{print $1}'
    fi
}

# expected_release_tag <version>
#   Derive the release-plz git tag for signal-kit from the real release-plz
#   configuration, rather than blindly assuming "v<version>".
#
#   release-plz tag rules:
#     * single workspace member, no tag_prefix => "v<version>"  (this repo);
#     * tag_prefix set in .release-plz.toml    => "<tag_prefix>v<version>";
#     * multiple workspace members              => "<package>-v<version>" (NOT
#       supported here — signal-kit has a single member; adding packages
#       requires extending this and the identity checks).
expected_release_tag() {
    local version="$1"
    local prefix=""
    local cfg
    cfg="$(repo_root)/.release-plz.toml"
    if [ -f "$cfg" ]; then
        prefix="$(grep -E '^[[:space:]]*tag_prefix[[:space:]]*=' "$cfg" | head -n1 \
            | sed -E 's/.*=[[:space:]]*"([^"]*)".*/\1/' 2>/dev/null || true)"
    fi
    echo "${prefix}v${version}"
}
