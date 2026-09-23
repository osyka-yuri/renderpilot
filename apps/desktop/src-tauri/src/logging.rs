//! Native tracing layers for console output and privacy-safe file diagnostics.

use std::env;

use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{EnvFilter, Layer, filter::filter_fn, layer::Context, prelude::*};

const TYPED_DIAGNOSTIC_MARKER: &str = "__renderpilot_diagnostic_recorded";
const DIAGNOSTIC_PATH_FIELD: &str = "diagnostic_path";

pub(crate) fn install() {
    let subscriber = make_subscriber(
        console_filter_from_env(),
        std::io::stderr,
        crate::backend_diagnostics::record_rust_log,
    );
    report_global_init(
        &tracing::subscriber::set_global_default(subscriber),
        "tracing_subscriber_already_initialized",
    );
    report_global_init(
        &tracing_log::LogTracer::init(),
        "log_bridge_already_initialized",
    );
}

fn report_global_init<E>(result: &Result<(), E>, code: &'static str) {
    if result.is_err() {
        eprintln!("RenderPilot: {code}");
    }
}

fn console_filter_from_env() -> EnvFilter {
    let parsed = match env::var("RUST_LOG") {
        Ok(value) => parse_console_filter(Some(&value)),
        Err(env::VarError::NotPresent) => parse_console_filter(None),
        Err(env::VarError::NotUnicode(_)) => None,
    };
    parsed.unwrap_or_else(|| {
        eprintln!("RenderPilot: invalid RUST_LOG; using info");
        EnvFilter::new("info")
    })
}

fn parse_console_filter(value: Option<&str>) -> Option<EnvFilter> {
    match value.filter(|value| !value.trim().is_empty()) {
        Some(value) => EnvFilter::try_new(value).ok(),
        None => Some(EnvFilter::new("info")),
    }
}

fn make_subscriber<F, W>(filter: EnvFilter, writer: W, record: F) -> impl Subscriber + Send + Sync
where
    F: Fn(crate::diagnostics::RustLogEvent) + Send + Sync + 'static,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    let console = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(filter);
    let file = FileDiagnosticLayer::new(record).with_filter(filter_fn(file_metadata_is_eligible));
    tracing_subscriber::registry().with(console).with(file)
}

fn file_metadata_is_eligible(metadata: &Metadata<'_>) -> bool {
    if metadata.fields().field(TYPED_DIAGNOSTIC_MARKER).is_some()
        || !matches!(
            *metadata.level(),
            tracing::Level::WARN | tracing::Level::ERROR
        )
    {
        return false;
    }
    let Some(module) = metadata.module_path() else {
        return false;
    };
    metadata.line().is_some() && crate::diagnostics::RustLogCategory::for_module(module).is_some()
}

fn project_event_metadata(
    metadata: &Metadata<'static>,
) -> Option<crate::diagnostics::RustLogEvent> {
    if !file_metadata_is_eligible(metadata) {
        return None;
    }
    crate::diagnostics::RustLogEvent::from_site(
        *metadata.level(),
        metadata.module_path()?,
        metadata.line()?,
    )
}

#[derive(Default)]
struct DiagnosticFieldVisitor {
    path: Option<String>,
}

