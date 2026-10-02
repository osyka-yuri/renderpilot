//! Exact backup-sidecar claims from completed component replacement baselines.

use std::{collections::BTreeMap, path::Path};

use renderpilot_domain::{ComponentRollbackBaseline, PathRef, Sha256Hash, normalized_path_key};

use super::{LocalCleanupCategory, LocalCleanupClaim, insert_if_below_root};

pub(super) fn insert_claims(
    claims: &mut BTreeMap<String, LocalCleanupClaim>,
    root: &PathRef,
    baseline: &ComponentRollbackBaseline,
) {
    let expected_active = baseline.expected_active_files();
    if !expected_active.is_empty() {
        for original in baseline.files() {
            let Some(original_digest) = original.sha256() else {
                continue;
            };
            if !original_was_archived_or_replaced(original.path(), original_digest, expected_active)
            {
                continue;
            }

            let Ok(sidecar) = crate::fs::backup_path(Path::new(original.path().as_str())) else {
                continue;
            };
            let Ok(path) = PathRef::new(sidecar.to_string_lossy()) else {
                continue;
            };
            insert_if_below_root(
                claims,
                root,
                LocalCleanupClaim {
                    path,
                    category: LocalCleanupCategory::ComponentFile,
                    sha256: Some(original_digest.clone()),
                    native_identity: None,
                    removable: true,
                    blocked_reason: None,
                },
            );
        }
    }

    if let Some(executable) = baseline.d3d12_executable() {
        let original = executable.original().sha256();
        if original != executable.expected_active().sha256() {
            let Ok(sidecar) =
                crate::fs::backup_path(Path::new(executable.executable_path().as_str()))
            else {
                return;
            };
            let Ok(path) = PathRef::new(sidecar.to_string_lossy()) else {
                return;
            };
            insert_if_below_root(
                claims,
                root,
                LocalCleanupClaim {
                    path,
                    category: LocalCleanupCategory::ComponentFile,
                    sha256: Some(original.clone()),
                    native_identity: None,
                    removable: true,
                    blocked_reason: None,
                },
            );
        }
    }
}

fn original_was_archived_or_replaced(
    original_path: &PathRef,
    original_digest: &Sha256Hash,
    expected_active: &[renderpilot_domain::ComponentFile],
) -> bool {
    let mut matching = expected_active
        .iter()
        .filter(|active| same_path(active.path(), original_path));
    let Some(active) = matching.next() else {
        return true;
    };
    if matching.next().is_some() {
        return false;
    }

    active
        .sha256()
        .is_some_and(|active_digest| active_digest != original_digest)
}

