//! Read-only distinction between a missing game payload and local add-on residue.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use renderpilot_detection::InstallTreeCompleteness;
use renderpilot_domain::{
    FileOwnership, ManagedFileBaseline, ManagedFileMode, NormalizedPathRelation,
    OptiScalerFileBaseline, PathRef, ProxyRootPrestate, Sha256Hash, normalized_path_key,
    normalized_path_relation,
};
use renderpilot_platform_windows::inspect_executable_candidates_complete_strict;
use renderpilot_storage_sqlite::LocalCleanupOwners;

use crate::fs::{EntryKind, VerifiedDir};

use super::residue::{LocalCleanupClaim, local_cleanup_claims};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GamePayloadObservation {
    Present,
    ConfirmedAbsent,
    Indeterminate { reason: String },
}

/// Checks only existing executable evidence first, then uses the platform's
/// complete non-hashing report to distinguish game payload from local residue.
/// Presence-only owner paths affect classification only; they never become
/// cleanup claims.
pub(crate) fn observe_game_payload(
    owners: &LocalCleanupOwners,
    expected_root_identity: &str,
) -> GamePayloadObservation {
    let root_ref = owners.game().install_path();
    let root = Path::new(root_ref.as_str());
    if expected_root_identity.trim().is_empty() {
        return indeterminate("native installation root identity is unavailable");
    }

    let root_authority =
        match VerifiedDir::open_absolute_components(root, Some(expected_root_identity)) {
            Ok(authority) => authority,
            Err(error) => {
                return indeterminate(format!(
                    "cannot reopen the observed installation root without following links: {error}"
                ));
            }
        };
    if root_authority.identity() != expected_root_identity {
        return indeterminate("installation root identity changed before payload inspection");
    }

    let candidate_paths = owners
        .game()
        .executable_candidates()
        .iter()
        .filter_map(|path| path_under_root(root, root_ref, path))
        .collect::<Vec<_>>();
    match existing_candidate(root, &candidate_paths) {
        Ok(true) => {
            drop(root_authority);
            return finish_after_probe(
                root,
                expected_root_identity,
                GamePayloadObservation::Present,
            );
        }
        Err(reason) => {
            drop(root_authority);
            return finish_after_probe(
                root,
                expected_root_identity,
                GamePayloadObservation::Indeterminate { reason },
            );
        }
        Ok(false) => {}
    }

    let report = inspect_executable_candidates_complete_strict(root);
    let claims = local_cleanup_claims(owners);
    let (claim_by_path, presence, applicable) = {
        let mut claim_by_path = BTreeMap::new();
        let mut presence = PresenceProjection::default();
        let mut has_owner_evidence = !candidate_paths.is_empty();

        for claim in &claims {
            let Some(path) = path_under_root(root, root_ref, claim.path()) else {
                continue;
            };
            has_owner_evidence = true;
            claim_by_path.insert(normalized_path_key(&path_string(&path)), claim.clone());
        }

        if let Some(addon) = owners.addon() {
            for path in std::iter::once(addon.addon_file())
                .chain(addon.created_files())
                .chain(addon.backed_up_files())
            {
                if let Some(path) = path_under_root(root, root_ref, path) {
                    has_owner_evidence = true;
                    presence
                        .path_only
                        .insert(normalized_path_key(&path_string(&path)));
                }
            }
        }

        add_typed_program_presence(owners, root, root_ref, &mut presence);
        has_owner_evidence |= !presence.typed_sha256.is_empty();

        for (_, baseline) in owners.baselines() {
            let paths = baseline
                .files()
                .iter()
                .chain(baseline.expected_active_files())
                .map(|file| file.path())
                .chain(
                    baseline
                        .d3d12_executable()
                        .map(|value| value.executable_path()),
                );
            has_owner_evidence |= paths
                .filter_map(|path| path_under_root(root, root_ref, path))
                .next()
                .is_some();
        }

        if let Some(engine_owner) = owners.engine_journal() {
            let journal = &engine_owner.journal;
            let mut paths = journal
                .stable
                .iter()
                .map(|receipt| receipt.path.as_str())
                .chain(journal.pending.iter().flat_map(|transition| {
                    transition
                        .prior
                        .iter()
                        .chain(transition.after.iter())
                        .map(|receipt| receipt.path.as_str())
                }));
            has_owner_evidence |= paths.any(|value| path_string_under_root(root, root_ref, value));
        }

        (claim_by_path, presence, has_owner_evidence)
    };

    let classification = if report.completeness() != InstallTreeCompleteness::Complete {
        let reason = report
            .diagnostics()
            .first()
            .cloned()
            .unwrap_or_else(|| "installation tree observation was incomplete".to_owned());
        indeterminate(reason)
    } else if !report.candidates().is_empty() {
        GamePayloadObservation::Present
    } else {
        classify_structural_files(
            root,
            root_ref,
            &root_authority,
            report.structural_files(),
            &claim_by_path,
            &presence,
            applicable,
        )
    };

    drop(root_authority);
    finish_after_probe(root, expected_root_identity, classification)
}

