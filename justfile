# Show available commands
default:
    @just --list --justfile {{justfile()}}

set shell := ["bash", "-cu"]
set dotenv-load := true

format-rust:
    @cargo fmt --all

lint-rust:
    @cargo clippy --all-targets --all-features --workspace

# --- Supply chain ----------------------------------------------------------

# Run the full dependency-policy + vulnerability gate locally (mirrors the
# `supply-chain` GitHub Actions workflow).
security: deny audit

# Run the supply-chain helper self-tests (release identity, SBOM mapping, gate
# behaviour, determinism, checksums, content validation, paths-with-spaces).
test-supply-chain:
    @bash scripts/selftest.sh

# cargo-deny policy check: advisories, licences, bans, sources (deny.toml).
deny:
    @cargo deny check

# RustSec vulnerability scan of Cargo.lock.
audit:
    @cargo audit

# Assert the publish policy is consistent (signal-kit is GitHub-Release only).
publish-policy:
    @bash scripts/check-publish-policy.sh

# List binary/cdylib targets (empty = source-only library; auditable is N/A).
binary-targets:
    @bash scripts/binary-targets.sh

# Generate the canonical (default-feature) CycloneDX 1.5 SBOM.
sbom:
    @bash scripts/sbom-generate.sh --config default

# Generate default + maximal (all-features, all-targets) CycloneDX SBOMs.
sbom-all:
    @bash scripts/sbom-generate.sh --config default
    @bash scripts/sbom-generate.sh --config all-features-all-targets --all-features --target all

# Download + verify the official CycloneDX CLI validator into target/.tools.
cyclonedx-cli:
    @bash scripts/fetch-cyclonedx-cli.sh --install-dir target/.tools

# Validate the canonical SBOM. Schema-validates if the CycloneDX CLI is present
# (set CYCLONEDX_CLI, or run `just cyclonedx-cli` to cache it under target/.tools).
sbom-validate:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    sboms=( target/sbom/signal-kit-*-default.cdx.json )
    [ "${#sboms[@]}" -gt 0 ] || { echo "Run 'just sbom' first."; exit 1; }
    SBOM="${sboms[0]}"
    CLI="${CYCLONEDX_CLI:-}"
    if [ -z "$CLI" ]; then
        tools=( target/.tools/cyclonedx-* )
        [ "${#tools[@]}" -gt 0 ] && CLI="${tools[0]}"
    fi
    CYCLONEDX_CLI="$CLI" bash scripts/sbom-validate.sh "$SBOM"

# Assemble release artefacts locally (.crate + SBOMs + SHA256SUMS) for review.
release-dry-run:
    @bash scripts/release-artifacts.sh --out-dir target/release-dry-run

# Inspect/scan cargo-auditable metadata embedded in a binary. Only meaningful
# when a binary/cdylib target exists (see `just binary-targets`).
# Usage: just audit-binary path/to/binary
audit-binary path:
    @cargo audit bin "{{path}}"

