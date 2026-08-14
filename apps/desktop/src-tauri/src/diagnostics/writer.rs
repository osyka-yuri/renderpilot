use std::{
    fs::File,
    io::{self, Write},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const MAX_LINE_BYTES: usize = 4 * 1024;
pub(super) const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const TERMINAL_RESERVE_BYTES: usize = MAX_LINE_BYTES;

pub(super) mod sealed {
    pub trait Sealed {}
}

/// A profile may only be defined inside `diagnostics`.  This keeps generic
/// writer construction from becoming an arbitrary serialization sink.
pub(super) trait SealedProfile: sealed::Sealed {
    type Context;
    type Event;

    fn encode(
        metadata: WriterMetadata,
        context: &Self::Context,
        event: Self::Event,
    ) -> Option<Vec<u8>>;
    fn encode_capacity(metadata: WriterMetadata, context: &Self::Context) -> Option<Vec<u8>>;
}

/// Metadata owned by the mechanism, not supplied by callers.
#[derive(Clone, Copy, Debug)]
pub(super) struct WriterMetadata {
    pub(super) unix_ms: Option<u64>,
    pub(super) sequence: u64,
}

impl WriterMetadata {
    fn next(sequence: u64) -> Self {
        Self {
            unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok()),
            sequence,
        }
    }
}

pub(super) trait DiagnosticSink: Write {
    fn sync_all(&mut self) -> io::Result<()>;
}

impl DiagnosticSink for File {
    fn sync_all(&mut self) -> io::Result<()> {
        File::sync_all(self)
    }
}

/// The only observer outcomes consumed by the App session state machine.
/// `Sealed` is the bounded capacity terminal marker, never a sink failure;
/// `Disabled` is an unusable observer and must not trigger another attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use = "observer emit status must drive the App observer state"]
pub(crate) enum DiagnosticEmitStatus {
    Written,
    Sealed,
    Disabled,
}

/// Result of the single controlled flush/sync attempt.  Closing a disabled
/// writer requires no further I/O; a failed active close must be reported by
/// the portable adapter without reopening the sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use = "observer close status must be handled by the portable adapter"]
pub(crate) enum DiagnosticCloseStatus {
    Synced,
    NotRequired,
    Failed,
}

#[derive(Debug)]
enum WriterState<S> {
    Active(S),
    Sealed(S),
    Disabled,
}

/// Generic bounded append-only NDJSON mechanics.  This type and its raw-byte
/// path are module-private: profiles can only provide typed events.
#[derive(Debug)]
pub(super) struct DiagnosticWriter<P: SealedProfile, S = File> {
    context: P::Context,
    state: WriterState<S>,
    next_sequence: u64,
    written: usize,
    _profile: std::marker::PhantomData<P>,
}

impl<P: SealedProfile> DiagnosticWriter<P, File> {
    pub(super) fn open(file: File, context: P::Context) -> Self {
        Self::new(file, context)
    }
}

impl<P: SealedProfile, S: DiagnosticSink> DiagnosticWriter<P, S> {
    fn new(sink: S, context: P::Context) -> Self {
        Self {
            context,
            state: WriterState::Active(sink),
            next_sequence: 1,
            written: 0,
            _profile: std::marker::PhantomData,
        }
    }

    pub(super) fn emit(&mut self, event: P::Event) -> DiagnosticEmitStatus {
        match &self.state {
            WriterState::Active(_) => {}
            WriterState::Sealed(_) => return DiagnosticEmitStatus::Sealed,
            WriterState::Disabled => return DiagnosticEmitStatus::Disabled,
        }
        let metadata = WriterMetadata::next(self.next_sequence);
        let Some(line) = P::encode(metadata, &self.context, event).and_then(valid_line) else {
            self.state = WriterState::Disabled;
            return DiagnosticEmitStatus::Disabled;
        };
        if self.written.saturating_add(line.len()) > MAX_FILE_BYTES - TERMINAL_RESERVE_BYTES {
            return self.emit_terminal_capacity_marker();
        }
        self.write_line(&line, false)
    }

