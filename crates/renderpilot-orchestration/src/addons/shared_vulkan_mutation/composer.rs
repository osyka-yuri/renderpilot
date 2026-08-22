//! Composition of independently planned game and shared-layer transitions.
//!
//! This module is intentionally boring: engine and platform planners own the
//! semantics of their respective resources, while this type only translates
//! their exact before/after observations into one SVAM participant set.  A
//! caller therefore cannot accidentally reimplement either planner while
//! assembling a combined transaction.

use std::path::PathBuf;

use renderpilot_platform_windows::vulkan_layer::{
    FileMutation, FileObservation, RegistryValueState, SharedVulkanLayerPlan,
};

use crate::ServiceError;
use crate::addons::renodx::game_participants::{GameFileIntent, GameParticipantPlan};
use crate::addons::shared_vulkan_mutation::{FileIntent, RegistryIntent};

/// Exact participants composed from one RenoDX game plan and one shared-layer
/// plan.  The values remain owned until the SVAM request is constructed so the
/// database projection and first filesystem write use the same snapshot.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ComposedParticipants {
    pub(crate) files: Vec<FileIntent>,
    pub(crate) registry: Vec<RegistryIntent>,
    pub(crate) created_dirs: Vec<PathBuf>,
}

/// Converts exact game and shared plans into one closed participant set.
///
/// The function rejects overlapping live participants.  Overlap is a planning
/// defect, not a runtime conflict: silently coalescing two independently
/// planned images would make ownership and rollback order ambiguous.
pub(crate) fn compose(
    game: Option<GameParticipantPlan>,
    shared: Option<SharedVulkanLayerPlan>,
) -> Result<ComposedParticipants, ServiceError> {
    let mut composed = ComposedParticipants::default();
    if let Some(game) = game {
        let (files, created_dirs) = game.into_parts();
        for intent in files {
            composed.push_file(game_file_intent(intent))?;
        }
        composed.created_dirs.extend(created_dirs);
    }
    if let Some(shared) = shared {
        let SharedVulkanLayerPlan {
            files,
            registry,
            directory,
            ..
        } = shared;
        for file in files {
            composed.push_file(file_intent(file))?;
        }
        if let Some(registry) = registry {
            composed.registry.push(RegistryIntent {
                manifest_path: registry.manifest_path,
                before: registry_value(registry.before),
                after: registry_value(registry.after),
            });
        }
        if directory.create_if_absent {
            composed.created_dirs.push(directory.path);
        }
    }
    composed
        .created_dirs
        .sort_by_key(|path| path.components().count());
    composed
        .created_dirs
        .dedup_by(|left, right| crate::paths::same_path(left, right));
    Ok(composed)
}

fn game_file_intent(intent: GameFileIntent) -> FileIntent {
    FileIntent {
        live_path: intent.live_path,
        before: intent.before,
        after: intent.after,
    }
}

impl ComposedParticipants {
    pub(crate) fn extend_files(
        &mut self,
        intents: impl IntoIterator<Item = FileIntent>,
    ) -> Result<(), ServiceError> {
        for intent in intents {
            self.push_file(intent)?;
        }
        Ok(())
    }

    pub(crate) fn prepend_files(
        &mut self,
        intents: impl IntoIterator<Item = FileIntent>,
    ) -> Result<(), ServiceError> {
        let existing = std::mem::take(&mut self.files);
        self.extend_files(intents)?;
        self.extend_files(existing)
    }

    fn push_file(&mut self, intent: FileIntent) -> Result<(), ServiceError> {
        if self
            .files
            .iter()
            .any(|existing| crate::paths::same_path(&existing.live_path, &intent.live_path))
        {
            return Err(ServiceError::invalid_input(format!(
                "combined mutation contains overlapping file participant `{}`",
                intent.live_path.display()
            )));
        }
        self.files.push(intent);
        Ok(())
    }
}

fn file_intent(file: FileMutation) -> FileIntent {
    FileIntent {
        live_path: file.path,
        before: observation_bytes(file.before),
        after: observation_bytes(file.after),
    }
}

fn observation_bytes(observation: FileObservation) -> Option<Vec<u8>> {
    match observation {
        FileObservation::Absent => None,
        FileObservation::Present(bytes) => Some(bytes),
    }
}

fn registry_value(
    value: RegistryValueState,
) -> crate::addons::shared_vulkan_mutation::RegistryValue {
    match value {
        RegistryValueState::Absent => crate::addons::shared_vulkan_mutation::RegistryValue::Absent,
        RegistryValueState::Present {
            value_type,
            raw_bytes,
        } => crate::addons::shared_vulkan_mutation::RegistryValue::Present {
            value_type,
            raw_bytes,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::shared_vulkan_mutation::RegistryValue;
    use renderpilot_platform_windows::vulkan_layer::{
        DirectoryMutation, DirectoryObservation, FileMutation, LayerPlanOperation, RegistryMutation,
    };

    #[test]
    fn composed_participants_reject_overlapping_live_paths() {
        let mut participants = ComposedParticipants::default();
        participants
            .push_file(FileIntent {
                live_path: std::path::Path::new("C:/game/dxgi.dll").to_path_buf(),
                before: None,
                after: Some(vec![1]),
            })
            .expect("first participant");
        let error = participants
            .push_file(FileIntent {
                live_path: std::path::Path::new("c:/GAME/DXGI.DLL").to_path_buf(),
                before: None,
                after: Some(vec![2]),
            })
            .expect_err("overlap must be rejected");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
    }

    #[test]
    fn composing_moves_shared_file_and_registry_preimages_without_cloning() {
        let file_before = vec![1, 2, 3];
        let file_before_ptr = file_before.as_ptr();
        let registry_before = vec![4, 5, 6, 7];
        let registry_before_ptr = registry_before.as_ptr();
        let plan = SharedVulkanLayerPlan {
            operation: LayerPlanOperation::Refresh,
            files: vec![FileMutation {
                path: PathBuf::from("shared/ReShade64.dll"),
                before: FileObservation::Present(file_before),
                after: FileObservation::Present(vec![8]),
            }],
            registry: Some(RegistryMutation {
                manifest_path: PathBuf::from("shared/ReShade64.json"),
                before: RegistryValueState::Present {
                    value_type: 4,
                    raw_bytes: registry_before,
                },
                after: RegistryValueState::Absent,
            }),
            directory: DirectoryMutation {
                path: PathBuf::from("shared"),
                before: DirectoryObservation {
                    exists: true,
                    entries: Vec::new(),
                },
                create_if_absent: false,
                remove_if_empty: false,
            },
            unregister_outcome: None,
        };

        let composed = compose(None, Some(plan)).expect("compose");
        assert_eq!(
            composed.files[0]
                .before
                .as_ref()
                .expect("file preimage")
                .as_ptr(),
            file_before_ptr
        );
        let RegistryValue::Present { raw_bytes, .. } = &composed.registry[0].before else {
            panic!("registry preimage must be present");
        };
        assert_eq!(raw_bytes.as_ptr(), registry_before_ptr);
    }
}
