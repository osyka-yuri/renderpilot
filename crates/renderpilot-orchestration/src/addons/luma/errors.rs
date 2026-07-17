//! Luma [`ServiceError`] constructors: the shared addon vocabulary
//! ([`crate::addons::errors`]) plus Luma's own tool-specific messages.

use crate::ServiceError;

pub(super) use crate::addons::errors::{failed, invalid, io};

/// No Luma install is on record for the game. Shared by every flow that
/// requires an existing install before it can act.
pub(super) fn not_installed() -> ServiceError {
    invalid("Luma Framework is not installed for this game".to_owned())
}

/// Install snapshot/revalidation drift while network prepare ran unlocked.
pub(super) fn state_changed_retry_install() -> ServiceError {
    invalid("Luma install state changed during preparation; retry the install".to_owned())
}

/// Update snapshot/revalidation drift while network prepare ran unlocked.
pub(super) fn state_changed_retry_update() -> ServiceError {
    invalid("Luma install state changed during update preparation; retry the update".to_owned())
}
