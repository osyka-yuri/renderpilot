#![expect(
    unsafe_code,
    reason = "this module is the narrow owner of the portable fatal-error Win32 dialog"
)]

use windows_sys::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
};

use crate::portable_runtime::error::PortableRuntimeError;

const DIALOG_TITLE: &str = "RenderPilot portable error";

pub(crate) fn show_portable_supervisor_failure(error: &PortableRuntimeError) {
    let title = wide(DIALOG_TITLE);
    let message = wide(&supervisor_failure_message(error));
    // SAFETY: both UTF-16 buffers are NUL-terminated and remain live for the
    // synchronous call. A null owner is valid for this fatal portable error.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

fn supervisor_failure_message(error: &PortableRuntimeError) -> String {
    format!(
        "RenderPilot portable mode could not start.\n\nError code: {}\n\nDetails may be available under data\\logs\\portable.",
        error.code()
    )
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::supervisor_failure_message;
    use crate::portable_runtime::error::PortableRuntimeError;

    #[test]
    fn supervisor_failure_dialog_excludes_internal_error_detail() {
        let error =
            PortableRuntimeError::new("portable_object", r"private path C:\Users\someone\portable");

        let message = supervisor_failure_message(&error);

        assert!(message.contains("Error code: portable_object"));
        assert!(!message.contains("private path"));
        assert!(!message.contains("Users"));
    }
}
