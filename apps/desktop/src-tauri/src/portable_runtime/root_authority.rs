//! Retained portable-root authority.
//!
//! A clone shares one verified root handle through `Arc`; it never reopens the
//! textual root for descendants.  The managed App may borrow this capability
//! for diagnostics but cannot mint a supervisor binding from it.

use std::sync::Arc;

use super::{
    error::{PortableRuntimeError, Result},
    supervisor::authority::SupervisorSessionAuthority,
    win32::object::{ObjectIdentity, PortableRoot, open_portable_root},
};

#[derive(Debug)]
struct RetainedRoot {
    object: PortableRoot,
}

#[derive(Clone, Debug)]
pub(crate) struct PortableRootAuthority(Arc<RetainedRoot>);

impl PortableRootAuthority {
    /// Opens the raw root once for this process and retains the verified
    /// no-follow directory handle for all authority descendants.
    pub(super) fn open(path: &std::path::Path) -> Result<Self> {
        Ok(Self(Arc::new(RetainedRoot {
            object: open_portable_root(path)?,
        })))
    }

    pub(crate) fn identity(&self) -> &ObjectIdentity {
        self.0.object.identity()
    }

    /// Returns an opaque purpose object, never a generic directory handle.
    pub(in crate::portable_runtime) fn object(&self) -> &PortableRoot {
        &self.0.object
    }
}

/// The supervisor-only bridge between protocol authority and the retained root
/// object.  Admission takes this instead of a path or App root capability.
#[derive(Debug)]
pub(crate) struct SupervisorRootBinding {
    authority: SupervisorSessionAuthority,
    root: PortableRootAuthority,
}

impl SupervisorRootBinding {
    pub(in crate::portable_runtime) fn bind(
        authority: SupervisorSessionAuthority,
        root: PortableRootAuthority,
    ) -> Result<Self> {
        if authority.portable_root_identity() != root.identity().as_str() {
            return Err(PortableRuntimeError::new(
                "portable_supervisor_session",
                "supervisor protocol root identity differed from retained root authority",
            ));
        }
        Ok(Self { authority, root })
    }

    pub(in crate::portable_runtime) fn authority(&self) -> &SupervisorSessionAuthority {
        &self.authority
    }

    pub(in crate::portable_runtime) fn root(&self) -> &PortableRootAuthority {
        &self.root
    }
}