    /// Flushes and syncs once.  An observer failure remains nonfatal.
    pub(super) fn close(&mut self) -> DiagnosticCloseStatus {
        let state = std::mem::replace(&mut self.state, WriterState::Disabled);
        let (WriterState::Active(mut sink) | WriterState::Sealed(mut sink)) = state else {
            return DiagnosticCloseStatus::NotRequired;
        };
        let flushed = sink.flush().is_ok();
        let synced = sink.sync_all().is_ok();
        if flushed && synced {
            DiagnosticCloseStatus::Synced
        } else {
            DiagnosticCloseStatus::Failed
        }
    }

    fn emit_terminal_capacity_marker(&mut self) -> DiagnosticEmitStatus {
        let metadata = WriterMetadata::next(self.next_sequence);
        let Some(line) = P::encode_capacity(metadata, &self.context).and_then(valid_line) else {
            self.state = WriterState::Disabled;
            return DiagnosticEmitStatus::Disabled;
        };
        match self.write_line(&line, true) {
            DiagnosticEmitStatus::Written => {
                self.seal();
                DiagnosticEmitStatus::Sealed
            }
            status => status,
        }
    }

    fn seal(&mut self) {
        let state = std::mem::replace(&mut self.state, WriterState::Disabled);
        self.state = match state {
            WriterState::Active(sink) | WriterState::Sealed(sink) => WriterState::Sealed(sink),
            WriterState::Disabled => WriterState::Disabled,
        };
    }

    fn write_line(&mut self, line: &[u8], terminal: bool) -> DiagnosticEmitStatus {
        let limit = if terminal {
            MAX_FILE_BYTES
        } else {
            MAX_FILE_BYTES - TERMINAL_RESERVE_BYTES
        };
        if self.written.saturating_add(line.len()) > limit {
            self.state = WriterState::Disabled;
            return DiagnosticEmitStatus::Disabled;
        }
        let result = match &mut self.state {
            WriterState::Active(sink) => sink.write_all(line),
            WriterState::Sealed(_) => return DiagnosticEmitStatus::Sealed,
            WriterState::Disabled => return DiagnosticEmitStatus::Disabled,
        };
        if result.is_err() {
            self.state = WriterState::Disabled;
            return DiagnosticEmitStatus::Disabled;
        }
        self.written += line.len();
        if self.next_sequence == u64::MAX {
            self.seal();
            DiagnosticEmitStatus::Sealed
        } else {
            self.next_sequence += 1;
            DiagnosticEmitStatus::Written
        }
    }

    #[cfg(test)]
    pub(super) fn from_test_sink(sink: S, context: P::Context) -> Self {
        Self::new(sink, context)
    }

    #[cfg(test)]
    pub(super) fn state_is_sealed(&self) -> bool {
        matches!(&self.state, WriterState::Sealed(_))
    }
}

