# Maintainer & release-supply-chain notes

Operational detail for maintainers of `signal-kit`. User-facing information lives
in `README.md`; `SECURITY.md` covers the supply-chain policy. This document
records *why* the pipeline behaves the way it does and the exact recovery
procedure when something goes wrong mid-release.

## How the first release differs from subsequent release-plz releases

`release-plz.yml` runs release-plz with no `command`, which executes **both**
`release-pr` and `release` on every push to `main`.

- **First release (0.1.0):** the manifest version is already `0.1.0` and
  unpublished, so the `release` step publishes to crates.io and creates the
  `v0.1.0` annotated tag + GitHub Release **directly** on that push. There is no
  separate "release PR" for the initial version. (`0.1.0` was in fact published
  this way on 2026-06-22; its supply-chain assets were completed by a manual
  dispatch because the tag-resolution step initially mishandled annotated tags —
  since fixed.)
- **Subsequent releases (0.1.x+):** a conventional change on `main` causes
  `release-pr` to open a release PR proposing the version bump + changelog. That
  PR is merged (its merge commit is the *release commit*); the next `release` run
  publishes the new version, creates the annotated tag, and creates the GitHub
  Release. The supply-chain workflow then resolves that annotated tag to its
  commit and attaches assets/attestations.

> Practical rule: the *initial* crate version publishes directly; every later
> version goes through a reviewable release PR. Do not be surprised that 0.1.0
> had no reviewable release PR.

## Why CycloneDX 1.4 (not 1.5)

`cargo-cyclonedx` 0.5.x emits a **non-conformant CycloneDX 1.5** document — it
writes `metadata.tools` as a JSON array (the 1.4 shape) while declaring
`specVersion: "1.5"` (which requires `tools` to be an object
`{components:[], services:[]}`). The CycloneDX CLI validator is lenient and
accepts it; `actions/attest-sbom`'s strict parser rejects it. We therefore emit
**CycloneDX 1.4** (whose `tools` array is conformant) until `cargo-cyclonedx`
ships a fully-conformant 1.5. The dependency inventory is identical; only the
document format differs.

## Why `actions/attest` is used for the CycloneDX predicate (not `attest-sbom`)

`actions/attest-sbom` rejects cargo-cyclonedx output with *"Unsupported SBOM
format. Must be valid SPDX or CycloneDX JSON."* — across `attest-sbom` v2.4.0 and
v4.1.0, and across CycloneDX 1.4 and 1.5. Its embedded CycloneDX validator is
incompatible with cargo-cyclonedx's output.

The SBOM↔crate bindings therefore use the generic **`actions/attest`** with
`predicate-type: https://cyclonedx.org/bom` and `predicate-path: <sbom>`.
`actions/attest` embeds the SBOM as the attestation predicate **without
re-validating it**, so the binding is created and remains verifiable with
`gh attestation verify`. Build provenance (`actions/attest-build-provenance`) is
unchanged and attests the `.crate` and SBOMs by digest.

If `attest-sbom` gains compatible CycloneDX support, this can be reverted — but
re-validate first, since the incompatibility was not version-dependent.

## Verifying crate and SBOM attestations locally

```sh
# Download the .crate from crates.io or the GitHub Release, then:
gh attestation verify signal-kit-<version>.crate --repo liamwh/signal-kit
# This verifies every attestation whose subject is that file's digest:
# build provenance AND the CycloneDX SBOM predicates.
# Filter: --predicate-type https://slsa.dev/provenance/v1  (provenance)
#         --predicate-type https://cyclonedx.org/bom        (SBOM)
```

## Verifying SHA256SUMS

```sh
gh release download v0.1.0 --repo liamwh/signal-kit \
    --pattern 'signal-kit-0.1.0.crate' \
    --pattern 'signal-kit-0.1.0-*.cdx.json' \
    --pattern 'SHA256SUMS'
sha256sum -c SHA256SUMS        # GNU
shasum -a 256 -c SHA256SUMS    # macOS/BSD
```
`SHA256SUMS` covers exactly the `.crate` and both CycloneDX SBOMs (each once).

## Why `v*` tag protection is not currently enabled

GitHub tag-protection rules restrict tag creation to explicitly-listed
users/teams/apps. release-plz creates the `v<version>` tag using the default
`GITHUB_TOKEN` (the `github-actions[bot]`), which is **not** in that allow-list —
so enabling `v*` tag protection now would **block every release-plz tag push** and
break releases.

### Enabling tag protection safely

Tag protection can be enabled without breaking releases only if the actor that
creates tags is permitted:

1. Create a **GitHub App** (or use a fine-grained PAT) dedicated to releases with
   `contents: write`.
2. Configure release-plz to use that App/PAT as its `GITHUB_TOKEN`
   (`release-plz.yml` `env: GITHUB_TOKEN: ${{ secrets.<APP_OR_PAT_SECRET> }}`).
3. Add the App (or PAT user) to the `v*` tag-protection allow-list.
4. Verify a release still creates the tag before relying on the rule.

Until that App-based release identity is set up, tag protection stays off and
the `main` branch ruleset + supply-chain gate remain the integrity boundary.

## Responding when SARIF upload is unavailable

The `upload-sarif` steps are **best-effort** (`continue-on-error: true`). When
code scanning is unavailable (repo without the feature, transient upload error),
the step emits a `::warning::` annotation and a job-summary entry
(`⚠️ SARIF → code scanning upload failed`). **The security gate is unaffected** —
cargo-deny and cargo-audit still run and fail the job on findings via the
explicit "Fail scan on policy/vulnerability findings" step.

When you see that warning: enable code scanning
(**Settings → Code security → Code scanning**) for the public repo (free), or
GitHub Advanced Security for a private repo, then re-run the workflow.

## Manual recovery when publication succeeds but asset assembly fails

This is the most important procedure. If release-plz publishes the crate to
crates.io but the `release-supply-chain` workflow fails *after* publication
(e.g. at assembly, attestation, or upload), the crate is already immutable on
crates.io and the GitHub Release lacks its assets/attestations.

**Do not re-publish** (the version is immutable on crates.io; only a new version
can be published). Instead, complete the existing release's assets:

1. Diagnose and **fix** the failure on a branch + PR (e.g. the annotated-tag
   dereference, attest action, or SBOM format fixes done for 0.1.0). Merge it.
2. Re-run the supply-chain for the **already-released tag**, explicitly (this is
   non-publishing — `release-supply-chain.yml` has no `CARGO_REGISTRY_TOKEN`):
   ```sh
   gh workflow run release-supply-chain.yml --repo liamwh/signal-kit \
       --ref main -f tag=v<version>
   ```
3. The run is **idempotent**: it re-verifies the release identity
   (`scripts/verify-release-identity.sh`), re-assembles the `.crate` + SBOMs +
   `SHA256SUMS`, re-creates attestations (by digest — adding to the existing
   set, all valid), and `gh release upload --clobber` overwrites the assets.
4. Watch the in-workflow **download-back verify** step (`gh attestation verify` +
   `SHA256SUMS` on the freshly-uploaded assets); it must pass.
5. Confirm as a consumer: `gh attestation verify signal-kit-<version>.crate
   --repo liamwh/signal-kit` and `sha256sum -c SHA256SUMS`.

This is exactly how the published `0.1.0` release was completed after the
supply-chain job was skipped by the (then-unfixed) annotated-tag resolution.