impl Visit for DiagnosticFieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == DIAGNOSTIC_PATH_FIELD {
            self.path =
                crate::diagnostics::is_valid_diagnostic_path(value).then(|| value.to_owned());
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

fn project_event(event: &Event<'_>) -> Option<crate::diagnostics::RustLogEvent> {
    let mut projected = project_event_metadata(event.metadata())?;
    let mut fields = DiagnosticFieldVisitor::default();
    event.record(&mut fields);
    projected.path = fields.path;
    Some(projected)
}

struct FileDiagnosticLayer<F> {
    record: F,
}

impl<F> FileDiagnosticLayer<F> {
    fn new(record: F) -> Self {
        Self { record }
    }
}

impl<S, F> Layer<S> for FileDiagnosticLayer<F>
where
    S: Subscriber,
    F: Fn(crate::diagnostics::RustLogEvent) + Send + Sync + 'static,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if let Some(projected) = project_event(event) {
            (self.record)(projected);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fmt,
        io::{self, Write},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };

    use log::{Level, Log, Record};
    use tracing_subscriber::{Layer, fmt::MakeWriter, prelude::*};

    use super::{
        FileDiagnosticLayer, file_metadata_is_eligible, make_subscriber, parse_console_filter,
    };
    use crate::diagnostics::{DiagnosticLevel, RustLogEvent};

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    struct CaptureGuard(Arc<Mutex<Vec<u8>>>);

    impl Write for CaptureGuard {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> MakeWriter<'writer> for CaptureWriter {
        type Writer = CaptureGuard;

        fn make_writer(&'writer self) -> Self::Writer {
            CaptureGuard(Arc::clone(&self.0))
        }
    }

    impl CaptureWriter {
        fn contents(&self) -> String {
            String::from_utf8_lossy(
                &self
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )
            .into_owned()
        }
    }

    fn event_sink(events: Arc<Mutex<Vec<RustLogEvent>>>) -> impl Fn(RustLogEvent) + Send + Sync {
        move |event| {
            events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(event);
        }
    }

    #[test]
    fn console_filter_defaults_to_info_and_honors_off_and_target_directives() {
        let default = parse_console_filter(None).expect("default INFO filter");
        let writer = CaptureWriter::default();
        tracing::subscriber::with_default(make_subscriber(default, writer.clone(), |_| {}), || {
            tracing::debug!("default_debug_hidden");
            tracing::info!("default_info_visible");
        });
        let output = writer.contents();
        assert!(!output.contains("default_debug_hidden"));
        assert!(output.contains("default_info_visible"));

        let off = parse_console_filter(Some("off")).expect("off filter");
        let writer = CaptureWriter::default();
        tracing::subscriber::with_default(make_subscriber(off, writer.clone(), |_| {}), || {
            tracing::error!("off_error_hidden")
        });
        assert!(!writer.contents().contains("off_error_hidden"));

        let targeted = parse_console_filter(Some("renderpilot_desktop::logging::tests=debug"))
            .expect("target directive");
        let writer = CaptureWriter::default();
        tracing::subscriber::with_default(
            make_subscriber(targeted, writer.clone(), |_| {}),
            || tracing::debug!("target_debug_visible"),
        );
        assert!(writer.contents().contains("target_debug_visible"));
    }

    #[test]
    fn file_projection_is_independent_from_console_filter_and_only_keeps_warn_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let writer = CaptureWriter::default();
        let sink = event_sink(Arc::clone(&events));
        let filter = parse_console_filter(Some("off")).expect("off filter");
        tracing::subscriber::with_default(make_subscriber(filter, writer.clone(), sink), || {
            tracing::info!("info_is_not_a_file_event");
            tracing::warn!("warning_is_a_file_event");
            tracing::error!("error_is_a_file_event");
        });
        let events = events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].level, DiagnosticLevel::Warning);
        assert_eq!(events[1].level, DiagnosticLevel::Error);
        assert!(writer.contents().is_empty(), "console remains filtered off");
    }

    struct SecretValue(Arc<AtomicBool>);

    impl fmt::Debug for SecretValue {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.store(true, Ordering::SeqCst);
            formatter.write_str("C:/Users/private/Games/secret.json")
        }
    }

    #[test]
    fn file_layer_reads_only_reserved_paths_without_formatting_other_fields() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let visited = Arc::new(AtomicBool::new(false));
        let sink = event_sink(Arc::clone(&events));
        let subscriber =
            tracing_subscriber::registry().with(FileDiagnosticLayer::new(sink).with_filter(
                tracing_subscriber::filter::filter_fn(file_metadata_is_eligible),
            ));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                target: "C:/Users/private/Games/private-target",
                secret = tracing::field::debug(&SecretValue(Arc::clone(&visited))),
                diagnostic_path = String::from("D:/Games/SecretTitle/private.ini"),
                "private argument C:/Users/private/Games/private-message"
            );
        });
        assert!(
            !visited.load(Ordering::SeqCst),
            "the layer must not visit fields"
        );
        let events = events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].path.as_deref(),
            Some("D:/Games/SecretTitle/private.ini")
        );
        let encoded = format!("{:?}", events[0]);
        for secret in [
            "private-target",
            "private-message",
            "secret.json",
            "logging.rs",
        ] {
            assert!(!encoded.contains(secret));
        }
        assert!(!encoded.contains("C:/Users/private"));
    }

    #[test]
    fn invalid_reserved_path_is_omitted_without_dropping_the_event() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = event_sink(Arc::clone(&events));
        let subscriber =
            tracing_subscriber::registry().with(FileDiagnosticLayer::new(sink).with_filter(
                tracing_subscriber::filter::filter_fn(file_metadata_is_eligible),
            ));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                diagnostic_path = "x".repeat(1025),
                "path omitted but warning retained"
            );
        });
        let events = events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(events.len(), 1);
        assert!(events[0].path.is_none());
    }

    #[test]
    fn marked_typed_callsite_is_not_projected_twice() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = event_sink(Arc::clone(&events));
        let subscriber =
            tracing_subscriber::registry().with(FileDiagnosticLayer::new(sink).with_filter(
                tracing_subscriber::filter::filter_fn(file_metadata_is_eligible),
            ));
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!(
                __renderpilot_diagnostic_recorded = tracing::field::Empty,
                "typed diagnostic remains the only file record"
            );
        });
        assert!(
            events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }

    #[test]
    fn log_tracer_events_keep_console_output_and_never_enter_the_file_projection() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let tracer = tracing_log::LogTracer::new();
        let writer = CaptureWriter::default();
        let filter = parse_console_filter(None).expect("default INFO filter");
        tracing::subscriber::with_default(
            make_subscriber(filter, writer.clone(), event_sink(Arc::clone(&events))),
            || {
                emit_legacy_log(
                    &tracer,
                    Level::Debug,
                    "external_crate",
                    "legacy_debug_hidden",
                );
                emit_legacy_log(
                    &tracer,
                    Level::Info,
                    "external_crate",
                    "legacy_info_visible",
                );
            },
        );
        let output = writer.contents();
        assert!(!output.contains("legacy_debug_hidden"));
        assert!(output.contains("legacy_info_visible"));

        let writer = CaptureWriter::default();
        let filter = parse_console_filter(Some("off")).expect("off filter");
        tracing::subscriber::with_default(
            make_subscriber(filter, writer.clone(), event_sink(Arc::clone(&events))),
            || emit_legacy_log(&tracer, Level::Error, "external_crate", "legacy_off_hidden"),
        );
        assert!(!writer.contents().contains("legacy_off_hidden"));

        let writer = CaptureWriter::default();
        let filter = parse_console_filter(Some("external_crate=debug"))
            .expect("third-party target directive");
        tracing::subscriber::with_default(
            make_subscriber(filter, writer.clone(), event_sink(Arc::clone(&events))),
            || {
                emit_legacy_log(
                    &tracer,
                    Level::Debug,
                    "external_crate",
                    "third-party bridged message",
                );
            },
        );
        assert!(writer.contents().contains("third-party bridged message"));
        assert!(
            events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }

    #[test]
    fn invalid_filter_configuration_is_rejected_for_the_safe_info_fallback() {
        assert!(parse_console_filter(Some("=invalid-directive")).is_none());
    }

    #[test]
    fn global_initialization_conflicts_are_reported_without_panicking() {
        const CHILD_FLAG: &str = "RENDERPILOT_LOGGING_INIT_CONFLICT_CHILD";
        let output =
            std::process::Command::new(std::env::current_exe().expect("test executable path"))
                .args([
                    "--exact",
                    "logging::tests::global_initialization_conflict_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env(CHILD_FLAG, "1")
                .env("RUST_LOG", "info")
                .output()
                .expect("spawn isolated initialization conflict test");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "child test failed: {stderr}");
        assert!(stderr.contains("RenderPilot: tracing_subscriber_already_initialized"));
        assert!(stderr.contains("RenderPilot: log_bridge_already_initialized"));
    }

    #[test]
    #[ignore = "launched in a fresh process by the conflict regression test"]
    fn global_initialization_conflict_child() {
        const CHILD_FLAG: &str = "RENDERPILOT_LOGGING_INIT_CONFLICT_CHILD";
        assert_eq!(std::env::var(CHILD_FLAG).as_deref(), Ok("1"));
        tracing::subscriber::set_global_default(tracing_subscriber::registry())
            .expect("preinstall global tracing subscriber");
        tracing_log::LogTracer::init().expect("preinstall global legacy-log bridge");

        let result = std::panic::catch_unwind(super::install);
        assert!(
            result.is_ok(),
            "install must handle both conflicts without panic"
        );
    }

    fn emit_legacy_log(
        tracer: &tracing_log::LogTracer,
        level: Level,
        target: &'static str,
        message: &'static str,
    ) {
        let args = format_args!("{message}");
        let record = Record::builder()
            .args(args)
            .level(level)
            .target(target)
            .module_path_static(Some("renderpilot_orchestration::catalog::scan"))
            .file_static(Some("C:/Users/private/Games/secret.rs"))
            .line(Some(17))
            .build();
        Log::log(tracer, &record);
    }
}
