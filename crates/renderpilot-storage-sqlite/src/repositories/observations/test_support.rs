//! Test-only complete-scan publication support.

use renderpilot_domain::{GameInstallation, LibraryComponent};

use super::*;

impl SqliteStorage {
    /// Publishes a test fixture through the same validated complete-scan path
    /// used by production scans.
    ///
    /// The synthetic facts carry a deliberately non-Windows identity kind, so
    /// they cannot match an observed filesystem object. A real scan therefore
    /// replaces them instead of treating them as reusable file facts. This is
    /// unavailable in normal production builds.
    #[cfg(feature = "test-instrumentation")]
    pub fn store_complete_components_for_test(
        &self,
        game: &GameInstallation,
        components: &[LibraryComponent],
    ) -> AppResult<CatalogReadyProjection> {
        const TEST_FIXTURE_SHA256: &str =
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let fixture_sha256 = Sha256Hash::new(TEST_FIXTURE_SHA256).map_err(invalid_row)?;

        let observations = components
            .iter()
            .enumerate()
            .flat_map(|(component_index, component)| {
                let fixture_sha256 = &fixture_sha256;
                component
                    .files()
                    .iter()
                    .enumerate()
                    .map(move |(file_index, file)| StoredFileObservation {
                        owner: ObservationOwner::Game(game.id().clone()),
                        normalized_path: file.path().clone(),
                        identity_kind: "renderpilot_test_fixture_non_windows_identity_v1"
                            .to_owned(),
                        object_identity: format!(
                            "fixture-component-{component_index}-file-{file_index}"
                        ),
                        change_token: format!("fixture-change-{component_index}-{file_index}"),
                        size: 0,
                        algorithm_revision: u32::MAX,
                        sha256: file
                            .sha256()
                            .cloned()
                            .unwrap_or_else(|| fixture_sha256.clone()),
                        version_observed: true,
                        version: file.version().cloned(),
                        runtime_observed: false,
                        runtime_json: None,
                        pe_observed: false,
                        pe_json: None,
                    })
            })
            .collect::<Vec<_>>();
        let authority = AuthorityCas::new(self.catalog_readiness(game.id())?.authority_epoch());

        self.save_complete_scan_write_unit(super::super::CompleteScanWriteUnit {
            game,
            components,
            artifacts: &[],
            observations: &observations,
            authority,
            prune_empty_operations: false,
        })
    }
}
