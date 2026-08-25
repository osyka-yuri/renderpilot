# Quality and release

The reusable quality workflow is the source of truth for main-branch and release gates. CI runs the Rust matrix on Ubuntu and Windows, the desktop suite on Ubuntu, and the Libraries V2 producer contract on Ubuntu. The release workflow invokes the same gates with the Rust matrix restricted to Windows before packaging.

## Local quality gates

The repository requires cargo-nextest 0.9.143 or newer. To match CI exactly,
install the pinned version once:

```powershell
cargo install cargo-nextest --locked --version 0.9.143
```

Run the complete Rust checks from the repository root:

```powershell
cargo fmt --all -- --check
cargo build --workspace --locked --all-features
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --profile ci --workspace --all-features --locked
cargo test --workspace --all-features --locked --doc --no-fail-fast
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --all-features --no-deps --locked
```

Run the desktop and documentation checks from `apps/desktop`:

```powershell
pnpm install --frozen-lockfile
pnpm run format:check
pnpm run lint
pnpm test
pnpm exec playwright install chromium
pnpm run test:a11y
pnpm run build
pnpm run docs:check
```

Formatting and Rustdoc run once on Ubuntu in CI. Workspace build, Clippy, nextest, and Cargo doctests run on Ubuntu and Windows with all features; Clippy and Rustdoc deny warnings. The nextest CI profile runs every test without retries, while the separate Cargo step preserves documentation-test coverage. The Windows lane also runs the deterministic portable-release ZIP packaging policy script plus the focused Windows-manifest selector and XML-contract tests. Desktop tests validate localization contracts, Chromium browser accessibility coverage, and repeat contracts, type checking, Vite compilation, and bundle-graph validation during the build. Chromium coverage does not validate the native WebView2 host or Windows screen-reader integration; the required packaged Windows NVDA/Narrator release gate is documented in [Accessibility](accessibility.md). Libraries V2 uses an immutable producer checkout, external localization-source verification, and the exact ignored fixture test described in [Catalogs and producer contracts](catalogs-and-contracts.md#libraries-v2).

## Release lifecycle

A `v*` tag must exactly match the desktop Cargo package version. After quality gates pass, the pinned `tauri-action` is used only to build and sign the Windows installer artifacts for that workflow run; it receives no release tag, release metadata, or GitHub token and does not upload updater metadata. Its step scopes `RENDERPILOT_WINDOWS_MANIFEST=production`, which embeds `requireAdministrator` in the standard App. The following `mt.exe` resource-#1 gate requires exactly one execution level, `uiAccess=false`, and the Common Controls v6 dependency before portable packaging overwrites the standard App image. The publisher consumes its current-run `artifactPaths`, requires exactly one versioned NSIS installer, and creates the byte-identical `RenderPilot-setup.exe` stable-download alias locally.

The workflow invokes `apps/desktop/scripts/release-portable-artifacts.ps1` to build the portable App image through the Tauri CLI with the `portable` feature and `--no-bundle`, build the stable raw supervisor, create and sign the public non-runnable RPU, embed the exact RPU and signature in the raw `RPSX1` overlay, sign the raw artifact, and package the versioned ZIP. The script scopes `production` independently around both Cargo release builds and restores the caller's environment after each one; `release-tooling` is never admitted into packaging. `mt.exe` requires `requireAdministrator`, `uiAccess=false`, and Common Controls v6 in the portable managed App before RPU construction, the raw supervisor, and the final overlay artifact without altering their signatures, RPU bytes, or ZIP identity. ZIP staging uses a new cryptographically unique temporary root without recursive cleanup; the archive is created once with exactly `RenderPilot/renderpilot-desktop.exe`. The release script validates raw/RPU/signature/ZIP byte identity before assets are uploaded. The updater manifest contains only `windows-x86_64-nsis` and `windows-x86_64-portable`; the portable target is the `.rpu`, so the installed updater cannot consume the raw portable supervisor.

The focused portable-runtime dynamic suite runs separately after static candidate clearance. It exercises signed RPU identity, raw/ZIP identity, managed-App startup admission, supervisor session lineage, job lifetime, activation/commit recovery, schema migration, and retained uncertain state in disposable roots. It does not publish assets or use production portable data.

Validate a locally assembled portable artifact without publishing it:

```powershell
node apps/desktop/scripts/portable-rpu.mjs validate --raw <raw.exe> --rpu <payload.rpu> --signature <payload.rpu.sig> --zip <portable.zip> --zip-entry RenderPilot/renderpilot-desktop.exe --expected-version <version>
```

Success reports SHA-256 identities for the raw supervisor, public RPU, and signature. The command is local-only and does not modify configured production credentials.

`prepare-release-assets.ps1` deterministically generates the final `latest.json` from those local NSIS artifacts, portable raw/RPU artifacts, the exact changelog section, and the tagged commit timestamp, creates the stable `RenderPilot-setup.exe` alias, and verifies all three release artifact signatures. The workflow then invokes `actions/attest` to generate and persist signed SLSA build provenance for all nine distribution assets in GitHub's attestation store before any assets are uploaded.

In the GitHub release workflow, `publish-release-assets.ps1` consumes the pre-prepared, attested distribution assets without mutating them. It creates or resumes only a private staging draft identified by the authenticated run ID and a provenance marker. Every asset upload is create-only by the exact release ID: a byte-identical existing asset is skipped, while a duplicate, changed, or unexpected asset fails closed. The complete digest-checked asset set is verified while the release is still a draft, then one PATCH publishes the final tag and metadata. A pre-existing final tag is read-only success only when its commit, metadata, and exact asset set all match; legacy drafts and mismatches require manual cleanup rather than automatic repair. The publisher captures the remote peeled tag target before staging and re-reads it immediately before and after that final PATCH, so a tag move fails closed. The portable ZIP entry must equal the signed raw supervisor, which must embed the exact public RPU and signature. Interactive UAC approve/cancel and protected-root behavior remain Windows release-machine checks because hosted CI cannot drive the secure desktop.

Release publication runs in the dedicated GitHub `release-publication` environment with the workflow token scoped to least privilege (`contents: write`, `id-token: write`, `attestations: write`). The publisher captures and rechecks the exact tag commit throughout publication and never deletes or repairs a conflicting release, asset, or tag. It does not depend on optional repository release or tag settings.

## Build provenance and artifact attestations

Artifact attestations allow downstream consumers to verify that a downloaded release artifact is cryptographically linked to a GitHub Actions build in the RenderPilot repository. Consumers that require the exact release identity can additionally enforce the release tag, source commit, and signer workflow. Artifact attestations are generated for every release published by the current release workflow.

Artifact attestation establishes provenance and integrity; it does not by itself assert that an artifact is vulnerability-free or otherwise safe.

```powershell
# Basic provenance verification against the RenderPilot repository
gh attestation verify RenderPilot-setup.exe --repo osyka-yuri/renderpilot

# Strict policy enforcing the exact release tag, commit SHA, and signer workflow
# Replace with the specific release version and its full 40-character commit SHA.
$version = "<RELEASE_VERSION>"       # e.g., "1.10.0"
$commitSha = "<RELEASE_COMMIT_SHA>"  # e.g., "7f8b9c..."
gh attestation verify RenderPilot-setup.exe `
  --repo osyka-yuri/renderpilot `
  --source-ref "refs/tags/v$version" `
  --source-digest $commitSha `
  --signer-workflow osyka-yuri/renderpilot/.github/workflows/release.yml
```

## Stable installer download

The README main download action uses the stable installer alias created for every release:

```text
https://github.com/osyka-yuri/renderpilot/releases/latest/download/RenderPilot-setup.exe
```

Release review must confirm that the stable alias and versioned installer have identical SHA-256 values. External link availability can also be checked during release review, but network checks are deliberately not required by documentation CI.

## Sources of truth

- [Quality workflow](../../.github/workflows/quality.yml)
- [Release workflow](../../.github/workflows/release.yml)
- [Tauri configuration](../../apps/desktop/src-tauri/tauri.conf.json)
- [Workspace changelog](../../CHANGELOG.md)
