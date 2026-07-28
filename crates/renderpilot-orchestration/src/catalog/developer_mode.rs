//! Developer Mode prerequisite assessment for D3D12 Agility previews.

use renderpilot_application::{OperationPlan, OperationPlanBlocker};
use renderpilot_domain::{GraphicsTechnology, LibraryArtifact, ReleaseChannel};
use renderpilot_platform_windows::DeveloperModeStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeveloperModeRequirement {
    NotRequired,
    Required,
    Unverifiable,
}

/// Enriches an otherwise executable plan with its Developer Mode prerequisite.
///
/// Existing blockers take precedence: probing the environment cannot make such
/// a plan executable and would only add unrelated work and diagnostics.
pub(super) fn apply_developer_mode_prerequisite(
    context: &crate::Context,
    operation_plan: OperationPlan,
    artifact: &LibraryArtifact,
) -> OperationPlan {
    if !operation_plan.blockers().is_empty() {
        return operation_plan;
    }

    match developer_mode_requirement(artifact) {
        DeveloperModeRequirement::NotRequired => operation_plan,
        DeveloperModeRequirement::Unverifiable => operation_plan
            .with_prerequisite_blocker(OperationPlanBlocker::DeveloperModeCheckUnavailable),
        DeveloperModeRequirement::Required => match context.developer_mode_status() {
            DeveloperModeStatus::Enabled => operation_plan,
            DeveloperModeStatus::Disabled => operation_plan
                .with_prerequisite_blocker(OperationPlanBlocker::DeveloperModeRequired),
            DeveloperModeStatus::Unknown => operation_plan
                .with_prerequisite_blocker(OperationPlanBlocker::DeveloperModeCheckUnavailable),
        },
    }
}