fn classify_structural_files(
    root: &Path,
    root_ref: &PathRef,
    root_authority: &VerifiedDir,
    structural_files: &[PathBuf],
    claim_by_path: &BTreeMap<String, LocalCleanupClaim>,
    presence: &PresenceProjection,
    applicable: bool,
) -> GamePayloadObservation {
    for reported_path in structural_files {
        let Some(relative) = relative_path_under_root(root_ref, &path_string(reported_path)) else {
            continue;
        };
        // Use the exact root spelling held by the verified directory, while
        // deriving only the relative components through normalized lookup.
        let path = root.join(relative);
        let key = normalized_path_key(&path_string(&path));
        if presence.path_only.contains(&key) {
            continue;
        }
        let expected_digest = match claim_by_path.get(&key) {
            Some(claim) if claim.removable() => claim.sha256().map(Sha256Hash::as_str),
            Some(_) | None => presence
                .typed_sha256
                .get(&key)
                .and_then(Option::as_ref)
                .map(String::as_str),
        };
        let Some(expected_digest) = expected_digest else {
            return GamePayloadObservation::Present;
        };
        let observed = match root_authority.observe_descendant(&path) {
            Ok(Some(observed)) => observed,
            Ok(None) => {
                return indeterminate("a structural file changed during payload inspection");
            }
            Err(error) => {
                return indeterminate(format!(
                    "cannot safely inspect recorded structural file {}: {error}",
                    path.display()
                ));
            }
        };
        if observed.kind != EntryKind::File {
            return indeterminate("a structural path changed kind during payload inspection");
        }
        if observed.digest.as_deref() != Some(expected_digest) {
            return GamePayloadObservation::Present;
        }
    }

    if applicable {
        GamePayloadObservation::ConfirmedAbsent
    } else {
        // An arbitrary active empty root carries no evidence that it ever held
        // a game payload, so an empty observation alone cannot hide it.
        GamePayloadObservation::Present
    }
}

fn existing_candidate(root: &Path, paths: &[PathBuf]) -> Result<bool, String> {
    for path in paths {
        let relative = path.strip_prefix(root).map_err(|_| {
            format!(
                "stored executable candidate escaped the installation root: {}",
                path.display()
            )
        })?;
        let components = relative.components().collect::<Vec<_>>();
        let mut current = root.to_owned();
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(name) = component else {
                return Err("stored executable candidate is not a normal relative path".to_owned());
            };
            current.push(name);
            let metadata = match fs::symlink_metadata(&current) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    return Err(format!(
                        "cannot inspect stored executable candidate {}: {error}",
                        current.display()
                    ));
                }
            };
            if is_reparse_point(&metadata) {
                return Err(format!(
                    "stored executable candidate crosses a link or reparse point: {}",
                    current.display()
                ));
            }
            let is_last = index + 1 == components.len();
            if is_last && metadata.is_file() {
                return Ok(true);
            }
            if (is_last && !metadata.is_file()) || (!is_last && !metadata.is_dir()) {
                return Err(format!(
                    "stored executable candidate has an unexpected entry kind: {}",
                    current.display()
                ));
            }
        }
    }
    Ok(false)
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn path_under_root(root: &Path, root_ref: &PathRef, path: &PathRef) -> Option<PathBuf> {
    relative_path_under_root(root_ref, path.as_str()).map(|relative| root.join(relative))
}

fn path_string_under_root(root: &Path, root_ref: &PathRef, value: &str) -> bool {
    let Ok(path) = PathRef::new(value) else {
        return false;
    };
    path_under_root(root, root_ref, &path).is_some()
}

fn normal_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn relative_path_under_root(root_ref: &PathRef, path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return normal_relative_path(candidate).then(|| candidate.to_owned());
    }
    if normalized_path_relation(root_ref.as_str(), path) != NormalizedPathRelation::LeftAncestor {
        return None;
    }

    let root_key = normalized_path_key(root_ref.as_str());
    let candidate_key = normalized_path_key(path);
    let prefix = if root_key.ends_with('/') {
        root_key
    } else {
        format!("{root_key}/")
    };
    let relative = Path::new(candidate_key.strip_prefix(&prefix)?);
    normal_relative_path(relative).then(|| relative.to_owned())
}

