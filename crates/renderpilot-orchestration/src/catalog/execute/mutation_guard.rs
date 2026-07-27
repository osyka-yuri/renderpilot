//! D3D12 executable mutation guard and immutable sidecar operations.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

use renderpilot_application::{AppError, AppResult, D3d12ExecutableAction};
use renderpilot_domain::Sha256Hash;

use crate::catalog::runtime_compatibility::{D3D12_SDK_VERSION_EXPORT, D3d12ExecutableState};

/// Exclusive mutation-boundary ownership of the live EXE and its immutable backup.
///
/// The handle is acquired before the durable snapshot and retained through the
/// filesystem and SQLite commit. On Windows the share mode permits readers but
/// denies writers and replacement/deletion; on other platforms the standard
/// exclusive file lock provides the same mutation boundary.
#[derive(Debug)]
pub(super) struct D3d12ExecutableMutationGuard {
    live: File,
    backup: Option<File>,
    original_bytes: Vec<u8>,
}

impl D3d12ExecutableMutationGuard {
    /// Acquires exclusive mutation ownership and revalidates the fresh preflight state.
    pub(super) fn acquire(state: &D3d12ExecutableState) -> AppResult<Self> {
        if state.repair_required {
            return Err(AppError::invalid_input(
                "D3D12 executable requires repair before it can be changed",
            ));
        }

        let mut live = open_locked(&state.executable_path, true)?;
        let live_bytes = read_locked(&mut live, &state.executable_path)?;
        validate_live_identity(state, &live_bytes)?;

        let (backup, original_bytes) = if state.backup_exists {
            let mut backup = open_locked(&state.backup_path, false)?;
            let original_bytes = read_locked(&mut backup, &state.backup_path)?;
            validate_backup_identity(state, &original_bytes, &live_bytes)?;
            (Some(backup), original_bytes)
        } else {
            if state.backup_path.exists() || state.current_sha256 != state.original_sha256 {
                return Err(AppError::confirmation_token_mismatch());
            }
            (None, live_bytes)
        };

        Ok(Self {
            live,
            backup,
            original_bytes,
        })
    }

    /// Creates or validates the immutable EXE sidecar under the live-file lock.
    pub(super) fn ensure_backup(
        &mut self,
        state: &D3d12ExecutableState,
        action: &D3d12ExecutableAction,
    ) -> AppResult<()> {
        if !action.changes_executable() {
            return Ok(());
        }
        if self.backup.is_some() {
            return Ok(());
        }
        let mut backup =
            create_locked_backup_exclusively(&state.backup_path, &self.original_bytes)?;
        let original_bytes = read_locked(&mut backup, &state.backup_path)?;
        if original_bytes != self.original_bytes
            || renderpilot_detection::sha256_bytes(&original_bytes)? != state.original_sha256
        {
            return Err(AppError::confirmation_token_mismatch());
        }
        self.backup = Some(backup);
        Ok(())
    }

    /// Writes and verifies the exact target built from the locked original bytes.
    pub(super) fn apply_action(
        &mut self,
        state: &D3d12ExecutableState,
        action: &D3d12ExecutableAction,
    ) -> AppResult<Sha256Hash> {
        if !action.changes_executable() {
            return Ok(state.current_sha256.clone());
        }
        let target_bytes = target_bytes_from_original(
            &self.original_bytes,
            state.original_sdk_version,
            action.target_sdk_version(),
        )?;
        self.write_sdk_field_and_verify(
            state,
            &target_bytes,
            action.target_sdk_version(),
            "D3D12 executable verification failed",
        )
    }

    /// Restores and verifies the original EXE while retaining its sidecar.
    pub(super) fn restore_for_rollback(&mut self, state: &D3d12ExecutableState) -> AppResult<()> {
        if self.backup.is_none() {
            return Err(AppError::invalid_input(
                "D3D12 executable backup is unavailable for rollback",
            ));
        }
        if state.current_sha256 != state.original_sha256 {
            let export = renderpilot_detection::pe_exported_u32_from_bytes(
                &self.original_bytes,
                D3D12_SDK_VERSION_EXPORT,
            )
            .ok_or_else(|| {
                AppError::invalid_input(
                    "original executable has no unique inline D3D12SDKVersion export",
                )
            })?;
            let end = export
                .file_offset
                .checked_add(4)
                .filter(|end| *end <= self.original_bytes.len())
                .ok_or_else(|| {
                    AppError::invalid_input("D3D12SDKVersion offset is out of bounds")
                })?;
            let sdk_bytes = self.original_bytes[export.file_offset..end]
                .try_into()
                .map_err(|_| AppError::invalid_input("D3D12SDKVersion field is not four bytes"))?;
            self.write_sdk_field_at_and_verify(
                state,
                export.file_offset,
                &sdk_bytes,
                &state.original_sha256,
                state.original_sdk_version,
                "restored D3D12 executable verification failed",
            )?;
        } else {
            verify_bytes(
                state,
                &read_locked(&mut self.live, &state.executable_path)?,
                &state.original_sha256,
                state.original_sdk_version,
                "restored D3D12 executable verification failed",
            )?;
        }
        Ok(())
    }

