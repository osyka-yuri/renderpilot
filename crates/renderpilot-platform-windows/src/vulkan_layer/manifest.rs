use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::pe::read_pe_architecture;
use super::types::{
    VulkanLayerArchitecture, VulkanLayerDiagnostic, VulkanLayerFacts, VulkanLayerState,
    VulkanLoaderVisibility,
};
use super::util::{manifest_path_looks_reshade, same_path};
use super::{LAYER_DLL_NAME, LAYER_JSON_NAME, LAYER_NAME};

/// Serializable Vulkan implicit-layer manifest in the loader's schema.
/// Matches the official ReShade manifest format exactly.
#[derive(Serialize)]
struct LayerManifest {
    file_format_version: &'static str,
    layer: LayerEntry,
}

#[derive(Serialize)]
struct LayerEntry {
    name: &'static str,
    #[serde(rename = "type")]
    layer_type: &'static str,
    library_path: &'static str,
    api_version: &'static str,
    implementation_version: &'static str,
    description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_extensions: Option<Vec<DeviceExtension>>,
    disable_environment: DisableEnvironment,
}

#[derive(Serialize)]
struct DeviceExtension {
    name: &'static str,
    spec_version: &'static str,
    entrypoints: [&'static str; 1],
}

#[derive(Serialize)]
struct DisableEnvironment {
    #[serde(rename = "DISABLE_VK_LAYER_reshade_1")]
    disable: &'static str,
}

/// Builds the Vulkan layer manifest JSON. Matches the official ReShade manifest
/// format: relative `library_path`, `device_extensions`, `disable_environment`.
#[must_use]
pub(crate) fn layer_manifest_json() -> String {
    let manifest = LayerManifest {
        file_format_version: "1.0.0",
        layer: LayerEntry {
            name: LAYER_NAME,
            layer_type: "GLOBAL",
            library_path: r".\ReShade64.dll",
            api_version: "1.3.268",
            implementation_version: "1",
            description: "crosire's ReShade post-processing injector for 64-bit",
            device_extensions: Some(vec![DeviceExtension {
                name: "VK_EXT_tooling_info",
                spec_version: "1",
                entrypoints: ["vkGetPhysicalDeviceToolPropertiesEXT"],
            }]),
            disable_environment: DisableEnvironment { disable: "1" },
        },
    };
    serde_json::to_string_pretty(&manifest).expect("layer manifest serializes")
}

// -----------------------------------------------------------------------------
// Manifest classification (pure)
// -----------------------------------------------------------------------------

/// A ReShade layer candidate discovered from a registered manifest.
pub(crate) struct LayerCandidate {
    pub(crate) state: VulkanLayerState,
    pub(crate) facts: VulkanLayerFacts,
    pub(crate) diagnostics: Vec<VulkanLayerDiagnostic>,
}

/// A registered manifest, classified for [`super::detect_report`].
pub(crate) enum ManifestKind {
    Candidate(LayerCandidate),
    Broken {
        diagnostic: VulkanLayerDiagnostic,
        facts: VulkanLayerFacts,
    },
    Other,
}

pub(crate) fn classify_manifest(
    manifest_path: &Path,
    layer_dir: &Path,
    active: bool,
) -> ManifestKind {
    // A registry value must point to an absolute manifest path. Relative paths
    // are a broken registration — the Vulkan loader resolves them against an
    // unpredictable working directory.
    if !manifest_path.is_absolute() {
        return ManifestKind::Broken {
            diagnostic: VulkanLayerDiagnostic::ManifestMalformed,
            facts: VulkanLayerFacts {
                manifest_path: Some(manifest_path.to_path_buf()),
                ..VulkanLayerFacts::default()
            },
        };
    }
    let is_app_manifest = same_path(manifest_path, &layer_dir.join(LAYER_JSON_NAME));
    if is_app_manifest {
        // The registry points at the standard manifest path. The manifest file
        // itself may be missing (stale registry key with partial cleanup).
        if !manifest_path.is_file() {
            return ManifestKind::Broken {
                diagnostic: VulkanLayerDiagnostic::MissingManifest,
                facts: VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(layer_dir.join(LAYER_DLL_NAME)),
                    ..VulkanLayerFacts::default()
                },
            };
        }
        // The manifest file exists — validate it parses as JSON. A corrupt
        // manifest means the Vulkan loader cannot use the layer even if the
        // DLL is present.
        if let Ok(content) = std::fs::read_to_string(manifest_path)
            && serde_json::from_str::<serde_json::Value>(&content).is_err()
        {
            return ManifestKind::Broken {
                diagnostic: VulkanLayerDiagnostic::ManifestMalformed,
                facts: VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(layer_dir.join(LAYER_DLL_NAME)),
                    ..VulkanLayerFacts::default()
                },
            };
        }
        let dll_path = layer_dir.join(LAYER_DLL_NAME);
        let architecture = match read_pe_architecture(&dll_path) {
            Ok(arch) => arch,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return ManifestKind::Broken {
                    diagnostic: VulkanLayerDiagnostic::MissingLayerDll,
                    facts: VulkanLayerFacts {
                        manifest_path: Some(manifest_path.to_path_buf()),
                        dll_path: Some(dll_path),
                        ..VulkanLayerFacts::default()
                    },
                };
            }
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(5) =>
            {
                return ManifestKind::Broken {
                    diagnostic: VulkanLayerDiagnostic::PermissionDenied,
                    facts: VulkanLayerFacts {
                        manifest_path: Some(manifest_path.to_path_buf()),
                        dll_path: Some(dll_path),
                        ..VulkanLayerFacts::default()
                    },
                };
            }
            Err(_) => {
                return ManifestKind::Broken {
                    diagnostic: VulkanLayerDiagnostic::UnreadableDll,
                    facts: VulkanLayerFacts {
                        manifest_path: Some(manifest_path.to_path_buf()),
                        dll_path: Some(dll_path),
                        ..VulkanLayerFacts::default()
                    },
                };
            }
        };
        if architecture == VulkanLayerArchitecture::X86 {
            return ManifestKind::Broken {
                diagnostic: VulkanLayerDiagnostic::UnsupportedArchitecture,
                facts: VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(dll_path),
                    architecture,
                    ..VulkanLayerFacts::default()
                },
            };
        }
        // Standard location with a valid manifest and a readable DLL. If the
        // registry entry is disabled (DWORD != 0), the loader will skip it.
        if !active {
            return ManifestKind::Candidate(LayerCandidate {
                state: VulkanLayerState::InstalledDisabled,
                facts: VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(dll_path),
                    version: None,
                    architecture,
                    loader_visibility: VulkanLoaderVisibility::Normal,
                },
                diagnostics: vec![VulkanLayerDiagnostic::RegistryDisabled],
            });
        }
        // In the standard location with a valid DLL — this is our layer (or
        // the official ReShade's, which is the same thing). Registered under
        // HKLM, so it is visible to all processes including elevated ones.
        return ManifestKind::Candidate(LayerCandidate {
            state: VulkanLayerState::Installed,
            facts: VulkanLayerFacts {
                manifest_path: Some(manifest_path.to_path_buf()),
                dll_path: Some(dll_path),
                version: None,
                architecture,
                loader_visibility: VulkanLoaderVisibility::Normal,
            },
            diagnostics: Vec::new(),
        });
    }
    match inspect_manifest(manifest_path) {
        Ok(Some(candidate)) => ManifestKind::Candidate(candidate),
        Ok(None) => ManifestKind::Other,
        Err((diagnostic, facts)) => ManifestKind::Broken { diagnostic, facts },
    }
}

