use windows_sys::Win32::Storage::FileSystem::{
    FILE_ADD_SUBDIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
    SYNCHRONIZE,
};

use crate::portable_runtime::{error::Result, win32::object::handle::open_root};

use super::handle::VerifiedDirectory;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObjectIdentity(pub(super) String);

impl ObjectIdentity {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque retained root object. Its underlying directory handle never leaves
/// this object boundary.
#[derive(Debug)]
pub(crate) struct PortableRoot(pub(super) VerifiedDirectory);

pub(crate) fn open_portable_root(path: &std::path::Path) -> Result<PortableRoot> {
    Ok(PortableRoot(open_root(
        path,
        FILE_TRAVERSE | FILE_ADD_SUBDIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )?))
}

impl PortableRoot {
    pub(crate) fn identity(&self) -> &ObjectIdentity {
        self.0.identity()
    }
}