    /// Releases only the backup handle so the sidecar can be removed while the
    /// live executable remains deny-write locked through the database commit.
    pub(super) fn release_backup_lock(&mut self) {
        drop(self.backup.take());
    }

    fn write_sdk_field_and_verify(
        &mut self,
        state: &D3d12ExecutableState,
        target_bytes: &[u8],
        target_sdk_version: u32,
        failure: &str,
    ) -> AppResult<Sha256Hash> {
        let export = renderpilot_detection::pe_exported_u32_from_bytes(
            target_bytes,
            D3D12_SDK_VERSION_EXPORT,
        )
        .ok_or_else(|| {
            AppError::invalid_input(
                "original executable has no unique inline D3D12SDKVersion export",
            )
        })?;
        let end = export
            .file_offset
            .checked_add(4)
            .filter(|end| *end <= target_bytes.len())
            .ok_or_else(|| AppError::invalid_input("D3D12SDKVersion offset is out of bounds"))?;
        let sdk_bytes = target_bytes[export.file_offset..end]
            .try_into()
            .map_err(|_| AppError::invalid_input("D3D12SDKVersion field is not four bytes"))?;
        let expected_sha256 = renderpilot_detection::sha256_bytes(target_bytes)?;
        self.write_sdk_field_at_and_verify(
            state,
            export.file_offset,
            &sdk_bytes,
            &expected_sha256,
            target_sdk_version,
            failure,
        )
    }

    fn write_sdk_field_at_and_verify(
        &mut self,
        state: &D3d12ExecutableState,
        file_offset: usize,
        sdk_bytes: &[u8; 4],
        expected_sha256: &Sha256Hash,
        target_sdk_version: u32,
        failure: &str,
    ) -> AppResult<Sha256Hash> {
        self.live
            .seek(SeekFrom::Start(file_offset as u64))
            .and_then(|_| self.live.write_all(sdk_bytes))
            .and_then(|_| self.live.sync_all())
            .map_err(|error| {
                AppError::provider_failed(format!(
                    "failed to update D3D12 executable {}: {error}",
                    state.executable_path.display()
                ))
            })?;

        let actual = read_locked(&mut self.live, &state.executable_path)?;
        verify_bytes(state, &actual, expected_sha256, target_sdk_version, failure)?;
        Ok(expected_sha256.clone())
    }
}

/// Releases the EXE sidecar after every DLL/EXE restore has verified.
pub(super) fn release_rollback_backup(state: &D3d12ExecutableState) -> AppResult<()> {
    std::fs::remove_file(&state.backup_path).map_err(|error| {
        AppError::provider_failed(format!(
            "failed to remove D3D12 executable backup {}: {error}",
            state.backup_path.display()
        ))
    })?;
    crate::fs::sync_parent_directory_best_effort(&state.executable_path);
    Ok(())
}

fn open_locked(path: &std::path::Path, write: bool) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(write);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        options.share_mode(FILE_SHARE_READ);
    }
    let file = options.open(path).map_err(|error| {
        AppError::provider_failed(format!(
            "cannot lock D3D12 executable file {}: {error}",
            path.display()
        ))
    })?;
    #[cfg(not(windows))]
    file.lock().map_err(|error| {
        AppError::provider_failed(format!(
            "cannot lock D3D12 executable file {}: {error}",
            path.display()
        ))
    })?;
    Ok(file)
}

fn create_locked_backup_exclusively(path: &std::path::Path, bytes: &[u8]) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        options.share_mode(FILE_SHARE_READ);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(AppError::confirmation_token_mismatch());
        }
        Err(error) => {
            return Err(AppError::provider_failed(format!(
                "cannot create immutable D3D12 executable backup {}: {error}",
                path.display()
            )));
        }
    };
    #[cfg(not(windows))]
    if let Err(error) = file.lock() {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(AppError::provider_failed(format!(
            "cannot lock new D3D12 executable backup {}: {error}",
            path.display()
        )));
    }
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(AppError::provider_failed(format!(
            "cannot persist immutable D3D12 executable backup {}: {error}",
            path.display()
        )));
    }
    crate::fs::sync_parent_directory_best_effort(path);
    Ok(file)
}