fn same_path(left: &PathRef, right: &PathRef) -> bool {
    normalized_path_key(left.as_str()) == normalized_path_key(right.as_str())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{ComponentFile, D3d12ExecutableBaseline, D3d12ExecutableIdentity};

    use super::*;

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("path")
    }

    fn digest(byte: char) -> Sha256Hash {
        Sha256Hash::new(byte.to_string().repeat(Sha256Hash::HEX_LENGTH)).expect("digest")
    }

    fn baseline(
        expected: Vec<ComponentFile>,
        original: Option<Sha256Hash>,
    ) -> ComponentRollbackBaseline {
        let mut file = ComponentFile::new(path("C:/Games/Example/vendor.dll"));
        if let Some(original) = original {
            file = file.with_sha256(original);
        }
        ComponentRollbackBaseline::new(vec![file]).with_expected_active_files(expected)
    }

    fn claim_for(baseline: &ComponentRollbackBaseline) -> Option<LocalCleanupClaim> {
        let mut claims = BTreeMap::new();
        insert_claims(&mut claims, &path("C:/Games/Example"), baseline);
        claims
            .get(&normalized_path_key("C:/Games/Example/vendor.dll.bak"))
            .cloned()
    }

    #[test]
    fn normal_receipt_baseline_archive_proves_only_the_exact_original_sidecar_hash() {
        let original = digest('a');
        let archived = baseline(
            vec![
                ComponentFile::new(path("C:/Games/Example/replacement.dll"))
                    .with_sha256(digest('c')),
            ],
            Some(original.clone()),
        );

        let claim = claim_for(&archived).expect("sidecar claim");
        assert_eq!(claim.path(), &path("C:/Games/Example/vendor.dll.bak"));
        assert_eq!(claim.category(), LocalCleanupCategory::ComponentFile);
        assert_eq!(claim.sha256(), Some(&original));
        assert!(claim.removable());
        assert!(claim.native_identity().is_none());
    }

    #[test]
    fn normal_receipt_unchanged_unknown_empty_and_unhashed_originals_do_not_prove_sidecars() {
        let original = digest('a');
        let unchanged = baseline(
            vec![
                ComponentFile::new(path("C:/Games/Example/vendor.dll"))
                    .with_sha256(original.clone()),
            ],
            Some(original.clone()),
        );
        let unknown_active = baseline(
            vec![ComponentFile::new(path("C:/Games/Example/vendor.dll"))],
            Some(original.clone()),
        );
        let empty = baseline(Vec::new(), Some(original));
        let unknown_original = baseline(
            vec![
                ComponentFile::new(path("C:/Games/Example/replacement.dll"))
                    .with_sha256(digest('b')),
            ],
            None,
        );

        assert!(claim_for(&unchanged).is_none());
        assert!(claim_for(&unknown_active).is_none());
        assert!(claim_for(&empty).is_none());
        assert!(claim_for(&unknown_original).is_none());
    }

    #[test]
    fn normal_receipt_same_path_with_a_known_different_active_hash_proves_the_original_sidecar() {
        let baseline = baseline(
            vec![ComponentFile::new(path("C:/Games/Example/vendor.dll")).with_sha256(digest('b'))],
            Some(digest('a')),
        );

        assert!(claim_for(&baseline).is_some());
    }

    #[test]
    fn completed_d3d12_baseline_claims_only_the_exact_executable_backup() {
        let executable_path = path("C:/Games/Example/game.exe");
        let original = digest('a');
        let baseline = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                executable_path,
                D3d12ExecutableIdentity::new(606, original.clone()),
                D3d12ExecutableIdentity::new(611, digest('b')),
            ),
        );
        let mut claims = BTreeMap::new();

        insert_claims(&mut claims, &path("C:/Games/Example"), &baseline);

        let sidecar = claims
            .get(&normalized_path_key("C:/Games/Example/game.exe.bak"))
            .expect("exact executable sidecar claim");
        assert_eq!(sidecar.path(), &path("C:/Games/Example/game.exe.bak"));
        assert_eq!(sidecar.sha256(), Some(&original));
        assert!(sidecar.removable());
        assert!(
            !claims.contains_key(&normalized_path_key("C:/Games/Example/game.exe")),
            "the executable itself is never claimed through this sidecar rule"
        );
        assert_eq!(claims.len(), 1);
    }

    #[test]
    fn d3d12_equal_identities_and_outside_root_paths_do_not_claim_backups() {
        let original = digest('a');
        let unchanged = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                path("C:/Games/Example/game.exe"),
                D3d12ExecutableIdentity::new(606, original.clone()),
                D3d12ExecutableIdentity::new(606, original),
            ),
        );
        let outside_root = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                path("C:/Games/Other/game.exe"),
                D3d12ExecutableIdentity::new(606, digest('a')),
                D3d12ExecutableIdentity::new(611, digest('b')),
            ),
        );

        assert!(claim_for_d3d12(&unchanged).is_none());
        assert!(claim_for_d3d12(&outside_root).is_none());
    }

    fn claim_for_d3d12(baseline: &ComponentRollbackBaseline) -> Option<LocalCleanupClaim> {
        let mut claims = BTreeMap::new();
        insert_claims(&mut claims, &path("C:/Games/Example"), baseline);
        claims
            .get(&normalized_path_key("C:/Games/Example/game.exe.bak"))
            .cloned()
    }

    #[test]
    fn normal_receipt_duplicate_same_path_projection_is_ambiguous_and_fails_closed() {
        let baseline = baseline(
            vec![
                ComponentFile::new(path("C:/Games/Example/vendor.dll")).with_sha256(digest('b')),
                ComponentFile::new(path("C:/Games/Example/vendor.dll")).with_sha256(digest('c')),
            ],
            Some(digest('a')),
        );

        assert!(claim_for(&baseline).is_none());
    }
}
