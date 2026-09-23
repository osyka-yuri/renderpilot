//! Installed-build diagnostics in the resolved application data directory.

use std::{
    borrow::Cow,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, Once, OnceLock},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use super::{
    DiagnosticLevel, FrontendDiagnosticEvent, RustLogEvent,
    name::{
        InstalledDiagnosticName, canonical_filename_prefix, display_id_matches,
        format_filename_timestamp, is_valid_timestamp_utc, parse_installed_diagnostic_name,
    },
    writer::{
        DiagnosticCloseStatus, DiagnosticEmitStatus, DiagnosticWriter, SealedProfile,
        WriterMetadata, sealed,
    },
};
use crate::diagnostic_event::{BackendDiagnosticEvent, BackendDiagnosticLevel};

const SCHEMA: &str = "renderpilot.installed.diagnostics";
const VERSION: u8 = 1;
const MAX_COMPLETED_FILES: usize = 8;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_CANONICAL_ATTEMPTS: usize = 64;
const MAX_HEADER_BYTES: usize = 4 * 1024;
const LOG_SUFFIX: &str = ".log";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstalledLifecycle {
    Ready,
    Failure,
    Exit,
}

#[derive(Clone, Debug)]
enum InstalledEvent {
    First,
    Lifecycle(InstalledLifecycle),
    Rust(RustLogEvent),
    Backend(BackendDiagnosticEvent),
    Frontend(FrontendDiagnosticEvent),
    Capacity,
}

struct InstalledProfile;

impl sealed::Sealed for InstalledProfile {}

impl SealedProfile for InstalledProfile {
    type Context = InstalledContext;
    type Event = InstalledEvent;

    fn encode(
        metadata: WriterMetadata,
        context: &Self::Context,
        event: &Self::Event,
    ) -> Option<Vec<u8>> {
        let mut reason_code = None;
        let mut path = None;
        let mut source_module = None;
        let mut source_line = None;
        let (level, phase, code, operation, category, site_id, locale, mode) = match event {
            InstalledEvent::First => (
                DiagnosticLevel::Info,
                "startup",
                "startup_started",
                None,
                None,
                None,
                None,
                None,
            ),
            InstalledEvent::Lifecycle(InstalledLifecycle::Ready) => (
                DiagnosticLevel::Info,
                "desktop_shell",
                "desktop_shell_ready",
                None,
                None,
                None,
                None,
                None,
            ),
            InstalledEvent::Lifecycle(InstalledLifecycle::Failure) => (
                DiagnosticLevel::Error,
                "startup",
                "startup_failed",
                None,
                None,
                None,
                None,
                None,
            ),
            InstalledEvent::Lifecycle(InstalledLifecycle::Exit) => (
                DiagnosticLevel::Info,
                "desktop_shell",
                "controlled_exit",
                None,
                None,
                None,
                None,
                None,
            ),
            InstalledEvent::Rust(event) => {
                let site_id = event.site_id_hex();
                path = event.path.as_deref();
                source_module = Some(event.source_module);
                source_line = Some(event.source_line);
                (
                    event.level,
                    "rust_log",
                    match event.level {
                        DiagnosticLevel::Info | DiagnosticLevel::Warning => "rust_warning",
                        DiagnosticLevel::Error => "rust_error",
                    },
                    None,
                    Some(event.category.code()),
                    Some(site_id),
                    None,
                    None,
                )
            }
            InstalledEvent::Frontend(event) => (
                event.level(),
                event.phase_code(),
                event.event_code(),
                event.operation(),
                None,
                None,
                event.locale(),
                event.mode(),
            ),
            InstalledEvent::Backend(event) => {
                let record = event.record();
                reason_code = record.reason_code();
                path = record.path();
                (
                    match record.level() {
                        BackendDiagnosticLevel::Warning => DiagnosticLevel::Warning,
                        BackendDiagnosticLevel::Error => DiagnosticLevel::Error,
                    },
                    record.phase(),
                    record.code(),
                    record.operation(),
                    None,
                    None,
                    None,
                    None,
                )
            }
            InstalledEvent::Capacity => (
                DiagnosticLevel::Info,
                "diagnostics",
                "capacity_reached",
                None,
                None,
                None,
                None,
                None,
            ),
        };
        serde_json::to_vec(&InstalledRecord {
            schema: SCHEMA,
            version: VERSION,
            unix_ms: metadata.unix_ms,
            timestamp_utc: metadata.timestamp_utc.as_deref(),
            seq: metadata.sequence,
            role: "app",
            app_version: &context.app_version,
            session: &context.session,
            segment: context.segment,
            level: level.code(),
            phase,
            code,
            operation,
            category,
            site_id,
            locale,
            mode,
            reason_code,
            path,
            source_module,
            source_line,
        })
        .ok()
    }

