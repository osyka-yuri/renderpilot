#![expect(
    unsafe_code,
    reason = "the supervisor creates and configures one KILL_ON_JOB_CLOSE Win32 job through this bounded owner"
)]

use windows_sys::Win32::{
    Foundation::HANDLE,
    System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    },
};

use super::handle::OwnedHandle;
use crate::portable_runtime::error::{PortableRuntimeError, Result};

/// One supervisor-owned `KILL_ON_JOB_CLOSE` job. There is no counter-based
/// authority release and no child receives this handle.
#[derive(Debug)]
pub struct KillOnCloseJob(OwnedHandle);

impl KillOnCloseJob {
    pub fn create() -> Result<Self> {
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        let job = OwnedHandle::new(raw).ok_or_else(|| {
            PortableRuntimeError::new("portable_job_create", "CreateJobObjectW failed")
        })?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(PortableRuntimeError::new(
                "portable_job_create",
                "could not set KILL_ON_JOB_CLOSE",
            ));
        }
        Ok(Self(job))
    }

    pub const fn raw(&self) -> HANDLE {
        self.0.raw()
    }
}
