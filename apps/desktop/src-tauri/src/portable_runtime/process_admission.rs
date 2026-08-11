use std::{fs::File, path::Path};

use super::{
    epoch_namespace::validate_authority_namespace, error::Result, win32::file::open_share_zero,
};

/// D18's one private share-zero handle. It has no public release method; drop
/// happens only when the supervisor process tears down.
#[derive(Debug)]
pub struct AdmissionLock {
    _file: File,
}

impl AdmissionLock {
    pub fn acquire(authority_root: &Path) -> Result<Self> {
        validate_authority_namespace(authority_root)?;
        let admission = Self {
            _file: open_share_zero(&authority_root.join("admission.lock"))?,
        };
        // The lock leaf is classified through its retained no-follow handle;
        // this second stable scan proves the surrounding namespace stayed
        // total while that share-zero authority was acquired.
        validate_authority_namespace(authority_root)?;
        Ok(admission)
    }
}