fn developer_mode_requirement(artifact: &LibraryArtifact) -> DeveloperModeRequirement {
    if artifact.technology() != GraphicsTechnology::D3D12Agility {
        return DeveloperModeRequirement::NotRequired;
    }

    let Some(receipt) = artifact.metadata().catalog_package_receipt() else {
        // Without an authoritative receipt there is no safe way to distinguish
        // Preview from Stable/Beta/Debug, regardless of the artifact's source.
        return DeveloperModeRequirement::Unverifiable;
    };

    if !receipt
        .technology
        .eq_ignore_ascii_case(GraphicsTechnology::D3D12Agility.as_slug())
    {
        return DeveloperModeRequirement::Unverifiable;
    }

    match receipt.release.channel {
        ReleaseChannel::Preview => DeveloperModeRequirement::Required,
        ReleaseChannel::Stable | ReleaseChannel::Beta | ReleaseChannel::Debug => {
            DeveloperModeRequirement::NotRequired
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use renderpilot_application::{OperationPlanBlocker, build_swap_operation_plan};
    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, CatalogPackageReceiptV1,
        CatalogReceiptSchemaV1, CatalogSignatureReceipt, CatalogTargetReceipt, ComponentFile,
        ComponentId, ComponentKind, GameId, GraphicsComponent, GraphicsTechnology, LibraryArtifact,
        PackageRelease, PackageVersion, PathRef, ReleaseChannel, Sha256Hash, Swappability,
    };
    use renderpilot_platform_windows::DeveloperModeStatus;
    use renderpilot_storage_sqlite::SqliteStorage;

    use super::{
        DeveloperModeRequirement, apply_developer_mode_prerequisite, developer_mode_requirement,
    };
    use crate::Context;

    const ARTIFACT_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const COMPONENT_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn preview_maps_platform_status_to_the_expected_blocker() {
        let artifact = artifact(GraphicsTechnology::D3D12Agility, ReleaseChannel::Preview);
        assert_eq!(
            developer_mode_requirement(&artifact),
            DeveloperModeRequirement::Required
        );

        for (status, expected_blocker) in [
            (DeveloperModeStatus::Enabled, None),
            (
                DeveloperModeStatus::Disabled,
                Some(OperationPlanBlocker::DeveloperModeRequired),
            ),
            (
                DeveloperModeStatus::Unknown,
                Some(OperationPlanBlocker::DeveloperModeCheckUnavailable),
            ),
        ] {
            let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"))
                .with_developer_mode_status_provider(move || status);
            let plan = apply_developer_mode_prerequisite(
                &context,
                build_swap_operation_plan(&component(), &artifact).expect("plan"),
                &artifact,
            );

            assert_eq!(plan.blockers().first().copied(), expected_blocker);
        }
    }

    #[test]
    fn stable_and_non_d3d12_artifacts_do_not_require_a_probe() {
        let reads = Arc::new(AtomicUsize::new(0));
        let probe_reads = Arc::clone(&reads);
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"))
            .with_developer_mode_status_provider(move || {
                probe_reads.fetch_add(1, Ordering::SeqCst);
                DeveloperModeStatus::Unknown
            });

        for artifact in [
            artifact(GraphicsTechnology::D3D12Agility, ReleaseChannel::Stable),
            artifact(GraphicsTechnology::D3D12Agility, ReleaseChannel::Beta),
            artifact(GraphicsTechnology::D3D12Agility, ReleaseChannel::Debug),
        ] {
            assert_eq!(
                developer_mode_requirement(&artifact),
                DeveloperModeRequirement::NotRequired
            );
            let plan = apply_developer_mode_prerequisite(
                &context,
                build_swap_operation_plan(&component(), &artifact).expect("plan"),
                &artifact,
            );
            assert!(plan.blockers().is_empty());
        }

        let non_d3d12 = artifact(
            GraphicsTechnology::DlssSuperResolution,
            ReleaseChannel::Preview,
        );
        assert_eq!(
            developer_mode_requirement(&non_d3d12),
            DeveloperModeRequirement::NotRequired
        );
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ambiguous_catalog_provenance_fails_closed_without_reading_developer_mode() {
        let reads = Arc::new(AtomicUsize::new(0));
        let probe_reads = Arc::clone(&reads);
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"))
            .with_developer_mode_status_provider(move || {
                probe_reads.fetch_add(1, Ordering::SeqCst);
                DeveloperModeStatus::Enabled
            });
        let without_receipt = artifact(GraphicsTechnology::D3D12Agility, ReleaseChannel::Preview)
            .with_metadata(ArtifactMetadata::default());
        let imported_without_receipt = LibraryArtifact::new(
            without_receipt.id().clone(),
            GraphicsTechnology::D3D12Agility,
            "D3D12Core.dll",
            without_receipt.files().to_vec(),
            ArtifactTrustLevel::UserImported,
        )
        .expect("artifact");
        let mismatched_receipt = artifact(
            GraphicsTechnology::DlssSuperResolution,
            ReleaseChannel::Preview,
        );
        let mismatched_receipt = LibraryArtifact::new(
            mismatched_receipt.id().clone(),
            GraphicsTechnology::D3D12Agility,
            "D3D12Core.dll",
            mismatched_receipt.files().to_vec(),
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(mismatched_receipt.metadata().clone());

        for artifact in [
            without_receipt,
            imported_without_receipt,
            mismatched_receipt,
        ] {
            assert_eq!(
                developer_mode_requirement(&artifact),
                DeveloperModeRequirement::Unverifiable
            );
            let plan = apply_developer_mode_prerequisite(
                &context,
                build_swap_operation_plan(&component(), &artifact).expect("plan"),
                &artifact,
            );
            assert_eq!(
                plan.blockers(),
                &[OperationPlanBlocker::DeveloperModeCheckUnavailable]
            );
        }
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn existing_plan_blockers_skip_the_developer_mode_probe() {
        let reads = Arc::new(AtomicUsize::new(0));
        let probe_reads = Arc::clone(&reads);
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"))
            .with_developer_mode_status_provider(move || {
                probe_reads.fetch_add(1, Ordering::SeqCst);
                DeveloperModeStatus::Enabled
            });
        let artifact = artifact(GraphicsTechnology::D3D12Agility, ReleaseChannel::Preview);
        let incompatible_component = GraphicsComponent::new(
            ComponentId::new("component:dlss-test").expect("component id"),
            GameId::new("manual:d3d12-test").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(
                PathRef::new("C:/Games/Test/nvngx_dlss.dll").expect("component path"),
            )
            .with_sha256(Sha256Hash::new(COMPONENT_HASH).expect("component hash")),
        );
        let blocked = build_swap_operation_plan(&incompatible_component, &artifact)
            .expect("technology mismatch is represented as a blocker");

        let result = apply_developer_mode_prerequisite(&context, blocked, &artifact);

        assert_eq!(
            result.blockers(),
            &[OperationPlanBlocker::TechnologyMismatch]
        );
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }

    fn component() -> GraphicsComponent {
        GraphicsComponent::new(
            ComponentId::new("component:d3d12-test").expect("component id"),
            GameId::new("manual:d3d12-test").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(
                PathRef::new("C:/Games/Test/D3D12Core.dll").expect("component path"),
            )
            .with_sha256(Sha256Hash::new(COMPONENT_HASH).expect("component hash")),
        )
    }

    fn artifact(technology: GraphicsTechnology, channel: ReleaseChannel) -> LibraryArtifact {
        let revision = Sha256Hash::new(ARTIFACT_HASH).expect("revision hash");
        let receipt = CatalogPackageReceiptV1 {
            schema_version: CatalogReceiptSchemaV1,
            package_id: "d3d12-test".to_owned(),
            vendor: "microsoft".to_owned(),
            technology: technology.as_slug().to_owned(),
            variant: "runtime".to_owned(),
            display_name: "D3D12 Agility".to_owned(),
            release: PackageRelease {
                version: PackageVersion::parse("1.619.1").expect("package version"),
                channel,
                label: None,
            },
            target: CatalogTargetReceipt {
                os: "windows".to_owned(),
                architecture: Architecture::X64,
                compatibility: None,
            },
            provenance: None,
            revision_sha256: revision.clone(),
            primary_file_name: "D3D12Core.dll".to_owned(),
            primary_sha256: revision.clone(),
            primary_signature: CatalogSignatureReceipt::Unsigned,
            legal_documents: Vec::new(),
            size_bytes: 1,
        };
        LibraryArtifact::new(
            ArtifactId::for_package_revision(&revision),
            technology,
            "D3D12Core.dll",
            vec![
                ComponentFile::new(
                    PathRef::new("C:/Library/D3D12Core.dll").expect("artifact path"),
                )
                .with_sha256(revision.clone()),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(ArtifactMetadata::default().with_catalog_package_receipt(receipt))
    }
}