fn valid_line(mut encoded: Vec<u8>) -> Option<Vec<u8>> {
    if encoded.iter().any(|byte| matches!(*byte, b'\r' | b'\n')) {
        return None;
    }
    encoded.push(b'\n');
    (encoded.len() <= MAX_LINE_BYTES).then_some(encoded)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Write},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::{
        DiagnosticCloseStatus, DiagnosticEmitStatus, DiagnosticSink, DiagnosticWriter,
        MAX_FILE_BYTES, MAX_LINE_BYTES, SealedProfile, WriterMetadata, WriterState, sealed,
    };

    #[derive(Default)]
    struct TestSink {
        bytes: Vec<u8>,
        fail: bool,
        fail_flush: bool,
        fail_sync: bool,
        writes: Arc<AtomicUsize>,
        syncs: Arc<AtomicUsize>,
    }

    impl Write for TestSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(io::Error::other("scripted observer failure"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("scripted flush failure"))
            } else {
                Ok(())
            }
        }
    }

    impl DiagnosticSink for TestSink {
        fn sync_all(&mut self) -> io::Result<()> {
            self.syncs.fetch_add(1, Ordering::SeqCst);
            if self.fail_sync {
                Err(io::Error::other("scripted sync failure"))
            } else {
                Ok(())
            }
        }
    }

    enum TestEvent {
        Normal,
        EmbeddedNewline,
        Oversized,
    }

    struct TestProfile;

    impl sealed::Sealed for TestProfile {}

    impl SealedProfile for TestProfile {
        type Context = ();
        type Event = TestEvent;

        fn encode(
            metadata: WriterMetadata,
            _: &Self::Context,
            event: Self::Event,
        ) -> Option<Vec<u8>> {
            Some(match event {
                TestEvent::Normal => format!("event:{}", metadata.sequence).into_bytes(),
                TestEvent::EmbeddedNewline => b"bad\r\nrecord".to_vec(),
                TestEvent::Oversized => vec![b'x'; MAX_LINE_BYTES],
            })
        }

        fn encode_capacity(metadata: WriterMetadata, _: &Self::Context) -> Option<Vec<u8>> {
            Some(format!("capacity:{}", metadata.sequence).into_bytes())
        }
    }

    #[test]
    fn profile_boundary_rejects_embedded_newlines_and_oversize_records() {
        let mut newline =
            DiagnosticWriter::<TestProfile, _>::from_test_sink(TestSink::default(), ());
        assert_eq!(
            newline.emit(TestEvent::EmbeddedNewline),
            DiagnosticEmitStatus::Disabled
        );
        assert!(matches!(newline.state, WriterState::Disabled));

        let mut oversized =
            DiagnosticWriter::<TestProfile, _>::from_test_sink(TestSink::default(), ());
        assert_eq!(
            oversized.emit(TestEvent::Oversized),
            DiagnosticEmitStatus::Disabled
        );
        assert!(matches!(oversized.state, WriterState::Disabled));
    }

    #[test]
    fn sequence_cap_terminal_marker_and_sync_are_bounded() {
        let mut writer =
            DiagnosticWriter::<TestProfile, _>::from_test_sink(TestSink::default(), ());
        let mut terminal = DiagnosticEmitStatus::Written;
        while !writer.state_is_sealed() {
            terminal = writer.emit(TestEvent::Normal);
        }
        assert_eq!(terminal, DiagnosticEmitStatus::Sealed);
        let WriterState::Sealed(sink) = &writer.state else {
            panic!("capacity seals with retained sink");
        };
        assert!(sink.bytes.len() <= MAX_FILE_BYTES);
        assert!(sink.bytes.ends_with(b"\n"));
        let output = String::from_utf8_lossy(&sink.bytes);
        assert!(output.contains("event:1\n"));
        assert!(
            output
                .lines()
                .last()
                .is_some_and(|line| line.starts_with("capacity:"))
        );
        // The terminal marker has its newline after its sequence digits.
        assert!(sink.bytes.last() == Some(&b'\n'));
        let syncs = Arc::clone(&sink.syncs);
        assert_eq!(writer.close(), DiagnosticCloseStatus::Synced);
        assert_eq!(syncs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn close_reports_flush_or_sync_failure_and_never_retries() {
        let syncs = Arc::new(AtomicUsize::new(0));
        let mut writer = DiagnosticWriter::<TestProfile, _>::from_test_sink(
            TestSink {
                fail_flush: true,
                fail_sync: true,
                syncs: Arc::clone(&syncs),
                ..TestSink::default()
            },
            (),
        );
        assert_eq!(writer.close(), DiagnosticCloseStatus::Failed);
        assert_eq!(syncs.load(Ordering::SeqCst), 1);
        assert_eq!(writer.close(), DiagnosticCloseStatus::NotRequired);
        assert_eq!(syncs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn observer_failure_disables_without_retry_or_panic() {
        let writes = Arc::new(AtomicUsize::new(0));
        let mut writer = DiagnosticWriter::<TestProfile, _>::from_test_sink(
            TestSink {
                fail: true,
                writes: Arc::clone(&writes),
                ..TestSink::default()
            },
            (),
        );
        assert_eq!(
            writer.emit(TestEvent::Normal),
            DiagnosticEmitStatus::Disabled
        );
        assert_eq!(
            writer.emit(TestEvent::Normal),
            DiagnosticEmitStatus::Disabled
        );
        assert!(matches!(writer.state, WriterState::Disabled));
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sealed_writer_drops_post_terminal_events_without_reopening() {
        let mut writer =
            DiagnosticWriter::<TestProfile, _>::from_test_sink(TestSink::default(), ());
        while !writer.state_is_sealed() {
            let _ = writer.emit(TestEvent::Normal);
        }
        assert_eq!(writer.emit(TestEvent::Normal), DiagnosticEmitStatus::Sealed);
        assert!(writer.state_is_sealed());
    }
}
