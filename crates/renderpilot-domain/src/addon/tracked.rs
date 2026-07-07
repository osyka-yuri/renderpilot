use serde::{Deserialize, Serialize};

use super::RenoDxHostKind;

/// The role a [`TrackedSource`] plays in an install, so the update system knows
/// what each upstream URL points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedSourceRole {
    /// The add-on payload itself (e.g. the RenoDX `.addon64`); its file is a member
    /// of [`super::InstalledAddon::created_files`].
    AddonPayload,
    /// The DLSS-Fix companion add-on (`renodx-dlssfix.addon64`), installed alongside
    /// the main add-on when the game has NVIDIA Frame Generation + DLSS + Streamline.
    DlssFix,
    /// A host binary artifact recorded for update/rollback provenance (e.g. the
    /// ReShade proxy binary).
    #[serde(rename = "host")]
    HostBinary,
}

/// Host mechanism used by an installed add-on.
///
/// This is persisted as per-game metadata so reversible flows do not have to
/// re-derive the host from mutable facts such as the currently selected
/// executable override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledAddonHostKind {
    /// A per-game ReShade proxy DLL, such as `dxgi.dll`.
    Proxy,
    /// The shared ReShade Vulkan implicit layer.
    SharedVulkanLayer,
}

impl From<InstalledAddonHostKind> for RenoDxHostKind {
    fn from(value: InstalledAddonHostKind) -> Self {
        match value {
            InstalledAddonHostKind::Proxy => Self::Proxy,
            InstalledAddonHostKind::SharedVulkanLayer => Self::Vulkan,
        }
    }
}

/// One upstream source an install tracks for updates: where a placed file came
/// from and the identity needed to tell whether it changed.
///
/// Tool-agnostic by design — an install records one entry per managed download
/// (RenoDX tracks the add-on and, when it installs one, the ReShade host; a future
/// tool tracks whatever it fetches), so the update system is generic over the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedSource {
    role: TrackedSourceRole,
    /// Upstream URL the file was fetched from.
    url: String,
    /// HTTP cache validator (ETag/Last-Modified) for a cheap update pre-check.
    etag: Option<String>,
    /// SHA-256 of the installed bytes — the durable change-detection digest.
    digest: String,
    /// The raw `Last-Modified` HTTP-date string from the download response, when
    /// the host sent one. Surfaced to the UI as the upstream "dated" anchor (a
    /// rolling-snapshot add-on has no version number). Optional and
    /// `#[serde(default)]` so records persisted before this field deserialize as
    /// `None`.
    #[serde(default)]
    last_modified: Option<String>,
    /// Tool-owned source variant/provenance tag. Generic at the domain layer:
    /// RenoDX uses `stable` / `nightly` for ReShade Host sources.
    #[serde(default)]
    channel: Option<String>,
    /// Whether this source was reconstructed from on-disk facts (e.g. adopting an
    /// install RenderPilot did not create) rather than recorded from an actual
    /// download. An advisory source's URL/digest are a best-effort guess, not
    /// proof of what is on disk. `#[serde(default)]` so records persisted before
    /// this field deserialize as `false`.
    #[serde(default)]
    advisory: bool,
}

impl TrackedSource {
    /// Creates a tracked source for a managed download. The upstream `Last-Modified`
    /// date is attached separately via [`with_last_modified`](Self::with_last_modified).
    #[must_use]
    pub fn new(
        role: TrackedSourceRole,
        url: impl Into<String>,
        etag: Option<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            role,
            url: url.into(),
            etag,
            digest: digest.into(),
            last_modified: None,
            channel: None,
            advisory: false,
        }
    }

    /// Attaches the upstream `Last-Modified` HTTP-date string (the file's publish
    /// date proxy), shown by the UI as the source's "dated" anchor.
    #[must_use]
    pub fn with_last_modified(mut self, last_modified: Option<String>) -> Self {
        self.last_modified = last_modified;
        self
    }

    /// Attaches a tool-owned source variant/provenance tag.
    #[must_use]
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    /// Marks this source as reconstructed from on-disk facts rather than a
    /// recorded download.
    #[must_use]
    pub fn with_advisory(mut self) -> Self {
        self.advisory = true;
        self
    }

    /// Returns the role this source plays in the install.
    #[must_use]
    pub fn role(&self) -> TrackedSourceRole {
        self.role
    }

    /// Returns the upstream URL the file was fetched from.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the HTTP cache validator for a cheap update pre-check.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }

    /// Returns the change-detection digest (SHA-256 of the installed bytes).
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Returns the upstream `Last-Modified` HTTP-date string, when the host sent one.
    #[must_use]
    pub fn last_modified(&self) -> Option<&str> {
        self.last_modified.as_deref()
    }

    /// Returns the tool-owned source variant/provenance tag, when present.
    #[must_use]
    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    /// Returns whether this source was reconstructed from on-disk facts rather
    /// than recorded from an actual download.
    #[must_use]
    pub fn is_advisory(&self) -> bool {
        self.advisory
    }
}
