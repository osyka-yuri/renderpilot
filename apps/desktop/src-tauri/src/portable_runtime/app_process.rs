#![expect(
    unsafe_code,
    reason = "exact HANDLE_LIST, JOB_LIST, suspended creation, and retained native handles are confined to this module"
)]

use std::{
    fs::File,
    io::BufReader,
    mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr,
};

use windows_sys::Win32::{
    Foundation::{GetLastError, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation},
    Security::SECURITY_ATTRIBUTES,
    System::{
        Pipes::CreatePipe,
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            INFINITE, InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION, ResumeThread, STARTUPINFOEXW,
            STARTUPINFOW, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

use super::{
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableAppSessionV2, read_message,
        read_message_or_eof, reader, write_message,
    },
    error::{PortableRuntimeError, Result},
    win32::{
        handle::OwnedHandle,
        job::KillOnCloseJob,
        process::{path_wide_nul, wide_nul},
    },
};

const APP_ARGUMENT: &str = "--renderpilot-portable-app";
const CONTROL_ARGUMENT: &str = "--renderpilot-control-handle";
const STATUS_ARGUMENT: &str = "--renderpilot-status-handle";

/// Exact child resources retained by the supervisor.  PID and wait results are
/// diagnostics; only the handles, job attribute, image path, and transcripts
/// authorize protocol progress.
pub struct TrialProcess {
    pub process: OwnedHandle,
    // Retained as an exact process-creation authority object until the child
    // leaves the trial lifecycle.
    _thread: OwnedHandle,
    control: File,
    status: BufReader<File>,
}

impl TrialProcess {
    pub fn spawn(app: &Path, job: &KillOnCloseJob, startup: &PortableAppSessionV2) -> Result<Self> {
        let (control_read, control_write) = private_pipe()?;
        let (status_read, status_write) = private_pipe()?;
        let child_handles = [
            control_read.as_raw_handle().cast(),
            status_write.as_raw_handle().cast(),
        ];
        let job_handles = [job.raw()];
        for handle in child_handles {
            // SAFETY: each handle is owned by one of the local `File`s and is
            // present in the exact child HANDLE_LIST below.
            if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
                == 0
            {
                return Err(PortableRuntimeError::new(
                    "portable_app_handles",
                    "could not mark exact child pipe handle inheritable",
                ));
            }
        }
        let attributes = AttributeList::new(&child_handles, &job_handles)?;
        let mut command = command_line(app, child_handles[0], child_handles[1]);
        let application = path_wide_nul(app);
        let directory = app.parent().map(path_wide_nul).ok_or_else(|| {
            PortableRuntimeError::new("portable_app_image", "App image had no parent directory")
        })?;
        let startup_info = STARTUPINFOEXW {
            StartupInfo: STARTUPINFOW {
                cb: mem::size_of::<STARTUPINFOEXW>() as u32,
                ..Default::default()
            },
            lpAttributeList: attributes.list,
        };
        let mut process_info = PROCESS_INFORMATION::default();
        // SAFETY: the application, command, working-directory, startup-info,
        // and attribute list remain live through creation. Handle inheritance
        // is constrained to the exact two private pipe ends by HANDLE_LIST;
        // JOB_LIST assigns KILL_ON_JOB_CLOSE before the first child instruction.
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                true.into(),
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                ptr::null(),
                directory.as_ptr(),
                &raw const startup_info.StartupInfo,
                &raw mut process_info,
            )
        };
        let create_error = unsafe { GetLastError() };
        drop(attributes);
        if created == 0 {
            return Err(PortableRuntimeError::new(
                "portable_app_create",
                format!("CreateProcessW failed: {create_error}"),
            ));
        }
        let process = OwnedHandle::new(process_info.hProcess).ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_app_create",
                "CreateProcessW returned no process handle",
            )
        })?;
        let thread = OwnedHandle::new(process_info.hThread).ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_app_create",
                "CreateProcessW returned no thread handle",
            )
        })?;
        // SAFETY: `thread` is the exact retained initial thread created suspended.
        if unsafe { ResumeThread(thread.raw()) } == u32::MAX {
            return Err(PortableRuntimeError::new(
                "portable_app_resume",
                "could not resume exact App initial thread",
            ));
        }
        let mut child = Self {
            process,
            _thread: thread,
            control: control_write,
            status: reader(status_read),
        };
        write_message(&mut child.control, &startup_control_message(startup))?;
        match read_message::<AppStatusMessage>(&mut child.status)? {
            AppStatusMessage::TrialHello(hello) if hello.challenge == startup.challenge => {}
            _ => {
                return Err(PortableRuntimeError::new(
                    "portable_app_protocol",
                    "App did not return the startup challenge",
                ));
            }
        }
        Ok(child)
    }

    pub fn send(&mut self, message: &AppControlMessage) -> Result<()> {
        write_message(&mut self.control, message)
    }
    pub fn receive(&mut self) -> Result<AppStatusMessage> {
        read_message(&mut self.status)
    }

    pub fn receive_or_eof(&mut self) -> Result<Option<AppStatusMessage>> {
        read_message_or_eof(&mut self.status)
    }

    pub fn wait_trial_ready(&mut self, startup: &PortableAppSessionV2) -> Result<u32> {
        match self.receive()? {
            AppStatusMessage::TrialReady(ready)
                if ready.db_query_only
                    && ready.webview_profile_ready
                    && ready.ui_bundle_ready
                    && ready.visible_window_ready
                    && ready.event_loop_roundtrip
                    && ready.transcript_sha256 == startup.transcript_sha256()?
                    && ready.runtime_paths_sha256 == startup.runtime_paths_sha256()?
                    && ready.supervisor_session_transcript_sha256
                        == startup.supervisor_session_transcript_sha256
                    && schema_observation_supported(startup, ready.schema_observed) =>
            {
                Ok(ready.schema_observed)
            }
            _ => Err(PortableRuntimeError::new(
                "portable_app_protocol",
                "App did not prove real TrialReadOnly readiness",
            )),
        }
    }

    /// The supervisor deliberately keeps the job, admission lock, and direct
    /// process/thread owners alive until this exact App exits. PID observations
    /// are not used to authorize any transition.
    pub fn wait_for_exit(&self) -> Result<()> {
        // SAFETY: `self.process` is the retained handle returned by the one
        // suspended CreateProcessW call for this transaction.
        if unsafe { WaitForSingleObject(self.process.raw(), INFINITE) } == u32::MAX {
            return Err(PortableRuntimeError::new(
                "portable_app_wait",
                format!("WaitForSingleObject failed: {}", unsafe { GetLastError() }),
            ));
        }
        Ok(())
    }

    pub fn wait_for_successful_exit(&self) -> Result<()> {
        self.wait_for_exit()?;
        validate_successful_exit_code(process_exit_code_after_wait(&self.process)?)
    }
}