fn read_locked(file: &mut File, path: &std::path::Path) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.read_to_end(&mut bytes))
        .map_err(|error| {
            AppError::provider_failed(format!(
                "cannot read locked D3D12 executable file {}: {error}",
                path.display()
            ))
        })?;
    Ok(bytes)
}

fn validate_live_identity(state: &D3d12ExecutableState, live: &[u8]) -> AppResult<()> {
    let sha256 = renderpilot_detection::sha256_bytes(live)?;
    let sdk = renderpilot_detection::pe_exported_u32_from_bytes(live, D3D12_SDK_VERSION_EXPORT)
        .map(|export| export.value);
    if sha256 != state.current_sha256 || sdk != Some(state.current_sdk_version) {
        return Err(AppError::confirmation_token_mismatch());
    }
    Ok(())
}

fn validate_backup_identity(
    state: &D3d12ExecutableState,
    original: &[u8],
    live: &[u8],
) -> AppResult<()> {
    let sha256 = renderpilot_detection::sha256_bytes(original)?;
    let sdk = renderpilot_detection::pe_exported_u32_from_bytes(original, D3D12_SDK_VERSION_EXPORT)
        .map(|export| export.value);
    if sha256 != state.original_sha256
        || sdk != Some(state.original_sdk_version)
        || !crate::catalog::runtime_compatibility::differs_only_at_sdk_export(original, live)
    {
        return Err(AppError::confirmation_token_mismatch());
    }
    Ok(())
}

fn target_bytes_from_original(
    original: &[u8],
    original_sdk_version: u32,
    target_sdk_version: u32,
) -> AppResult<Vec<u8>> {
    let mut bytes = original.to_vec();
    renderpilot_detection::replace_pe_exported_u32_in_bytes(
        &mut bytes,
        D3D12_SDK_VERSION_EXPORT,
        original_sdk_version,
        target_sdk_version,
    )
    .ok_or_else(|| {
        AppError::invalid_input("original executable has no unique inline D3D12SDKVersion export")
    })?;
    Ok(bytes)
}