pub(crate) fn inspect_manifest(
    manifest_path: &Path,
) -> Result<Option<LayerCandidate>, (VulkanLayerDiagnostic, VulkanLayerFacts)> {
    let content = match std::fs::read_to_string(manifest_path) {
        Ok(content) => content,
        // A read failure on a ReShade-looking manifest is worth surfacing, but
        // permission denial and a genuinely broken manifest are different
        // problems with different fixes — keep them distinguishable.
        Err(error) if manifest_path_looks_reshade(manifest_path) => {
            let diagnostic = if error.kind() == io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(5)
            {
                VulkanLayerDiagnostic::PermissionDenied
            } else {
                VulkanLayerDiagnostic::ManifestMalformed
            };
            return Err((
                diagnostic,
                VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    ..VulkanLayerFacts::default()
                },
            ));
        }
        Err(_) => return Ok(None),
    };
    let parsed = match serde_json::from_str::<ForeignManifest>(&content) {
        Ok(parsed) => parsed,
        Err(_) if manifest_path_looks_reshade(manifest_path) => {
            return Err((
                VulkanLayerDiagnostic::ManifestMalformed,
                VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    ..VulkanLayerFacts::default()
                },
            ));
        }
        Err(_) => return Ok(None),
    };
    let dll = resolve_library_path(manifest_path, &parsed.layer.library_path);
    let looks_reshade = parsed.layer.name.eq_ignore_ascii_case(LAYER_NAME)
        || dll
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("reshade"));
    if !looks_reshade {
        return Ok(None);
    }
    if !dll.is_file() {
        return Err((
            VulkanLayerDiagnostic::MissingLayerDll,
            VulkanLayerFacts {
                manifest_path: Some(manifest_path.to_path_buf()),
                dll_path: Some(dll),
                loader_visibility: VulkanLoaderVisibility::Normal,
                ..VulkanLayerFacts::default()
            },
        ));
    }
    let mut architecture = match read_pe_architecture(&dll) {
        Ok(arch) => arch,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err((
                VulkanLayerDiagnostic::MissingLayerDll,
                VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(dll),
                    loader_visibility: VulkanLoaderVisibility::Normal,
                    ..VulkanLayerFacts::default()
                },
            ));
        }
        Err(error)
            if error.kind() == io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(5) =>
        {
            return Err((
                VulkanLayerDiagnostic::PermissionDenied,
                VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(dll),
                    loader_visibility: VulkanLoaderVisibility::Normal,
                    ..VulkanLayerFacts::default()
                },
            ));
        }
        Err(_) => {
            return Err((
                VulkanLayerDiagnostic::UnreadableDll,
                VulkanLayerFacts {
                    manifest_path: Some(manifest_path.to_path_buf()),
                    dll_path: Some(dll),
                    loader_visibility: VulkanLoaderVisibility::Normal,
                    ..VulkanLayerFacts::default()
                },
            ));
        }
    };
    if architecture == VulkanLayerArchitecture::Unknown {
        architecture = architecture_from_manifest(parsed.layer.library_arch.as_deref());
    }
    if architecture == VulkanLayerArchitecture::X86 {
        return Err((
            VulkanLayerDiagnostic::UnsupportedArchitecture,
            VulkanLayerFacts {
                manifest_path: Some(manifest_path.to_path_buf()),
                dll_path: Some(dll),
                architecture,
                loader_visibility: VulkanLoaderVisibility::Normal,
                ..VulkanLayerFacts::default()
            },
        ));
    }
    Ok(Some(LayerCandidate {
        state: VulkanLayerState::External,
        facts: VulkanLayerFacts {
            manifest_path: Some(manifest_path.to_path_buf()),
            dll_path: Some(dll),
            architecture,
            loader_visibility: VulkanLoaderVisibility::Normal,
            ..VulkanLayerFacts::default()
        },
        diagnostics: Vec::new(),
    }))
}

/// Minimal view of an external layer manifest for ReShade identification.
#[derive(Deserialize)]
struct ForeignManifest {
    #[serde(default)]
    layer: ForeignLayer,
}

#[derive(Deserialize, Default)]
struct ForeignLayer {
    #[serde(default)]
    name: String,
    #[serde(default)]
    library_path: String,
    #[serde(default)]
    library_arch: Option<String>,
}

pub(crate) fn resolve_library_path(manifest_path: &Path, library_path: &str) -> PathBuf {
    let stripped = library_path.strip_prefix(".\\").unwrap_or(library_path);
    let candidate = PathBuf::from(stripped);
    if candidate.is_absolute() {
        return candidate;
    }
    manifest_path
        .parent()
        .map_or_else(|| candidate.clone(), |dir| dir.join(&candidate))
}

fn architecture_from_manifest(library_arch: Option<&str>) -> VulkanLayerArchitecture {
    match library_arch.map(str::trim) {
        Some("64") | Some("x64") | Some("X64") => VulkanLayerArchitecture::X64,
        Some("32") | Some("x86") | Some("X86") => VulkanLayerArchitecture::X86,
        _ => VulkanLayerArchitecture::Unknown,
    }
}
