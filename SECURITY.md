# Supply-chain & release security

How `signal-kit` manages its software supply chain: what is released, how
dependencies are governed and scanned, how to verify what you download, and the
exact policy under which a release may ship.

> **What signal-kit is.** `signal-kit` is a **library** crate (`signal-kit`,
> dual-licensed `MIT OR Apache-2.0`), **published to crates.io** by release-plz
> (Cargo.toml is crates.io-publishable; `.release-plz.toml` `publish = true`;
> consistency asserted by `scripts/check-publish-policy.sh`). Each version is
> therefore available BOTH on crates.io (the `.crate` source tarball) and as a
> GitHub Release asset, with the same `.crate` bytes, CycloneDX SBOMs, checksums
> and attestations attached to the Release. There is no binary target, so there
> is no per-target executable to embed metadata into or scan as a binary;
> `cargo-auditable` is conditionally wired and currently a no-op (see
> [cargo-auditable](#cargo-auditable)).

---

## Release pipeline

release-plz (`release-plz.yml`) versions the crate, opens the changelog/release
PR, and on merge to `main` **publishes to crates.io**, creates the git tag +
GitHub Release, then resolves the **exact** tag/version/SHA at the triggering
commit (never "latest"/creation-order) and invokes the **reusable**
`release-supply-chain.yml` workflow to attach SBOMs, checksums and attestations,
and download them back to prove a consumer can verify them.

```
push: main ─▶ Release-plz ──publish to crates.io + tag + Release @ SHA──▶ resolves exact
                                                                         tag/version/sha
                                                                              │
                                                                              ▼
                          release-supply-chain.yml (reusable workflow_call)
                          verify identity → gate → build .crate → SBOMs →
                          attest → upload → download+verify
```

### crates.io publishing

release-plz publishes with the `CARGO_REGISTRY_TOKEN` secret (the
[hyprfocus](https://github.com/liamwh/HyprFocus) pattern; works from the first
release). **Recommended secure upgrade — Trusted Publishing (OIDC, RFC 3691)** is
supported by release-plz and crates.io and removes the long-lived token. It
needs (a) the crate published once manually first (a crates.io limitation for
brand-new crates) and (b) a trusted publisher configured on crates.io for
`liamwh/signal-kit` + the `Release-plz` workflow. Once done: remove
`CARGO_REGISTRY_TOKEN` from `release-plz.yml` and add `id-token: write` to the
`release-plz` job permissions.

### Exact release identity (no "latest")

A release run operates on one exact tag, version and commit SHA. `release-plz.yml`
finds the single tag pointing at the triggering commit (`gh …/git/matching-refs`,
filtered by SHA — no ordering), then `scripts/verify-release-identity.sh` asserts
that the checked-out commit, the manifest version, the tag name (derived from
`.release-plz.toml`: `<tag_prefix>v<version>`, default `v<version>`) and the
GitHub Release all agree, and refuses multiple tags at one commit. Any mismatch
or any "latest"/glob selector fails the run. The privileged job checks out only
that exact commit.

---

## What is generated

For release `signal-kit-<version>`:

| Asset | Description |
|---|---|
| `signal-kit-<version>.crate` | Packaged source (`cargo package --locked`) — **also published to crates.io**. The source of truth. |
| `signal-kit-<version>-default.cdx.json` | **Canonical** CycloneDX 1.5 SBOM (see scope below). |
| `signal-kit-<version>-all-features-all-targets.cdx.json` | **Maximal** CycloneDX 1.5 SBOM. |
| `SHA256SUMS` | SHA-256 of the `.crate` and both SBOMs. |

Plus GitHub **attestations** (sigstore; verifiable with `gh`):
- **Build provenance** for the `.crate` and both SBOMs.
- **SBOM attestations** binding each SBOM to the exact `.crate` (subject = `.crate`, payload = the CycloneDX document).

The `.crate` is the source of truth for everything: it is produced with
`cargo package --locked`, extracted into a clean directory, content-validated
(no path traversal, no repository-only/private files), compiled independently
with the release lockfile, and the SBOMs are generated **from the extracted
packaged manifest** (with the release `Cargo.lock`) — so the SBOM corresponds to
exactly what users download, not to workspace-only state.

---

## CycloneDX SBOMs — scope and resolution inputs

A library SBOM is a **reference dependency resolution for signal-kit's release
configuration**, not a prediction of every downstream consumer's final graph (a
consumer's own lockfile + target + features determine that). Both SBOMs are
generated with:

- **Toolchain:** Rust **1.92.0** (`rust-toolchain.toml`) · Cargo **1.92.0**.
- **Tool:** `cargo-cyclonedx` 0.5.9 → CycloneDX **1.5** JSON.
- **Lockfile:** the committed `Cargo.lock` (the release dependency resolution).
- **Manifest:** the packaged `signal-kit-<version>.crate`'s `Cargo.toml`.
- **Reproducibility:** `SOURCE_DATE_EPOCH` = the release commit timestamp → byte-identical output per commit (no random serial number).

| SBOM | Features | Target scope | Components |
|---|---|---|---|
| `…-default.cdx.json` | default (`traces, metrics, fmt, grpc-tonic, tls-roots`) | build host (`x86_64-unknown-linux-gnu` in CI) | 120 |
| `…-all-features-all-targets.cdx.json` | all features | **all targets** (`--target all`) | 180 |

**What "all targets" means (proven).** `cargo cyclonedx --target all` instructs
cargo to resolve dependencies for every supported target triple, including
platform-conditional ones. The maximal SBOM contains platform dependencies
absent from the host-target SBOM — e.g. `windows*`, `winapi-*`, `wasi`,
`wasip2`, `redox_syscall` (the 60-component delta) — which is why the name is
accurate rather than overstated.

**Development-only dependencies are excluded.** `axum`, `test-log`, and
`tempfile` (dev-dependencies) do not appear in either distributable SBOM
(verified: 0 occurrences). cargo-cyclonedx `--describe crate` emits the runtime
dependency graph only.

### Filenames

```
signal-kit-<version>-default.cdx.json
signal-kit-<version>-all-features-all-targets.cdx.json
```

Every CycloneDX document is validated (structural checks + the official CycloneDX
CLI schema, `--fail-on-errors`) before publishing; a malformed/invalid SBOM fails
the release. The SBOM's `metadata.component` is `signal-kit@<version>` (the
subject the attestations bind to).

### What the SBOM does and does not guarantee

**Does:** authoritatively inventory signal-kit's resolved cargo dependency graph
for the stated feature/target configuration, from the same lockfile as the build;
reproduce byte-for-byte per commit. **Does not:** describe dependencies a
downstream consumer adds; resolve to the consumer's target (consumers on unusual
targets should regenerate with `cargo cyclonedx --target <triple>`); include
dev/build-only dependencies as runtime components; guarantee absence of
vulnerabilities with no published RustSec advisory at generation time.

---

## Scanning tools

| Tool (pinned) | Role | Runs in |
|---|---|---|
| `cargo-deny` 0.19.0 | Policy gate: RustSec advisories, licences, duplicates, sources (`deny.toml`). | PR + release |
| `cargo-audit` 0.22.2 | RustSec vulnerability scan of `Cargo.lock`. (≥0.22.2 required: 0.21.x cannot parse CVSS 4.0 advisories in the live DB.) | PR + release |
| `cargo-cyclonedx` 0.5.9 | CycloneDX SBOM generation. | PR + release |
| CycloneDX CLI 0.32.0 | Authoritative SBOM schema validation (checksum-verified download). | PR + release |

Results appear in four GitHub-native places — no third-party service: workflow
logs, the job summary (`$GITHUB_STEP_SUMMARY`), GitHub code scanning (SARIF from
`cargo-deny` + `cargo-audit`), and workflow artefacts (the SBOM, 7-day
retention).

**No Grype/Trivy/Syft.** signal-kit's artefact is source, not a binary/image.
`cargo-audit` is the authoritative RustSec scan of that source's dependency
surface, and `cargo-cyclonedx` the authoritative SBOM. An independent
Grype/Trivy/Syft pass on a source `.crate` would re-derive the same manifest data
less precisely — the redundancy the policy avoids.

### cargo-auditable

`cargo-auditable` embeds the dependency tree into an executable at link time.
signal-kit currently has **no binary/cdylib target** (`scripts/binary-targets.sh`
returns empty), so there is no executable to embed into and auditable is
**skipped** — never falsely claimed. The release build uses `cargo auditable
build` only when a binary target exists, so the moment one is added it is
automatically auditable (and `cargo audit bin` verifies the embedded metadata).

---

## Vulnerability failure policy

| Condition | Result |
|---|---|
| Any `cargo-deny` advisory (vulnerability / unsound / yanked) | **FAIL** |
| Any **unmaintained** crate (direct or transitive; `unmaintained = "all"`) | **FAIL** |
| Any licence outside the allow-list / any copyleft licence | **FAIL** |
| Any dependency from a non-crates.io registry or git source | **FAIL** |
| Any `cargo-audit` RustSec vulnerability | **FAIL** |
| Malformed / schema-invalid CycloneDX SBOM | **FAIL** |
| Release-identity mismatch (tag ≠ version ≠ SHA ≠ Release) | **FAIL** |

This runs on every PR, on `main`, weekly, and again at **release time** with a
freshly fetched advisory database. A release is never partially published: the
gate runs first; assets are assembled, uploaded and attested only if it passes.
Scanner steps capture exit codes and upload SARIF **before** the final
`gate` job fails — nothing is hidden behind `continue-on-error`.

### Unmaintained policy (decision + rationale)

signal-kit blocks **all** unmaintained advisories (direct and transitive).
cargo-deny cannot split "block direct, warn transitive", so this is the stricter
of the two practical choices. Rationale: the dependency tree runs through
security-sensitive code (`ring`, `rustls`, `rustls-webpki`, tonic TLS), where an
unmaintained crate anywhere on the path is a real risk. The availability cost — a
release may be blocked on an unmaintained transitive crate — is accepted and
resolved by updating the dependency or adding a narrow exception. To relax to
warn-only, set `unmaintained = "none"` in `deny.toml` (cargo-audit then surfaces
unmaintained as warnings) and document the change here.

### Advisory exceptions

**No blanket ignores.** An advisory can only pass the gate via a named, owned,
expiry-dated entry in `deny.toml`:

```toml
ignore = [
    { id = "RUSTSEC-0000-0000", reason = "…", owner = "@liamwh", expiry = "2026-12-31" },
]
```

Exceptions are reviewed on/before their expiry; never extended without
re-evaluating the advisory.

---

## Running the checks locally

`justfile` mirrors CI (needs `cargo`, `just`, and `cargo install --locked cargo-deny cargo-audit cargo-cyclonedx`).

| Command | What it does |
|---|---|
| `just security` | `deny` + `audit` |
| `just test-supply-chain` | Release-identity, SBOM-mapping, gate, determinism, checksum, content self-tests |
| `just deny` / `just audit` | cargo-deny / cargo-audit |
| `just publish-policy` | Assert crates.io-publishable policy |
| `just binary-targets` | List binary/cdylib targets (empty = library) |
| `just sbom` / `just sbom-all` | Generate canonical / canonical+maximal SBOMs |
| `just sbom-validate` | Validate the canonical SBOM (schema if CLI present) |
| `just release-dry-run` | Assemble release artefacts into `target/release-dry-run` |
| `just audit-binary <path>` | Inspect a cargo-auditable binary (if one exists) |

---

## Inspecting an SBOM

```sh
gh release download v0.1.0 --repo liamwh/signal-kit --pattern 'signal-kit-*-default.cdx.json'

jq '{bomFormat, specVersion, component: .metadata.component, components: (.components|length)}' \
    signal-kit-0.1.0-default.cdx.json
jq -r '.components[] | "\(.name)\t\(.version // "-")"' signal-kit-0.1.0-default.cdx.json

just cyclonedx-cli   # download + checksum-verify the validator
CYCLONEDX_CLI="$(ls target/.tools/cyclonedx-*)" \
    bash scripts/sbom-validate.sh signal-kit-0.1.0-default.cdx.json
```

---

## Verifying checksums

```sh
gh release download v0.1.0 --repo liamwh/signal-kit \
    --pattern 'signal-kit-0.1.0.crate' --pattern 'signal-kit-0.1.0-*.cdx.json' --pattern 'SHA256SUMS'
sha256sum -c SHA256SUMS        # GNU
shasum -a 256 -c SHA256SUMS    # macOS/BSD
```

---

## Verifying GitHub attestations

```sh
gh attestation verify signal-kit-0.1.0.crate --repo liamwh/signal-kit
```

This verifies every attestation whose subject is that file's digest — both build
provenance and the SBOM binding (the SBOM attestation's predicate is the
CycloneDX document bound to that crate). Filter with
`--predicate-type https://slsa.dev/provenance/v1` (provenance) or
`https://spdx.dev/spdx-cdx-attestation` (SBOM).

The release workflow itself performs this verification end-to-end after upload:
it downloads the published assets, runs `gh attestation verify` on the `.crate`,
and re-checks `SHA256SUMS`, failing the release if a consumer could not verify.

---

## Repository settings

Some controls are enforced by repository files; others must be configured by a
repository/organisation administrator in GitHub's UI. They are clearly separated.

### Enforced by repository files (version-controlled)

- **Action pinning:** every third-party action is pinned to a full commit SHA with an inline tag comment (auditable in `.github/workflows/*`).
- **Least-privilege permissions:** each workflow/job declares only the permissions it needs; default workflow tokens are read-only by intent (see below).
- **Supply-chain gate as a check:** the `Supply chain` workflow always produces a check on PRs; the **`Supply chain / Gate (required)`** job is the stable final gate.
- **Publish policy:** `scripts/check-publish-policy.sh` asserts signal-kit is crates.io-publishable (Cargo.toml publishable + `release-plz.toml publish = true`).
- **Dependabot:** `.github/dependabot.yml` keeps GitHub-Actions SHA pins and Cargo deps patched (grouped, labelled).

### Must be configured by an administrator (GitHub UI / crates.io)

- **crates.io publish token:** create the `CARGO_REGISTRY_TOKEN` Actions secret (a crates.io API token scoped to `publish-new`). release-plz uses it to publish. (Upgrade path: Trusted Publishing — see [crates.io publishing](#crates-io-publishing).)
- **First crates.io publish:** release-plz publishes every version, but a brand-new crate must exist on crates.io first. Publish `v0.1.0` once (`cargo login` + `cargo publish` locally, or let the first release-plz run create it with the token) before relying on automation.
- **Required branch-protection check:** add `Supply chain / Gate (required)` to the `main` branch ruleset as a required status check. Do **not** require `scan` or `sbom` individually — `sbom` is intentionally skipped when no dependency files change, so requiring it would block unrelated PRs. Require only the `Gate (required)` job.
- **Default workflow token permissions:** set repository default to **Read repository contents and packages permissions** (read-only). Write permissions are granted per-job only where needed (release uploads/attestations).
- **Tag protection:** create a tag-protection rule restricting `v*` tags to repository administrators (release-plz creates these).
- **Dependency graph & Dependabot alerts:** enable *Settings → Code security → Dependency graph* and *Dependabot security updates / alerts*.
- **Private vulnerability reporting:** enable *Settings → Code security → Private vulnerability reporting*.
- **Artifact attestations:** confirm the repository/organisation permits artifact attestations (enabled by default; required by the release workflow's `id-token`/`attestations` permissions).
- **Release settings:** keep "automatically generated release notes" off (release-plz owns the changelog/release body).

---

## Adding a binary target

If `signal-kit` gains a `[[bin]]`/cdylib target, `scripts/binary-targets.sh`
auto-detects it and the release workflow switches to `cargo auditable build` +
`cargo audit bin` automatically. Add per-target SBOMs and a binary scanner
(Grype/Trivy) at that point; the assemble/attest/verify steps are already
parameterised for it.