fn verify_bytes(
    state: &D3d12ExecutableState,
    bytes: &[u8],
    expected_sha256: &Sha256Hash,
    expected_sdk: u32,
    failure: &str,
) -> AppResult<()> {
    let actual_sha256 = renderpilot_detection::sha256_bytes(bytes)?;
    let actual_sdk =
        renderpilot_detection::pe_exported_u32_from_bytes(bytes, D3D12_SDK_VERSION_EXPORT)
            .map(|export| export.value);
    if &actual_sha256 != expected_sha256 || actual_sdk != Some(expected_sdk) {
        return Err(AppError::provider_failed(format!(
            "{failure} for {}",
            state.executable_path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{D3d12ExecutableAction, D3d12ExecutableProfile};
    use renderpilot_domain::PathRef;

    use super::*;
    use crate::catalog::runtime_compatibility::{
        assess_d3d12_executable, synthetic_d3d12_executable,
    };

    fn action_for(state: &D3d12ExecutableState, target: u32) -> D3d12ExecutableAction {
        let executable_path =
            PathRef::new(state.executable_path.to_string_lossy().into_owned()).expect("exe path");
        let backup_path =
            PathRef::new(state.backup_path.to_string_lossy().into_owned()).expect("backup path");
        D3d12ExecutableAction::for_swap(
            &D3d12ExecutableProfile::new(
                executable_path,
                backup_path,
                state.original_sdk_version,
                state.current_sdk_version,
                state.backup_exists,
                state.repair_required,
            ),
            target,
        )
        .expect("action")
    }

    #[test]
    fn patch_to_patch_reuses_original_and_full_rollback_releases_backup() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable_path = dir.path().join("game.exe");
        let original = synthetic_d3d12_executable(606);
        std::fs::write(&executable_path, &original).expect("original executable");

        let initial = assess_d3d12_executable(&executable_path, None).expect("initial");
        let patch_619 = action_for(&initial, 619);
        {
            let mut guard = D3d12ExecutableMutationGuard::acquire(&initial).expect("initial lock");
            guard
                .ensure_backup(&initial, &patch_619)
                .expect("capture backup");
            guard.apply_action(&initial, &patch_619).expect("patch 619");
        }
        assert_eq!(
            std::fs::read(&initial.backup_path).expect("backup"),
            original
        );

        let active_619 = assess_d3d12_executable(&executable_path, None).expect("619");
        let patch_620 = action_for(&active_619, 620);
        {
            let mut guard = D3d12ExecutableMutationGuard::acquire(&active_619).expect("619 lock");
            guard
                .ensure_backup(&active_619, &patch_620)
                .expect("reuse backup");
            guard
                .apply_action(&active_619, &patch_620)
                .expect("patch 620");
        }
        assert_eq!(
            std::fs::read(&initial.backup_path).expect("backup"),
            original
        );
        assert_eq!(
            renderpilot_detection::read_pe_exported_u32(&executable_path, D3D12_SDK_VERSION_EXPORT),
            Some(620)
        );

        let active_620 = assess_d3d12_executable(&executable_path, None).expect("620");
        let restore = action_for(&active_620, 606);
        {
            let mut guard = D3d12ExecutableMutationGuard::acquire(&active_620).expect("620 lock");
            guard
                .apply_action(&active_620, &restore)
                .expect("compatible restore");
        }
        assert_eq!(std::fs::read(&executable_path).expect("restored"), original);
        assert!(
            initial.backup_path.exists(),
            "compatible restore must retain the immutable sidecar"
        );

        let restored = assess_d3d12_executable(&executable_path, None).expect("restored state");
        {
            let mut guard =
                D3d12ExecutableMutationGuard::acquire(&restored).expect("rollback lock");
            guard
                .restore_for_rollback(&restored)
                .expect("verified rollback restore");
        }
        release_rollback_backup(&restored).expect("release backup");
        assert!(!initial.backup_path.exists());
    }

    #[test]
    fn repair_state_never_overwrites_external_executable_changes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable_path = dir.path().join("game.exe");
        let backup_path = dir.path().join("game.exe.bak");
        let original = synthetic_d3d12_executable(606);
        let mut changed = original.clone();
        changed[2] ^= 1;
        std::fs::write(&executable_path, &changed).expect("changed executable");
        std::fs::write(&backup_path, &original).expect("original backup");

        let state = assess_d3d12_executable(&executable_path, None).expect("repair assessment");
        assert!(state.repair_required);
        assert!(D3d12ExecutableMutationGuard::acquire(&state).is_err());
        assert_eq!(
            std::fs::read(&executable_path).expect("unchanged live"),
            changed
        );
        assert_eq!(
            std::fs::read(&backup_path).expect("unchanged backup"),
            original
        );
    }

    #[test]
    fn stale_live_executable_is_rejected_before_any_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable_path = dir.path().join("game.exe");
        let mut original = synthetic_d3d12_executable(606);
        std::fs::write(&executable_path, &original).expect("original executable");
        let state = assess_d3d12_executable(&executable_path, None).expect("assessment");

        original[2] ^= 1;
        std::fs::write(&executable_path, &original).expect("external update");

        let error = D3d12ExecutableMutationGuard::acquire(&state)
            .expect_err("stale state must not acquire");
        assert_eq!(
            error.kind(),
            &renderpilot_application::AppErrorKind::ConfirmationTokenMismatch
        );
        assert_eq!(
            std::fs::read(&executable_path).expect("unchanged external update"),
            original
        );
        assert!(!state.backup_path.exists());
    }

    #[test]
    fn backup_created_after_lock_is_never_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable_path = dir.path().join("game.exe");
        let original = synthetic_d3d12_executable(606);
        std::fs::write(&executable_path, &original).expect("original executable");
        let state = assess_d3d12_executable(&executable_path, None).expect("assessment");
        let action = action_for(&state, 619);
        let mut guard =
            D3d12ExecutableMutationGuard::acquire(&state).expect("mutation boundary lock");
        let foreign_backup = b"foreign-backup-must-survive";
        std::fs::write(&state.backup_path, foreign_backup).expect("concurrent backup");

        let error = guard
            .ensure_backup(&state, &action)
            .expect_err("exclusive creation must reject the race");
        assert_eq!(
            error.kind(),
            &renderpilot_application::AppErrorKind::ConfirmationTokenMismatch
        );
        assert_eq!(
            std::fs::read(&state.backup_path).expect("foreign backup"),
            foreign_backup
        );
        assert_eq!(
            std::fs::read(&executable_path).expect("unchanged executable"),
            original
        );
    }
}
