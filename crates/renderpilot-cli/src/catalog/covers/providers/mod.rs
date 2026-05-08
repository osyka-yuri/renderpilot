//! Remote cover resolution: Steam CDN, GOG catalog + CDN, SteamGridDB.
//!
//! Each source can be turned off via catalog settings (see [`super::policy::CoverRemotePolicy`]).
//! Defaults match previous behavior: all sources enabled when settings are absent.
//!
//! ## Resolution order (unchanged; toggles skip steps)
//!
//! | Launcher | Steps | `SteamGridDbApiKeyMissing` |
//! |----------|-------|---------------------------|
//! | **Steam** | Steam CDN → GridDB `steam-{app_id}` | When GridDB step is allowed but key missing |
//! | **Gog** | GOG CDN → GridDB `gog-{id}` → autocomplete | Same |
//! | **Other** | GridDB autocomplete only | When GridDB allowed but key missing |
//!
//! If SteamGridDB is disabled in policy, GridDB steps are skipped (`CoverNotFound` instead).
//!
//! CONTRACT: Launcher/policy branching here must stay aligned with background-sync eligibility in the
//! desktop UI (`apps/desktop/ui/src/shared/covers/cover-sync.ts`,
//! `gameMayReceiveRemoteCoverViaPolicy` / `filterGamesMissingStoredCoverForBackgroundSync`).
//!
//! Orchestration lives in [`resolve`] (`resolve_cover_bytes`); network helpers are under
//! [`download`], [`gog`], [`steam_cdn`], [`steamgriddb`].

mod backend;
mod download;
mod gog;
mod resolve;
mod steam_cdn;
mod steamgriddb;

pub(crate) use resolve::resolve_cover_bytes;
