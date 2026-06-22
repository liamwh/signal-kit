# Skip-path probe (temporary)

This file exists only to prove the supply-chain workflow's intentional-SBOM-skip
path: a documentation-only change outside the dependency-relevant path set must
keep the workflow visible, run detect+scan, skip the expensive SBOM job, and let
the final gate pass. Removed after verification.
