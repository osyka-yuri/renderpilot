#![expect(
    unsafe_code,
    reason = "the exact HANDLE_LIST pipe ends become linear File owners only during authenticated child startup"
)]

use std::{
    ffi::OsString,
    fs::File,
    os::windows::io::{FromRawHandle, RawHandle},
};

use super::{
    activation::install_trial_session,
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableStartupV3, read_message, write_message,
    },
    error::{PortableRuntimeError, Result},
};

const APP_ARGUMENT: &str = "--renderpilot-portable-app";
const CONTROL_ARGUMENT: &str = "--renderpilot-control-handle=";
const STATUS_ARGUMENT: &str = "--renderpilot-status-handle=";

pub enum EarlyDispatch {
    DirectLaunchExit,
    App(Box<PortableStartupV3>),
}

/// Must be called before logger, filesystem, WebView2, Tauri, or GUI startup.
pub fn dispatch_before_desktop() -> Result<EarlyDispatch> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.len() == 4 && args.get(1).is_some_and(|arg| arg == APP_ARGUMENT) {
        return receive_app_startup(&args).map(|startup| EarlyDispatch::App(Box::new(startup)));
    }
    // A generation App is never a portable entry point. Only the separate raw
    // supervisor binary owns manual startup; copied RPU tails and supervisor
    // arguments cannot route this executable into supervisor admission.
    Ok(EarlyDispatch::DirectLaunchExit)
}

fn receive_app_startup(args: &[OsString]) -> Result<PortableStartupV3> {
    let control = parse_handle(args.get(2), CONTROL_ARGUMENT)?;
    let status = parse_handle(args.get(3), STATUS_ARGUMENT)?;
    if control == status {
        return Err(PortableRuntimeError::new(
            "portable_startup_invalid",
            "private App control and status handles were not distinct",
        ));
    }
    // SAFETY: these are the exact HANDLE_LIST pipe ends supplied only during
    // suspended child creation. A direct launch lacks both and returns Exit.
    let control = unsafe { File::from_raw_handle(control as RawHandle) };
    let mut status = unsafe { File::from_raw_handle(status as RawHandle) };
    let mut reader = std::io::BufReader::new(control.try_clone()?);
    let startup = match read_message::<AppControlMessage>(&mut reader)? {
        AppControlMessage::Startup(startup) => *startup,
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_startup_invalid",
                "first App message was not PortableStartupV3",
            ));
        }
    };
    startup.validate()?;
    write_message(
        &mut status,
        &AppStatusMessage::TrialHello {
            challenge: startup.challenge.clone(),
        },
    )?;
    install_trial_session(control, status, startup.clone())?;
    Ok(startup)
}

fn parse_handle(argument: Option<&OsString>, prefix: &str) -> Result<isize> {
    let value = argument
        .and_then(|argument| argument.to_str())
        .and_then(|text| text.strip_prefix(prefix))
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_startup_invalid",
                "required inherited pipe handle was absent",
            )
        })?;
    let handle = value.parse::<usize>().map_err(|_| {
        PortableRuntimeError::new(
            "portable_startup_invalid",
            "inherited pipe handle was invalid",
        )
    })?;
    if handle == 0 {
        return Err(PortableRuntimeError::new(
            "portable_startup_invalid",
            "inherited pipe handle was null",
        ));
    }
    Ok(handle as isize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn managed_app_requires_the_exact_private_argument_shape() {
        let valid = args(&[
            "renderpilot-desktop.exe",
            APP_ARGUMENT,
            "--renderpilot-control-handle=1",
            "--renderpilot-status-handle=2",
        ]);
        assert_eq!(
            parse_handle(valid.get(2), CONTROL_ARGUMENT).expect("control handle"),
            1
        );
        assert_eq!(
            parse_handle(valid.get(3), STATUS_ARGUMENT).expect("status handle"),
            2
        );

        for malformed in [
            args(&["renderpilot-desktop.exe"]),
            args(&[
                "renderpilot-desktop.exe",
                "--renderpilot-portable-supervisor",
            ]),
            args(&[
                "renderpilot-desktop.exe",
                APP_ARGUMENT,
                "--renderpilot-control-handle=1",
                "--renderpilot-status-handle=2",
                "unexpected",
            ]),
        ] {
            assert!(
                malformed.len() != 4
                    || malformed
                        .get(1)
                        .is_none_or(|argument| argument != APP_ARGUMENT)
            );
        }
    }

    #[test]
    fn private_handles_reject_missing_zero_and_wrong_slots() {
        assert!(parse_handle(None, CONTROL_ARGUMENT).is_err());
        assert!(
            parse_handle(
                Some(&OsString::from("--renderpilot-control-handle=0")),
                CONTROL_ARGUMENT,
            )
            .is_err()
        );
        assert!(
            parse_handle(
                Some(&OsString::from("--renderpilot-status-handle=2")),
                CONTROL_ARGUMENT,
            )
            .is_err()
        );
    }

    #[test]
    fn managed_app_bootstrap_cannot_dispatch_the_supervisor() {
        let source = include_str!("bootstrap.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production bootstrap source");
        assert!(!production.contains("dispatch_raw_or_supervisor"));
        assert!(!production.contains("supervisor::"));
    }

    #[test]
    fn managed_intent_propagates_bootstrap_failures_while_direct_launch_is_successful() {
        let source = include_str!("bootstrap.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production bootstrap source");
        assert!(production.contains("pub fn dispatch_before_desktop() -> Result<EarlyDispatch>"));
        assert!(production.contains("return receive_app_startup(&args).map"));
        assert!(production.contains("Ok(EarlyDispatch::DirectLaunchExit)"));
        assert!(!production.contains("unwrap_or(EarlyDispatch"));
    }
}