fn process_exit_code_after_wait(process: &OwnedHandle) -> Result<u32> {
    let mut exit_code = 0;
    // SAFETY: `process` is a live retained process handle. The caller waits
    // for it before interpreting the returned code as terminal.
    if unsafe { GetExitCodeProcess(process.raw(), &raw mut exit_code) } == 0 {
        return Err(PortableRuntimeError::new(
            "portable_app_wait",
            format!("GetExitCodeProcess failed: {}", unsafe { GetLastError() }),
        ));
    }
    Ok(exit_code)
}

fn validate_successful_exit_code(exit_code: u32) -> Result<()> {
    if exit_code == 0 {
        return Ok(());
    }
    Err(PortableRuntimeError::new(
        "portable_app_exit",
        format!("App exited with code {exit_code}"),
    ))
}

pub(super) fn startup_control_message(startup: &PortableAppSessionV2) -> AppControlMessage {
    AppControlMessage::startup(startup.clone())
}

pub(crate) fn schema_observation_supported(
    startup: &PortableAppSessionV2,
    schema_observed: u32,
) -> bool {
    schema_observed == 0
        || (schema_observed >= startup.minimum_schema && schema_observed <= startup.maximum_schema)
}

fn private_pipe() -> Result<(File, File)> {
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: false.into(),
    };
    // SAFETY: output pointers and security attributes are valid. The pipe ends
    // become owned `File`s exactly once below.
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
        return Err(PortableRuntimeError::new(
            "portable_pipe_create",
            "CreatePipe failed",
        ));
    }
    // SAFETY: both handles are newly created and uniquely transferred into File.
    Ok(unsafe {
        (
            File::from_raw_handle(read.cast()),
            File::from_raw_handle(write.cast()),
        )
    })
}

struct AttributeList<'a> {
    storage: Vec<u8>,
    list: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
    // Win32 retains the lpValue pointers. These borrows keep their backing
    // arrays alive until DeleteProcThreadAttributeList runs in Drop.
    inherited_handles: &'a [HANDLE],
    job_handles: &'a [HANDLE],
}

