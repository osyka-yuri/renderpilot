use super::*;

pub(super) fn plan_removed_paths(
    request: RemovedPathRequest<'_>,
) -> Result<RemovedPathPlan, ServiceError> {
    let RemovedPathRequest {
        game_id,
        old_state,
        old_managed_files,
        old_proxy_path,
        new_proxy_path,
        target_dir,
        removed_paths,
        other_claim_paths,
        expected_hashes,
    } = request;
    let proxy_relocated = !crate::paths::same_path(old_proxy_path, new_proxy_path);
    let mut quarantine = Vec::new();
    let mut preservations = Vec::new();
    let mut preplanned_nonmanaged_keys = HashSet::new();
    let mut exact_removed_paths = Vec::new();
    let mut preplanned_preserved = Vec::new();
    for old_path in removed_paths {
        if proxy_relocated && crate::paths::same_path(old_path, old_proxy_path) {
            continue;
        }
        // Runtime/module bindings are released by the generic managed-file
        // pass below. Release-file receipts, however, are part of this
        // exact removal plan: a manifest-proven release member must become
        // a durable Delete ordinal before the write pass can publish the
        // new release. Adoption provenance does not change that lifecycle.
        let is_release_receipt = old_state.is_some_and(|state| {
            state
                .release_files
                .iter()
                .any(|receipt| crate::paths::same_path(Path::new(receipt.path.as_str()), old_path))
        });
        if !is_release_receipt
            && old_managed_files.iter().any(|managed| {
                crate::paths::same_path(Path::new(managed.path().as_str()), old_path)
            })
        {
            continue;
        }
        let key = crate::paths::normalized_key(old_path);
        preplanned_nonmanaged_keys.insert(key.clone());
        let managed_root = old_state
            .map(|state| Path::new(state.target_dir.as_str()))
            .filter(|root| {
                crate::paths::is_within(old_path, root) && !crate::paths::same_path(old_path, root)
            });
        let live = match managed_root {
            Some(root) => super::super::super::maybe_exact_managed_receipt_from_live(
                root,
                old_path,
                FileOwnership::Reused,
            )?,
            None => {
                super::super::super::maybe_exact_receipt_from_live(old_path, FileOwnership::Reused)?
            }
        };
        let Some(live) = live else {
            preplanned_preserved.push(old_path.to_string_lossy().into_owned());
            // The durable program still needs to consume this endpoint's
            // exact Verify(Absent) before it publishes any replacement.
            // `build_operations` derives Verify rather than Delete because
            // its sealed preimage is Absent.
            exact_removed_paths.push(old_path.clone());
            quarantine.push(MutationTarget::absent_file(old_path));
            continue;
        };
        let actual = live.digest().clone();
        let relocated_config = old_state.is_some_and(|state| {
            crate::paths::same_path(
                old_path,
                &Path::new(state.target_dir.as_str()).join("OptiScaler.ini"),
            ) && !crate::paths::same_path(Path::new(state.target_dir.as_str()), target_dir)
        });
        let prior = old_state.and_then(|state| prior_release_receipt(Some(state), old_path));
        if relocated_config
            && !prior.is_some_and(|receipt| receipt.installed.ownership() == FileOwnership::Owned)
        {
            // Exact adoption records provenance, not a right to consume the
            // user's configuration. A Reused configuration handoff is
            // always a Verify-only participant, whether its live bytes still
            // match the adopted digest or were edited after adoption.
            preplanned_preserved.push(old_path.to_string_lossy().into_owned());
            continue;
        }
        let expected = expected_hashes.get(&key);
        if relocated_config && expected != Some(&actual) {
            let preservation = ConfigPreservationPlan::for_game(game_id, old_path, actual, true)?;
            quarantine.push(preservation.source_target());
            preservations.push(preservation);
            continue;
        } else if expected != Some(&actual) {
            preplanned_preserved.push(old_path.to_string_lossy().into_owned());
            continue;
        }
        quarantine.push(MutationTarget::quarantine(old_path, Some(actual)));
        exact_removed_paths.push(old_path.clone());
    }
    Ok(RemovedPathPlan {
        cleanup: CleanupPlan {
            other_claim_paths,
            preplanned_nonmanaged_keys,
            exact_removed_paths,
            preplanned_preserved,
        },
        preservations,
        quarantine,
    })
}
