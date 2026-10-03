# Catalogs and producer contracts

RenderPilot consumes versioned library and add-on data produced outside the application repository. Remote data is untrusted input: transport, host, schema, size, content identity, and publication relationships are validated before it becomes active state.

## Libraries V2

The client requests `libraries/v1/index.json` from the configured HTTPS CDN host. The index identifies immutable vendor snapshots under `libraries/v1/vendors/<vendor>/<sha>.json`. Packages refer to content-addressed compressed blobs, while the local cache stores the active catalog at `libraries/v1/catalog.json`, downloaded archives under `libraries/v1/blobs`, and extracted files under `libraries/v1/artifacts/<sha>/<filename>`.

Validation covers allowed host and scheme, bounded response sizes, expected SHA-256 values, catalog structure, package revisions, technology identity, architecture, filenames, compression and extraction results. A failed refresh preserves the last known good catalog. Corrupt downloads are not promoted as active content.

A package receipt binds a planned operation to the catalog revision and payload identity used to produce it. Refreshing a catalog must not silently change the meaning of an already reviewed plan; apply validates its source evidence again.

The canonical producer is `osyka-yuri/renderpilot-libraries`. CI checks it out at the full commit in `RENDERPILOT_LIBRARIES_CONTRACT_REF`, runs the external localization-source check, and executes the ignored Rust golden-fixture test through `RENDERPILOT_LIBRARIES_FIXTURE`. The fixture command is intentionally exact so an absent or renamed test cannot pass unnoticed.

```bash
cargo test -p renderpilot-orchestration \
  libraries::revision::tests::producer_v2_golden_fixture_matches_rust_projection \
  --locked -- --ignored --exact
```

## Xiph Unreal packages

The X64-only variants `shared.unreal` and `embedded_ogg.unreal` use `libvorbis_64.dll`, `libvorbisfile_64.dll` and `libvorbisenc_64.dll`; shared packages also contain `libogg_64.dll`. Installation keeps these compiled basenames and writes only members already present in the game. Other vendor suffixes, including `_vs2010_x64_rwdi`, retain their import-driven alias policy.

Runtime parsing represents `_64` as an opaque `Lib` suffix to preserve component identity. Vendor-suffixed deployments require a complete game-root scan of regular and delay imports, including when no external bindings are found. Apply revalidates the proof; candidates must preserve required external basenames and the internal import graph.

CI pins the producer snapshot with `RENDERPILOT_XIPH_CONTRACT_REF`. To run its contract test locally, set `RENDERPILOT_XIPH_CATALOG` to that snapshot:

```bash
cargo test -p renderpilot-orchestration \
  libraries::tests::producer_xiph_catalog_matches_consumer_contract \
  --locked -- --ignored --exact
```

## Add-on manifests

Current tool manifests use versioned paths: RenoDX primary is `addons/v2/renodx.json` and Luma primary is `addons/v2/luma.json`. Luma v1 at `addons/v1/luma.json` remains a legacy-client compatibility contract; the current client does not fall back to it. If the RenoDX v2 CDN request is unavailable at the transport layer, the client may use the CDN v1 compatibility document at `addons/v1/renodx.json`; a valid but incompatible or malformed v2 response is terminal. ReShade remains on `addons/v1/reshade.json`; OptiScaler remains on `addons/v1/optiscaler.json`, with compatibility knowledge at `addons/v1/optiscaler-compatibility.json`. These documents define supported profiles, source locations, expected files, hashes, compatibility evidence, dependencies, and stable localization identifiers. Every user-visible catalog message needs a stable ID and a reviewed English fallback.

ReShade source data may use a bundled last-resort fallback after the CDN contract fails. OptiScaler's release manifest and compatibility catalog use their validated bundled snapshots or last-admitted cache after a remote failure, according to each store's immutable-history rules. Luma and RenoDX catalogs have no bundled fallback. Source-specific refresh policies may keep prior data available after a transient error; callers must preserve the distinction between cached, refreshed, and unavailable state.

## Producer change checklist

When the producer schema or projection changes, update validation and Rust/domain models, create or update the golden fixture, pin CI to the reviewed immutable producer commit, run the localization source checks, and confirm that old last-known-good data still has an intentional outcome. Do not mutate published content at an existing content address.

## Sources of truth

- [Library catalog client](../../crates/renderpilot-orchestration/src/libraries/catalog.rs)
- [Library validation](../../crates/renderpilot-orchestration/src/libraries/validation/mod.rs)
- [Package receipt](../../crates/renderpilot-domain/src/catalog_package/receipt.rs)
- [Bundled manifest policy](../../crates/renderpilot-orchestration/assets/README.md)
- [Quality workflow](../../.github/workflows/quality.yml)