impl<'a> AttributeList<'a> {
    fn new(inherited_handles: &'a [HANDLE], job_handles: &'a [HANDLE]) -> Result<Self> {
        let mut bytes = 0;
        // SAFETY: the sizing call intentionally receives null storage.
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(PortableRuntimeError::new(
                "portable_app_attributes",
                "could not size process attribute list",
            ));
        }
        let mut storage = vec![0_u8; bytes];
        let list = storage.as_mut_ptr().cast();
        // SAFETY: storage has the exact size returned by the sizing call.
        if unsafe { InitializeProcThreadAttributeList(list, 2, 0, &mut bytes) } == 0 {
            return Err(PortableRuntimeError::new(
                "portable_app_attributes",
                "could not initialize process attribute list",
            ));
        }
        let result = Self {
            storage,
            list,
            inherited_handles,
            job_handles,
        };
        // SAFETY: the borrowed handle arrays remain live until the attribute
        // list is dropped after process creation.
        let handles_ok = unsafe {
            UpdateProcThreadAttribute(
                result.list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                result.inherited_handles.as_ptr().cast_mut().cast(),
                mem::size_of_val(result.inherited_handles),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } != 0;
        let job_ok = unsafe {
            UpdateProcThreadAttribute(
                result.list,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                result.job_handles.as_ptr().cast_mut().cast(),
                mem::size_of_val(result.job_handles),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } != 0;
        if !handles_ok || !job_ok {
            return Err(PortableRuntimeError::new(
                "portable_app_attributes",
                "could not install HANDLE_LIST and JOB_LIST",
            ));
        }
        Ok(result)
    }
}

impl Drop for AttributeList<'_> {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.list);
        }
        let _ = &self.storage;
    }
}

fn command_line(app: &Path, control: HANDLE, status: HANDLE) -> Vec<u16> {
    let path = app.as_os_str().encode_wide().collect::<Vec<_>>();
    let text = format!(
        "\"{}\" {APP_ARGUMENT} {CONTROL_ARGUMENT}={} {STATUS_ARGUMENT}={}",
        String::from_utf16_lossy(&path),
        control as usize,
        status as usize
    );
    wide_nul(std::ffi::OsStr::new(&text))
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;

    use super::*;

    #[test]
    fn job_list_storage_remains_valid_through_process_creation() {
        let (control_read, _control_write) = private_pipe().expect("control pipe");
        let (_status_read, status_write) = private_pipe().expect("status pipe");
        let child_handles = [
            control_read.as_raw_handle().cast(),
            status_write.as_raw_handle().cast(),
        ];
        for handle in child_handles {
            assert_ne!(
                unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
                0,
                "make child pipe handle inheritable"
            );
        }

        let job = KillOnCloseJob::create().expect("create test job");
        let job_handles = [job.raw()];
        let attributes =
            AttributeList::new(&child_handles, &job_handles).expect("process attributes");
        let app = std::env::current_exe().expect("current test executable");
        let application = path_wide_nul(&app);
        let directory = path_wide_nul(app.parent().expect("test executable directory"));
        let path = app.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut command = wide_nul(std::ffi::OsStr::new(&format!(
            "\"{}\" --exact __renderpilot_attribute_list_child__",
            String::from_utf16_lossy(&path)
        )));
        let startup_info = STARTUPINFOEXW {
            StartupInfo: STARTUPINFOW {
                cb: mem::size_of::<STARTUPINFOEXW>() as u32,
                ..Default::default()
            },
            lpAttributeList: attributes.list,
        };
        let mut process_info = PROCESS_INFORMATION::default();
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                true.into(),
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                ptr::null(),
                directory.as_ptr(),
                &raw const startup_info.StartupInfo,
                &raw mut process_info,
            )
        };
        let create_error = unsafe { GetLastError() };
        drop(attributes);
        assert_ne!(created, 0, "CreateProcessW failed: {create_error}");

        let process = OwnedHandle::new(process_info.hProcess).expect("child process handle");
        let thread = OwnedHandle::new(process_info.hThread).expect("child thread handle");
        assert_ne!(
            unsafe { ResumeThread(thread.raw()) },
            u32::MAX,
            "resume child process"
        );
        assert_eq!(
            unsafe { WaitForSingleObject(process.raw(), 30_000) },
            WAIT_OBJECT_0,
            "child process did not exit"
        );
        assert_eq!(
            process_exit_code_after_wait(&process).expect("child exit code"),
            0
        );
    }

    #[test]
    fn only_zero_is_a_successful_app_exit_code() {
        validate_successful_exit_code(0).expect("zero exit code");
        assert_eq!(
            validate_successful_exit_code(1)
                .expect_err("nonzero exit must remain fatal")
                .code(),
            "portable_app_exit"
        );
    }
}