    fn encode_capacity(metadata: WriterMetadata, context: &Self::Context) -> Option<Vec<u8>> {
        Self::encode(metadata, context, &InstalledEvent::Capacity)
    }
}

#[derive(Clone, Debug)]
struct InstalledContext {
    app_version: String,
    session: String,
    start_timestamp: String,
    segment: u32,
}

#[derive(Serialize)]
struct InstalledRecord<'a> {
    schema: &'static str,
    version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_utc: Option<&'a str>,
    seq: u64,
    role: &'static str,
    app_version: &'a str,
    session: &'a str,
    segment: u32,
    level: &'static str,
    phase: &'static str,
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_module: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_line: Option<u32>,
}

impl InstalledRecord<'_> {
    fn retained_header_matches(bytes: &[u8], identity: &InstalledDiagnosticName) -> bool {
        let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') else {
            return false;
        };
        let Ok(first) = serde_json::from_slice::<InstalledFirstRecord<'_>>(&bytes[..newline])
        else {
            return false;
        };
        first.schema == SCHEMA
            && first.version == VERSION
            && first.seq == 1
            && first.role == "app"
            && display_id_matches(&identity.display_id, &first.session, 32)
            && first.segment == identity.segment
            && first
                .timestamp_utc
                .as_deref()
                .is_none_or(is_valid_timestamp_utc)
            && first.level == "info"
            && first.phase == "startup"
            && first.code == "startup_started"
            && first.operation.is_none()
            && first.category.is_none()
            && first.site_id.is_none()
            && first.locale.is_none()
            && first.mode.is_none()
            && first.reason_code.is_none()
            && first.path.is_none()
            && first.source_module.is_none()
            && first.source_line.is_none()
            && valid_version(&first.app_version)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledFirstRecord<'a> {
    #[serde(borrow)]
    schema: Cow<'a, str>,
    version: u8,
    #[serde(default, rename = "unix_ms", deserialize_with = "present_u64")]
    _unix_ms: Option<u64>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    timestamp_utc: Option<Cow<'a, str>>,
    seq: u64,
    #[serde(borrow)]
    role: Cow<'a, str>,
    #[serde(borrow)]
    app_version: Cow<'a, str>,
    #[serde(borrow)]
    session: Cow<'a, str>,
    segment: u32,
    #[serde(borrow)]
    level: Cow<'a, str>,
    #[serde(borrow)]
    phase: Cow<'a, str>,
    #[serde(borrow)]
    code: Cow<'a, str>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    operation: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    category: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    site_id: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    locale: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    mode: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    reason_code: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    path: Option<Cow<'a, str>>,
    #[serde(default, borrow, deserialize_with = "present_cow_string")]
    source_module: Option<Cow<'a, str>>,
    #[serde(default, deserialize_with = "present_u32")]
    source_line: Option<u32>,
}

fn present_u64<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Some)
}

fn present_u32<'de, D>(deserializer: D) -> std::result::Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u32::deserialize(deserializer).map(Some)
}

fn present_cow_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Cow<'de, str>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Cow::<'de, str>::deserialize(deserializer).map(Some)
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
}

struct InstalledWriter {
    inner: DiagnosticWriter<InstalledProfile>,
}

impl InstalledWriter {
    fn open(file: File, context: InstalledContext) -> Option<Self> {
        let mut result = Self {
            inner: DiagnosticWriter::open(file, context),
        };
        (result.inner.emit(&InstalledEvent::First) == DiagnosticEmitStatus::Written)
            .then_some(result)
    }

    fn emit(&mut self, event: &InstalledEvent) -> DiagnosticEmitStatus {
        self.inner.emit(event)
    }

    fn close(&mut self) -> DiagnosticCloseStatus {
        self.inner.close()
    }

    fn into_closed_context(self) -> InstalledContext {
        self.inner.into_closed_context()
    }
}

struct InstalledSession {
    directory: PathBuf,
    active_name: String,
    writer: Option<InstalledWriter>,
    _process_lock: File,
}

impl InstalledSession {
    fn emit(&mut self, event: &InstalledEvent) -> DiagnosticEmitStatus {
        let Some(writer) = self.writer.as_mut() else {
            return DiagnosticEmitStatus::Disabled;
        };
        let status = writer.emit(event);
        if status != DiagnosticEmitStatus::Capacity {
            return status;
        }
        self.rollover(event)
    }

