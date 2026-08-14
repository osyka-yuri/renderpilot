use super::{
    error::Result,
    root_authority::SupervisorRootBinding,
    win32::object::{AdmissionObjects, acquire_supervisor_admission},
};

/// D18's one retained share-zero lock.  It carries the supervisor-only binding
/// and both parent handles through drop, so an App root capability alone never
/// acquires supervisor admission.
#[derive(Debug)]
pub struct AdmissionLock {
    _root: super::root_authority::PortableRootAuthority,
    _objects: AdmissionObjects,
}

impl AdmissionLock {
    pub fn acquire(binding: &SupervisorRootBinding) -> Result<Self> {
        Ok(Self {
            _root: binding.root().clone(),
            _objects: acquire_supervisor_admission(binding)?,
        })
    }
}
