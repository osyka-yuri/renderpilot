use std::io::{self, Cursor, Read};

use crate::portable_runtime::{
    app_protocol::PortableUpdateEvent,
    error::PortableRuntimeError,
    supervisor_updates::download::{DownloadStageError, read_limited_body_with_events},
};

struct OneByteReader {
    bytes: Vec<u8>,
    offset: usize,
}

impl Read for OneByteReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let Some(byte) = self.bytes.get(self.offset).copied() else {
            return Ok(0);
        };
        output[0] = byte;
        self.offset += 1;
        Ok(1)
    }
}

struct FailingReader {
    returned: bool,
}

impl Read for FailingReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.returned {
            return Err(io::Error::other("injected read failure"));
        }
        output[0] = 7;
        self.returned = true;
        Ok(1)
    }
}

#[test]
fn download_stream_coalesces_short_reads_at_the_exact_logical_boundary() {
    const UNIT: usize = 64 * 1024;
    let body = vec![42; 16 * 1024];
    let mut events = Vec::new();
    let bytes = read_limited_body_with_events(
        OneByteReader {
            bytes: body.clone(),
            offset: 0,
        },
        32 * 1024,
        Some(body.len() as u64),
        "portable_update_test",
        "too large",
        &mut |event| {
            events.push(event);
            Ok(())
        },
    )
    .expect("short positive reads must coalesce before the accepted EOF");
    assert_eq!(bytes, body);
    assert_eq!(
        events,
        vec![
            PortableUpdateEvent::download_started(Some(16 * 1024)),
            PortableUpdateEvent::download_progress(16 * 1024),
            PortableUpdateEvent::download_finished(),
        ]
    );

    let crossing_body = vec![17; UNIT + 5];
    let mut crossing_events = Vec::new();
    let crossing_bytes = read_limited_body_with_events(
        OneByteReader {
            bytes: crossing_body.clone(),
            offset: 0,
        },
        (2 * UNIT) as u64,
        Some(crossing_body.len() as u64),
        "portable_update_test",
        "too large",
        &mut |event| {
            crossing_events.push(event);
            Ok(())
        },
    )
    .expect("one-byte reads crossing 64KiB must emit the fixed logical unit at its threshold");
    assert_eq!(crossing_bytes, crossing_body);
    assert_eq!(
        crossing_events,
        vec![
            PortableUpdateEvent::download_started(Some((UNIT + 5) as u64)),
            PortableUpdateEvent::download_progress(UNIT as u64),
            PortableUpdateEvent::download_progress(5),
            PortableUpdateEvent::download_finished(),
        ],
        "64KiB is the exact coalescing threshold; only the accepted EOF remainder is partial"
    );
}

#[test]
fn download_stream_reports_one_truthful_unknown_length_sequence() {
    let mut unknown_length_events = Vec::new();
    read_limited_body_with_events(
        Cursor::new(b"abc"),
        5,
        None,
        "portable_update_test",
        "too large",
        &mut |event| {
            unknown_length_events.push(event);
            Ok(())
        },
    )
    .expect("an unknown-size accepted body still has one truthful terminal sequence");
    assert_eq!(
        unknown_length_events,
        vec![
            PortableUpdateEvent::download_started(None),
            PortableUpdateEvent::download_progress(3),
            PortableUpdateEvent::download_finished(),
        ]
    );
}

#[test]
fn failed_download_streams_never_flush_partial_progress_or_finish() {
    let mut known_oversize_events = Vec::new();
    assert!(
        read_limited_body_with_events(
            Cursor::new(b"abcdef"),
            5,
            Some(6),
            "portable_update_test",
            "too large",
            &mut |event| {
                known_oversize_events.push(event);
                Ok(())
            },
        )
        .is_err()
    );
    assert!(
        known_oversize_events.is_empty(),
        "a known oversize response rejects before Started"
    );

    for (body, known_length) in [(b"abcdef".as_slice(), None), (b"abc".as_slice(), Some(4))] {
        let mut failed_events = Vec::new();
        assert!(
            read_limited_body_with_events(
                Cursor::new(body),
                5,
                known_length,
                "portable_update_test",
                "too large",
                &mut |event| {
                    failed_events.push(event);
                    Ok(())
                },
            )
            .is_err()
        );
        assert_eq!(
            failed_events,
            vec![PortableUpdateEvent::download_started(known_length)],
            "overflow and length mismatches never flush a partial progress unit or Finished"
        );
    }

    let mut read_failure_events = Vec::new();
    assert!(
        read_limited_body_with_events(
            FailingReader { returned: false },
            5,
            None,
            "portable_update_test",
            "too large",
            &mut |event| {
                read_failure_events.push(event);
                Ok(())
            },
        )
        .is_err()
    );
    assert_eq!(
        read_failure_events,
        vec![PortableUpdateEvent::download_started(None)]
    );
}

#[test]
fn event_transport_failures_retain_their_error_classification() {
    let mut failed_delivery_count = 0;
    let error = read_limited_body_with_events(
        Cursor::new(b"x"),
        5,
        None,
        "portable_update_test",
        "too large",
        &mut |_| {
            failed_delivery_count += 1;
            Err(PortableRuntimeError::new(
                "portable_update_test_sink",
                "injected event delivery failure",
            ))
        },
    )
    .expect_err("event delivery failures stop the transaction before later events");
    let DownloadStageError::EventTransport(error) = error else {
        panic!("event delivery failure must retain its transport classification");
    };
    assert_eq!(error.code(), "portable_update_test_sink");
    assert_eq!(failed_delivery_count, 1);
}