    fn rollover(&mut self, event: &InstalledEvent) -> DiagnosticEmitStatus {
        let Some(mut previous) = self.writer.take() else {
            return DiagnosticEmitStatus::Disabled;
        };
        if previous.close() == DiagnosticCloseStatus::Failed {
            report_failure();
            return DiagnosticEmitStatus::Disabled;
        }
        let context = previous.into_closed_context();
        if retain_completed(&self.directory, None).is_err() {
            report_retention_failure();
        }
        let Some(segment) = context.segment.checked_add(1) else {
            report_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let Some(name) = canonical_name_at(&context.start_timestamp, &context.session, segment)
        else {
            report_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let Ok(file) = create_new(&self.directory, &name) else {
            report_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let next_context = InstalledContext {
            app_version: context.app_version,
            session: context.session,
            start_timestamp: context.start_timestamp,
            segment,
        };
        let Some(mut writer) = InstalledWriter::open(file, next_context) else {
            report_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let status = writer.emit(event);
        if status != DiagnosticEmitStatus::Written {
            let _ = writer.close();
            report_failure();
            return DiagnosticEmitStatus::Disabled;
        }
        self.active_name = name;
        self.writer = Some(writer);
        DiagnosticEmitStatus::Written
    }

    fn close(mut self) {
        if let Some(mut writer) = self.writer.take()
            && writer.close() == DiagnosticCloseStatus::Failed
        {
            report_failure();
        }
        if retain_completed(&self.directory, None).is_err() {
            report_retention_failure();
        }
    }
}

enum InstalledObserver {
    Uninitialized,
    Active(Box<InstalledSession>),
    Disabled,
    Closed,
}

static OBSERVER: OnceLock<Mutex<InstalledObserver>> = OnceLock::new();
static FAILURE_ONCE: Once = Once::new();
static RETENTION_FAILURE_ONCE: Once = Once::new();

pub(crate) fn install_installed() {
    let observer = OBSERVER.get_or_init(|| Mutex::new(InstalledObserver::Uninitialized));
    let Ok(mut state) = observer.lock() else {
        report_failure();
        return;
    };
    if !matches!(*state, InstalledObserver::Uninitialized) {
        return;
    }
    match open_session() {
        Ok(session) => *state = InstalledObserver::Active(Box::new(session)),
        Err(()) => {
            *state = InstalledObserver::Disabled;
            report_failure();
        }
    }
}

pub(crate) fn installed_event(event: InstalledLifecycle) {
    emit_event(&InstalledEvent::Lifecycle(event));
}

pub(crate) fn record_backend_event(event: BackendDiagnosticEvent) {
    emit_event(&InstalledEvent::Backend(event));
}

pub(crate) fn rust_log_event(event: RustLogEvent) {
    emit_event(&InstalledEvent::Rust(event));
}

pub(crate) fn frontend_event(event: FrontendDiagnosticEvent) {
    emit_event(&InstalledEvent::Frontend(event));
}

fn emit_event(event: &InstalledEvent) {
    let Some(observer) = OBSERVER.get() else {
        return;
    };
    let Ok(mut state) = observer.lock() else {
        report_failure();
        return;
    };
    let status = match &mut *state {
        InstalledObserver::Active(session) => session.emit(event),
        InstalledObserver::Uninitialized
        | InstalledObserver::Disabled
        | InstalledObserver::Closed => return,
    };
    if status == DiagnosticEmitStatus::Disabled {
        *state = InstalledObserver::Disabled;
        report_failure();
    }
}

pub(crate) fn shutdown_installed() {
    let Some(observer) = OBSERVER.get() else {
        return;
    };
    let Ok(mut state) = observer.lock() else {
        return;
    };
    let prior = std::mem::replace(&mut *state, InstalledObserver::Closed);
    drop(state);
    if let InstalledObserver::Active(session) = prior {
        (*session).close();
    }
}

fn report_failure() {
    FAILURE_ONCE.call_once(|| eprintln!("RenderPilot: installed_diagnostics_disabled"));
}

fn report_retention_failure() {
    RETENTION_FAILURE_ONCE.call_once(|| {
        let _ = std::io::stderr()
            .lock()
            .write_all(b"RenderPilot: installed_diagnostics_retention_failed\n");
    });
}

fn open_session() -> Result<InstalledSession, ()> {
    let data_directory =
        renderpilot_orchestration::resolved_app_data_directory().map_err(|_| ())?;
    open_session_in_data_directory(&data_directory)
}

fn open_session_in_data_directory(data_directory: &Path) -> Result<InstalledSession, ()> {
    let directory = data_directory.join("logs").join("installed").join("app");
    fs::create_dir_all(&directory).map_err(|_| ())?;
    let lock_path = directory.join(".diagnostics.lock");
    let process_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|_| ())?;
    process_lock.try_lock().map_err(|_| ())?;

    let start_timestamp = format_filename_timestamp(SystemTime::now());
    for _ in 0..64 {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| ())?;
        let session = lowercase_hex(&random);
        let segment = 0;
        let name = canonical_name_at(&start_timestamp, &session, segment).ok_or(())?;
        let file = match create_new(&directory, &name) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(()),
        };
        let context = InstalledContext {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            session,
            start_timestamp,
            segment,
        };
        let writer = InstalledWriter::open(file, context).ok_or(())?;
        let session = InstalledSession {
            directory,
            active_name: name,
            writer: Some(writer),
            _process_lock: process_lock,
        };
        // The new active leaf is always excluded from verified retention.
        if retain_completed(&session.directory, Some(&session.active_name)).is_err() {
            report_retention_failure();
        }
        return Ok(session);
    }
    Err(())
}

#[cfg(test)]
fn canonical_name(session: &str, segment: u32) -> String {
    canonical_name_at("2026-09-23_14-35-12Z", session, segment)
        .expect("test and runtime session IDs are canonical lowercase hex")
}

fn canonical_name_at(timestamp: &str, session: &str, segment: u32) -> Option<String> {
    let prefix = canonical_filename_prefix(timestamp, session, 32)?;
    Some(format!("{prefix}-s{segment:08x}{LOG_SUFFIX}"))
}

fn create_new(directory: &Path, name: &str) -> std::io::Result<File> {
    OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(directory.join(name))
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn retain_completed(directory: &Path, active_name: Option<&str>) -> Result<(), ()> {
    let entries = fs::read_dir(directory).map_err(|_| ())?;
    let mut visited = 0usize;
    let mut canonical_attempts = 0usize;
    let mut candidates = Vec::<(SystemTime, String)>::new();
    for entry in entries {
        visited = visited.saturating_add(1);
        if visited > MAX_DIRECTORY_ENTRIES {
            return Err(());
        }
        let entry = entry.map_err(|_| ())?;
        let name = entry.file_name().into_string().map_err(|_| ())?;
        if active_name == Some(name.as_str()) {
            continue;
        }
        let Some(identity) = parse_installed_diagnostic_name(&name) else {
            continue;
        };
        canonical_attempts = canonical_attempts.saturating_add(1);
        if canonical_attempts > MAX_CANONICAL_ATTEMPTS {
            return Err(());
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| ())?;
        if !metadata.file_type().is_file() {
            return Err(());
        }
        let file = File::open(&path).map_err(|_| ())?;
        let Some(first) = read_header(&file).map_err(|_| ())? else {
            continue;
        };
        if !InstalledRecord::retained_header_matches(&first, &identity) {
            continue;
        }
        let modified = metadata.modified().map_err(|_| ())?;
        candidates.push((modified, name));
    }
    candidates.sort_unstable();
    let remove_count = candidates.len().saturating_sub(MAX_COMPLETED_FILES);
    for (_, name) in candidates.into_iter().take(remove_count) {
        fs::remove_file(directory.join(name)).map_err(|_| ())?;
    }
    Ok(())
}

fn read_header(reader: impl Read) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::with_capacity(MAX_HEADER_BYTES + 1);
    let mut reader = BufReader::new(reader.take((MAX_HEADER_BYTES + 1) as u64));
    let read = reader.read_until(b'\n', &mut bytes)?;
    if read == 0 || read > MAX_HEADER_BYTES || bytes.last() != Some(&b'\n') {
        return Ok(None);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        fs::OpenOptions,
        io::{self, Cursor, Read},
    };

    use super::super::{
        projection::RustLogCategory,
        writer::{SealedProfile, WriterMetadata},
    };
    use super::{
        DiagnosticLevel, InstalledContext, InstalledDiagnosticName, InstalledEvent,
        InstalledFirstRecord, InstalledProfile, InstalledRecord, InstalledSession, InstalledWriter,
        MAX_CANONICAL_ATTEMPTS, MAX_HEADER_BYTES, RustLogEvent, canonical_name, create_new,
        open_session_in_data_directory, read_header, retain_completed,
    };
    use crate::{
        command_error_contract::CommandErrorKind,
        diagnostic_event::{BackendDiagnosticEvent, CommandOperation},
    };

    fn context(session: String, segment: u32) -> InstalledContext {
        InstalledContext {
            app_version: "1.2.3".to_owned(),
            session,
            start_timestamp: "2026-09-23_14-35-12Z".to_owned(),
            segment,
        }
    }

    fn session_id_for_index(index: impl std::fmt::LowerHex) -> String {
        format!("{index:016x}{}", "0".repeat(16))
    }

    #[test]
    fn installed_projection_serializes_only_closed_rust_fields() {
        let event = InstalledEvent::Rust(RustLogEvent {
            level: DiagnosticLevel::Error,
            category: RustLogCategory::StorageFilesystem,
            site_id: [0xab; 16],
            source_module: "renderpilot_orchestration::fs",
            source_line: 81,
            path: Some("D:/Games/Example".to_owned()),
        });
        let bytes = InstalledProfile::encode(
            WriterMetadata {
                unix_ms: Some(10),
                timestamp_utc: Some("1970-01-01T00:00:00.010Z".to_owned()),
                sequence: 2,
            },
            &InstalledContext {
                app_version: "1.2.3".to_owned(),
                session: "a".repeat(32),
                start_timestamp: "2026-09-23_14-35-12Z".to_owned(),
                segment: 4,
            },
            &event,
        )
        .expect("closed projection");
        let record = String::from_utf8(bytes).expect("utf-8 json");
        assert!(record.contains("\"category\":\"storage_filesystem\""));
        assert!(record.contains("\"site_id\":\"abababababababababababababababab\""));
        assert!(record.contains("\"source_module\":\"renderpilot_orchestration::fs\""));
        assert!(record.contains("\"source_line\":81"));
        assert!(record.contains("\"path\":\"D:/Games/Example\""));
        assert!(record.contains("\"timestamp_utc\":\"1970-01-01T00:00:00.010Z\""));
        assert!(!record.contains("message"));
    }

    #[test]
    fn installed_backend_projection_serializes_structured_game_path() {
        let event = InstalledEvent::Backend(BackendDiagnosticEvent::command_failure(
            CommandOperation::InspectGameInstall,
            CommandErrorKind::StaleInstallInspection,
            None,
            Some("D:/Games/Example".to_owned()),
        ));
        let bytes = InstalledProfile::encode(
            WriterMetadata {
                unix_ms: Some(10),
                timestamp_utc: Some("1970-01-01T00:00:00.010Z".to_owned()),
                sequence: 2,
            },
            &context("a".repeat(32), 4),
            &event,
        )
        .expect("closed backend projection");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON record");
        assert_eq!(value["phase"], "command");
        assert_eq!(value["code"], "stale_install_inspection");
        assert_eq!(value["operation"], "inspect_game_install");
        assert_eq!(value["path"], "D:/Games/Example");
        assert!(value.get("message").is_none());
    }

    #[test]
    fn installed_backend_projection_serializes_only_allowlisted_reason_code() {
        let event = InstalledEvent::Backend(BackendDiagnosticEvent::command_failure(
            CommandOperation::InspectGameInstall,
            CommandErrorKind::InvalidInstallRoot,
            Some("contains_proven_install"),
            None,
        ));
        let bytes = InstalledProfile::encode(
            WriterMetadata {
                unix_ms: Some(10),
                timestamp_utc: Some("1970-01-01T00:00:00.010Z".to_owned()),
                sequence: 2,
            },
            &context("a".repeat(32), 4),
            &event,
        )
        .expect("closed backend projection");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON record");
        assert_eq!(value["reason_code"], "contains_proven_install");
        assert!(value.get("path").is_none());
    }

    #[test]
    fn installed_retention_accepts_omitted_optional_fields_and_rejects_them_on_first_record() {
        let context = context("a".repeat(32), 4);
        let identity = InstalledDiagnosticName {
            display_id: context.session[..16].to_owned(),
            segment: context.segment,
        };
        let first = InstalledProfile::encode(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 1,
            },
            &context,
            &InstalledEvent::First,
        )
        .expect("first record");
        let mut line = first.clone();
        line.push(b'\n');
        assert!(InstalledRecord::retained_header_matches(&line, &identity));

        let borrowed: InstalledFirstRecord<'_> =
            serde_json::from_slice(&first).expect("ordinary header borrows its strings");
        assert!(matches!(
            borrowed.app_version,
            std::borrow::Cow::Borrowed("1.2.3")
        ));
        assert!(matches!(
            borrowed.session,
            std::borrow::Cow::Borrowed(value) if value == context.session
        ));

        let escaped = String::from_utf8(first.clone())
            .expect("first record is UTF-8")
            .replace(
                "\"app_version\":\"1.2.3\"",
                "\"app_version\":\"1\\u002e2.3\"",
            )
            .replace(
                &format!("\"session\":\"{}\"", context.session),
                &format!("\"session\":\"\\u0061{}\"", &context.session[1..]),
            );
        let mut escaped_line = escaped.into_bytes();
        escaped_line.push(b'\n');
        let escaped_record: InstalledFirstRecord<'_> =
            serde_json::from_slice(escaped_line.strip_suffix(b"\n").expect("line terminator"))
                .expect("escaped JSON string is semantically valid");
        assert!(matches!(
            escaped_record.app_version,
            std::borrow::Cow::Owned(_)
        ));
        assert!(matches!(escaped_record.session, std::borrow::Cow::Owned(_)));
        assert!(InstalledRecord::retained_header_matches(
            &escaped_line,
            &identity
        ));

        let mut value: serde_json::Value = serde_json::from_slice(&first).expect("JSON record");
        value["path"] = serde_json::json!("D:/Games/Example");
        let mut line = serde_json::to_vec(&value).expect("encode first record with path");
        line.push(b'\n');
        assert!(!InstalledRecord::retained_header_matches(&line, &identity));

        for (field, invalid_value) in [
            ("path", serde_json::Value::Null),
            ("future_field", serde_json::json!("unknown")),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(&first).expect("JSON record");
            value[field] = invalid_value;
            let mut line = serde_json::to_vec(&value).expect("encode invalid first record");
            line.push(b'\n');
            assert!(
                !InstalledRecord::retained_header_matches(&line, &identity),
                "first record rejects invalid {field} field"
            );
        }
    }

