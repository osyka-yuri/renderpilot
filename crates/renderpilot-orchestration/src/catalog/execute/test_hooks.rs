//! Failure injection isolated from the production execution flow.

use renderpilot_application::{AppError, AppResult};

thread_local! {
    static BEFORE_COPY_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
    static D3D12_APPLY_FAILURE_POINT: std::cell::Cell<Option<D3d12ApplyFailurePoint>> =
        const { std::cell::Cell::new(None) };
    static D3D12_ROLLBACK_FAILURE_POINT: std::cell::Cell<Option<D3d12RollbackFailurePoint>> =
        const { std::cell::Cell::new(None) };
}

pub(super) struct BeforeCopyHookGuard;

impl Drop for BeforeCopyHookGuard {
    fn drop(&mut self) {
        BEFORE_COPY_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

pub(super) fn set_before_copy_hook(hook: impl FnOnce() + 'static) -> BeforeCopyHookGuard {
    BEFORE_COPY_HOOK.with(|slot| {
        let previous = slot.replace(Some(Box::new(hook)));
        assert!(
            previous.is_none(),
            "before-copy test hook already installed"
        );
    });
    BeforeCopyHookGuard
}

pub(super) fn run_before_copy_hook() {
    BEFORE_COPY_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum D3d12ApplyFailurePoint {
    AfterExecutableBackup,
    AfterDllMutation,
    AfterExecutableMutation,
    BeforeDatabaseCommit,
}

pub(super) struct D3d12ApplyFailureGuard;

impl Drop for D3d12ApplyFailureGuard {
    fn drop(&mut self) {
        D3D12_APPLY_FAILURE_POINT.set(None);
    }
}

pub(super) fn set_d3d12_apply_failure_point(
    point: D3d12ApplyFailurePoint,
) -> D3d12ApplyFailureGuard {
    D3D12_APPLY_FAILURE_POINT.with(|slot| {
        assert!(
            slot.replace(Some(point)).is_none(),
            "D3D12 apply failure point already installed"
        );
    });
    D3d12ApplyFailureGuard
}

pub(super) fn inject_d3d12_apply_failure(point: D3d12ApplyFailurePoint) -> AppResult<()> {
    D3D12_APPLY_FAILURE_POINT.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            Err(AppError::provider_failed(format!(
                "injected D3D12 apply failure at {point:?}"
            )))
        } else {
            Ok(())
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum D3d12RollbackFailurePoint {
    AfterDllRestore,
    AfterExecutableRestore,
    AfterDllSidecarRelease,
    AfterExecutableSidecarRelease,
    BeforeDatabaseCommit,
}

pub(super) struct D3d12RollbackFailureGuard;

impl Drop for D3d12RollbackFailureGuard {
    fn drop(&mut self) {
        D3D12_ROLLBACK_FAILURE_POINT.set(None);
    }
}

pub(super) fn set_d3d12_rollback_failure_point(
    point: D3d12RollbackFailurePoint,
) -> D3d12RollbackFailureGuard {
    D3D12_ROLLBACK_FAILURE_POINT.with(|slot| {
        assert!(
            slot.replace(Some(point)).is_none(),
            "D3D12 rollback failure point already installed"
        );
    });
    D3d12RollbackFailureGuard
}

pub(super) fn inject_d3d12_rollback_failure(point: D3d12RollbackFailurePoint) -> AppResult<()> {
    D3D12_ROLLBACK_FAILURE_POINT.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            Err(AppError::provider_failed(format!(
                "injected D3D12 rollback failure at {point:?}"
            )))
        } else {
            Ok(())
        }
    })
}
