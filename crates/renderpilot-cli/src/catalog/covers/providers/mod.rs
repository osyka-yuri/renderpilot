//! Remote cover resolution: Steam CDN, GOG catalog + CDN, SteamGridDB.
//!
//! Each source can be turned off via catalog settings (see [`super::policy::CoverRemotePolicy`]).
//! Defaults match previous behavior: all sources enabled when settings are absent.
//!
//! ## Resolution order (toggles skip steps)
//!
//! | Launcher | Steps |
//! |----------|-------|
//! | **Steam** | Steam CDN → GridDB `steam-{app_id}` |
//! | **Gog** | GOG CDN → GridDB `gog-{id}` → autocomplete |
//! | **Other** | GridDB autocomplete only |
//!
//! GridDB steps are skipped (`CoverNotFound` instead) when *any* of the
//! following holds, so background cover sync never produces per-game
//! "Add a SteamGridDB API key" warnings purely because of a global
//! configuration state:
//!
//! * `policy.steamgriddb` is `false`.
//! * The configured SteamGridDB API key is missing or blank.
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