    #[test]
    fn operating_system_lock_excludes_a_concurrent_installed_observer() {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        let path = directory.path().join(".diagnostics.lock");
        let first = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .expect("first lock handle");
        first.try_lock().expect("first process lock");
        let second = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .expect("second lock handle");
        assert!(second.try_lock().is_err());
        drop(first);
        second.try_lock().expect("crash/close releases the OS lock");
    }

    #[test]
    fn installed_startup_opens_a_healthy_session_around_prior_crash_orphans() {
        let data_directory = tempfile::tempdir().expect("temporary installed data directory");
        let diagnostics_directory = data_directory.path().join("logs/installed/app");
        fs::create_dir_all(&diagnostics_directory).expect("create diagnostics directory");

        let mut budget_orphans = Vec::new();
        for index in 0..=MAX_CANONICAL_ATTEMPTS {
            let name = canonical_name(&session_id_for_index(index), 7);
            fs::write(diagnostics_directory.join(&name), [])
                .expect("seed over-budget canonical orphan");
            budget_orphans.push(name);
        }

        let empty_name = canonical_name(&"a".repeat(32), 0);
        let partial_name = canonical_name(&"b".repeat(32), 0);
        let empty_bytes = b"";
        let partial_bytes = b"{\"schema\":\"renderpilot.installed.diagnostics\"";
        fs::write(diagnostics_directory.join(&empty_name), empty_bytes)
            .expect("seed empty canonical orphan");
        fs::write(diagnostics_directory.join(&partial_name), partial_bytes)
            .expect("seed partial canonical orphan");

        let mut session = open_session_in_data_directory(data_directory.path())
            .expect("startup opens and retains a new healthy segment");
        let active_name = session.active_name.clone();
        let event = InstalledEvent::Rust(RustLogEvent {
            level: DiagnosticLevel::Error,
            category: RustLogCategory::StorageFilesystem,
            site_id: [0x3d; 16],
            source_module: "renderpilot_orchestration::fs",
            source_line: 17,
            path: None,
        });
        assert_eq!(
            session.emit(&event),
            super::super::writer::DiagnosticEmitStatus::Written
        );
        session.close();

        let contents = fs::read_to_string(diagnostics_directory.join(active_name))
            .expect("read the new completed session");
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2, "startup record followed by a healthy event");
        assert!(lines[0].contains("\"code\":\"startup_started\""));
        assert!(lines[0].contains("\"seq\":1"));
        assert!(lines[1].contains("\"seq\":2"));
        assert!(lines[1].contains("\"site_id\":\"3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d\""));
        assert_eq!(
            fs::read(diagnostics_directory.join(empty_name)).expect("empty orphan is preserved"),
            empty_bytes
        );
        assert_eq!(
            fs::read(diagnostics_directory.join(partial_name))
                .expect("partial orphan is preserved"),
            partial_bytes
        );
        assert!(
            budget_orphans
                .iter()
                .all(|name| diagnostics_directory.join(name).exists())
        );
    }

    #[test]
    fn rollover_replays_the_capacity_trigger_once_under_the_new_segment_header() {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        let session_id = "a".repeat(32);
        let name = canonical_name(&session_id, 0);
        let file = create_new(directory.path(), &name).expect("create first segment");
        let writer = InstalledWriter::open(file, context(session_id, 0))
            .expect("write first segment header");
        let orphan_name = canonical_name(&"b".repeat(32), 1);
        let orphan_bytes = b"{\"schema\":\"renderpilot.installed.diagnostics\"";
        fs::write(directory.path().join(&orphan_name), orphan_bytes)
            .expect("seed interrupted canonical next segment");
        let mut budget_orphans = Vec::new();
        for index in 0..=MAX_CANONICAL_ATTEMPTS {
            let name = canonical_name(&session_id_for_index(index), 7);
            fs::write(directory.path().join(&name), []).expect("seed over-budget orphan");
            budget_orphans.push(name);
        }
        let lock_path = directory.path().join(".diagnostics.lock");
        let process_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .expect("process lock handle");
        process_lock.try_lock().expect("process lock");
        let mut observer = InstalledSession {
            directory: directory.path().to_path_buf(),
            active_name: name,
            writer: Some(writer),
            _process_lock: process_lock,
        };
        let trigger = InstalledEvent::Rust(RustLogEvent {
            level: DiagnosticLevel::Error,
            category: RustLogCategory::StorageFilesystem,
            site_id: [0x5a; 16],
            source_module: "renderpilot_orchestration::fs",
            source_line: 23,
            path: Some("D:/Games/Example".to_owned()),
        });
        let old_name = observer.active_name.clone();
        for _ in 0..20_000 {
            assert_eq!(
                observer.emit(&trigger),
                super::super::writer::DiagnosticEmitStatus::Written
            );
            if observer.active_name != old_name {
                break;
            }
        }
        assert_ne!(
            observer.active_name, old_name,
            "capacity rolls over in time"
        );
        assert_eq!(observer.active_name, canonical_name(&"a".repeat(32), 1));

        let mut file = fs::File::open(directory.path().join(&observer.active_name))
            .expect("open active second segment");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("read second segment");
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2, "header plus exactly one replayed trigger");
        assert!(lines[0].contains("\"segment\":1"));
        assert!(lines[1].contains("\"seq\":2"));
        assert!(lines[1].contains("\"site_id\":\"5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a\""));
        assert!(lines[1].contains("\"path\":\"D:/Games/Example\""));
        assert!(
            fs::read_to_string(directory.path().join(old_name))
                .expect("read completed first segment")
                .contains("\"code\":\"capacity_reached\"")
        );
        assert_eq!(
            fs::read(directory.path().join(orphan_name)).expect("orphan remains untouched"),
            orphan_bytes
        );
        assert!(
            budget_orphans
                .iter()
                .all(|name| directory.path().join(name).exists())
        );
        observer.close();
    }

    #[test]
    fn retention_keeps_eight_verified_segments_and_leaves_incomplete_or_foreign_files() {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        for index in 0..9_u32 {
            let session = session_id_for_index(index);
            let name = canonical_name(&session, 0);
            let file = create_new(directory.path(), &name).expect("create completed segment");
            let mut writer = InstalledWriter::open(file, context(session, 0))
                .expect("write first segment header");
            assert_eq!(
                writer.close(),
                super::super::writer::DiagnosticCloseStatus::Synced
            );
        }

        let empty_name = canonical_name(&"e".repeat(32), 0);
        fs::write(directory.path().join(&empty_name), []).expect("seed empty orphan");
        let partial_name = canonical_name(&"f".repeat(32), 0);
        let partial_bytes = b"{\"schema\":\"renderpilot.installed.diagnostics\"";
        fs::write(directory.path().join(&partial_name), partial_bytes)
            .expect("seed partial orphan");
        let foreign_name = canonical_name(&"d".repeat(32), 7);
        let foreign_file = create_new(directory.path(), &foreign_name)
            .expect("create canonical leaf with a mismatching display ID");
        let mut foreign_writer = InstalledWriter::open(foreign_file, context("a".repeat(32), 7))
            .expect("write a valid header with a mismatching filename ID");
        assert_eq!(
            foreign_writer.close(),
            super::super::writer::DiagnosticCloseStatus::Synced
        );
        let legacy_name = format!("{}-s00000000.log", "c".repeat(32));
        fs::write(directory.path().join(&legacy_name), b"old dev log\n")
            .expect("seed a prior development filename");

        retain_completed(directory.path(), None).expect("verified retention");
        let retained = fs::read_dir(directory.path())
            .expect("list retained files")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".log"))
            .count();
        assert_eq!(retained, 12, "eight verified plus four untouched leaves");
        assert!(directory.path().join(empty_name).exists());
        assert_eq!(
            fs::read(directory.path().join(partial_name)).unwrap(),
            partial_bytes
        );
        assert!(directory.path().join(foreign_name).exists());
        assert_eq!(
            fs::read(directory.path().join(legacy_name)).unwrap(),
            b"old dev log\n",
            "legacy development names remain outside retention"
        );

        let healthy_name = canonical_name(&"a".repeat(32), 3);
        let file = create_new(directory.path(), &healthy_name).expect("create healthy segment");
        let mut writer =
            InstalledWriter::open(file, context("a".repeat(32), 3)).expect("healthy first record");
        let event = InstalledEvent::Rust(RustLogEvent {
            level: DiagnosticLevel::Error,
            category: RustLogCategory::StorageFilesystem,
            site_id: [0x4c; 16],
            source_module: "renderpilot_orchestration::fs",
            source_line: 31,
            path: None,
        });
        assert_eq!(
            writer.emit(&event),
            super::super::writer::DiagnosticEmitStatus::Written
        );
        assert!(
            fs::read_to_string(directory.path().join(healthy_name))
                .expect("healthy file remains writable")
                .contains("\"seq\":2")
        );
    }

    #[test]
    fn canonical_attempt_budget_counts_unverified_leaves_before_any_deletion() {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        for index in 0..65_u32 {
            let name = canonical_name(&session_id_for_index(index), 0);
            fs::write(directory.path().join(name), []).expect("seed empty canonical leaf");
        }

        assert!(retain_completed(directory.path(), None).is_err());
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("enumerate unchanged directory")
                .count(),
            65
        );
    }

    #[test]
    fn real_canonical_io_errors_remain_fail_closed() {
        let directory = tempfile::tempdir().expect("temporary diagnostics directory");
        let mut stream_failure = ErrorReader;
        assert!(read_header(&mut stream_failure).is_err());

        let mut verified = Vec::new();
        for index in 0..9_u32 {
            let session = session_id_for_index(index);
            let name = canonical_name(&session, 0);
            let file = create_new(directory.path(), &name).expect("create verified segment");
            let mut writer = InstalledWriter::open(file, context(session, 0))
                .expect("write verified first record");
            assert_eq!(
                writer.close(),
                super::super::writer::DiagnosticCloseStatus::Synced
            );
            verified.push(name);
        }

        let invalid_type = canonical_name(&"f".repeat(32), 99);
        fs::create_dir(directory.path().join(&invalid_type)).expect("seed non-file canonical");
        assert!(retain_completed(directory.path(), None).is_err());
        assert!(directory.path().join(invalid_type).is_dir());
        assert_eq!(
            verified
                .iter()
                .filter(|name| directory.path().join(name).exists())
                .count(),
            9,
            "retention does not delete until the full classification succeeds"
        );
    }

    struct ErrorReader;

    impl Read for ErrorReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected stream failure"))
        }
    }

    #[test]
    fn bounded_header_reader_classifies_eof_and_size_without_hiding_io_errors() {
        assert_eq!(read_header(Cursor::new(Vec::<u8>::new())).unwrap(), None);
        assert_eq!(read_header(Cursor::new(b"partial".to_vec())).unwrap(), None);

        let mut oversized = vec![b'x'; MAX_HEADER_BYTES];
        oversized.push(b'\n');
        assert_eq!(read_header(Cursor::new(oversized)).unwrap(), None);

        assert_eq!(
            read_header(Cursor::new(b"{\"valid\":true}\n".to_vec())).unwrap(),
            Some(b"{\"valid\":true}\n".to_vec())
        );
    }
}
