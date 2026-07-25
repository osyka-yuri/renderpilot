# Bundled catalogue snapshots

`reshade-v1-fallback.json` is a release-pinned, mechanically copied snapshot of
`renderpilot-libraries/addons/v1/reshade.json`. It is used only after the CDN
and every parseable cache copy are unavailable.

The snapshot is not a second live source and must never be merged into a valid
remote document. Refresh it when shipping an application release, or when the
wire schema or source security policy changes; it need not follow every CDN
refresh between releases.

## Release gate: CDN catalogue paths

Before shipping an application release, confirm these remote documents are
published and fetchable (CDN + cache warm path):

| Catalogue              | Remote path                                              | Bundled fallback                                  |
| ---------------------- | -------------------------------------------------------- | ------------------------------------------------- |
| Shared ReShade sources | `addons/v1/reshade.json`                                 | `reshade-v1-fallback.json` (last resort)          |
| Luma tool catalogue    | `addons/v1/luma.json`                                    | **none** — install/update hard-fail if unresolved |
| RenoDX tool catalogue  | tool path under `addons/v1/` (see RenoDX manifest store) | **none**                                          |

Tool catalogues intentionally have no offline fallback: a stale or invented
profile set is worse than a clear fetch failure. Keep `luma.json` / RenoDX tool
JSON current on the CDN for every release that ships those features.

When the shared ReShade document cannot be loaded, the app logs a warning and
uses the release-pinned bundled snapshot for host downloads only.