#[derive(Debug, Default)]
struct PresenceProjection {
    path_only: BTreeSet<String>,
    // `None` records conflicting typed installed hashes for a normalized path.
    typed_sha256: BTreeMap<String, Option<String>>,
}

fn add_typed_program_presence(
    owners: &LocalCleanupOwners,
    root: &Path,
    root_ref: &PathRef,
    presence: &mut PresenceProjection,
) {
    if let Some(addon) = owners.addon() {
        for file in addon.managed_files() {
            if managed_file_presence_digest(file).is_some() {
                insert_typed_presence(
                    &mut presence.typed_sha256,
                    root,
                    root_ref,
                    file.path(),
                    file.installed_sha256(),
                );
            }
        }
    }

    if let Some(state) = owners.optiscaler_state() {
        for file in &state.release_files {
            if file.installed.ownership() == FileOwnership::Owned {
                insert_typed_presence(
                    &mut presence.typed_sha256,
                    root,
                    root_ref,
                    &file.path,
                    file.installed.digest(),
                );
            }
        }
        for binding in &state.runtime_bindings {
            if optiscaler_binding_presence_digest(binding).is_some() {
                insert_typed_presence(
                    &mut presence.typed_sha256,
                    root,
                    root_ref,
                    &binding.path,
                    binding.installed.digest(),
                );
            }
        }
    }

    if let Some(topology) = owners.topology() {
        if topology.outer.receipt.ownership() == FileOwnership::Owned {
            insert_typed_presence(
                &mut presence.typed_sha256,
                root,
                root_ref,
                &topology.outer.path,
                topology.outer.receipt.digest(),
            );
        }
        // A relocated downstream is the original root preimage. Its bytes
        // remain game evidence even when the relocation receipt is `Owned`.
        if topology.root_prestate == ProxyRootPrestate::Absent
            && let Some(downstream) = &topology.downstream
            && downstream.receipt.ownership() == FileOwnership::Owned
        {
            insert_typed_presence(
                &mut presence.typed_sha256,
                root,
                root_ref,
                &downstream.path,
                downstream.receipt.digest(),
            );
        }
    }
}

fn managed_file_presence_digest(
    file: &renderpilot_domain::ManagedAddonFile,
) -> Option<&Sha256Hash> {
    if file.mode() != ManagedFileMode::Owned {
        return None;
    }
    match file.baseline() {
        ManagedFileBaseline::Absent => Some(file.installed_sha256()),
        ManagedFileBaseline::Present { sha256 } if sha256 != file.installed_sha256() => {
            Some(file.installed_sha256())
        }
        ManagedFileBaseline::Present { .. } => None,
    }
}

fn optiscaler_binding_presence_digest(
    binding: &renderpilot_domain::OptiScalerModuleRuntimeBinding,
) -> Option<&Sha256Hash> {
    if binding.installed.ownership() != FileOwnership::Owned {
        return None;
    }
    match &binding.baseline {
        OptiScalerFileBaseline::Absent => Some(binding.installed.digest()),
        OptiScalerFileBaseline::Present { receipt }
            if receipt.digest() != binding.installed.digest() =>
        {
            Some(binding.installed.digest())
        }
        OptiScalerFileBaseline::Present { .. } => None,
    }
}

fn insert_typed_presence(
    paths: &mut BTreeMap<String, Option<String>>,
    root: &Path,
    root_ref: &PathRef,
    path: &PathRef,
    digest: &Sha256Hash,
) {
    let Some(path) = path_under_root(root, root_ref, path) else {
        return;
    };
    let key = normalized_path_key(&path_string(&path));
    let value = paths
        .entry(key)
        .or_insert_with(|| Some(digest.as_str().to_owned()));
    if value
        .as_deref()
        .is_some_and(|existing| existing != digest.as_str())
    {
        *value = None;
    }
}

fn finish_after_probe(
    root: &Path,
    expected_identity: &str,
    result: GamePayloadObservation,
) -> GamePayloadObservation {
    match VerifiedDir::open_absolute_components(root, Some(expected_identity)) {
        Ok(authority) if authority.identity() == expected_identity => result,
        Ok(_) => indeterminate("installation root identity changed during payload inspection"),
        Err(error) => indeterminate(format!(
            "cannot reopen the same native installation root after payload inspection: {error}"
        )),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn indeterminate(reason: impl Into<String>) -> GamePayloadObservation {
    GamePayloadObservation::Indeterminate {
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "payload_presence/tests.rs"]
mod tests;
