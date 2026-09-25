use std::io::Cursor;

use renderpilot_detection::pe::{PeSectionHeader, ScanTerminalCondition, scan_version_section_all};
use renderpilot_domain::{Architecture, PathRef};

use super::common::{write_synthetic_pe, write_synthetic_pe_advanced, write_synthetic_pe_with_pdb};
use crate::addons::game_analysis::budget::AnalysisBudget;
use crate::addons::game_analysis::context::GameInstallationContext;
use crate::addons::game_analysis::evidence_set::GameEvidenceSet;
use crate::addons::game_analysis::facade::{
    EngineDetection, UnrealDetection, UnrealPresenceProof, analyze_unreal_installation,
    generation_from_iostore_version, probe_iostore_installation, probe_ue3_installation,
};
use crate::addons::game_analysis::forensics::DetectionDiagnostic;
use crate::addons::game_analysis::parsers::metadata_files::{
    parse_build_version, parse_project_descriptor,
};
use crate::addons::game_analysis::parsers::release_markers::{
    BoundaryCheckResult, MarkerScanIncomplete, scan_chunk_for_release_markers,
    scan_installation_release_markers, validate_marker_boundaries, validate_wide_marker_boundaries,
};
use crate::addons::game_analysis::parsers::tokens::VersionClaim;
use crate::addons::game_analysis::topology::executable::{
    BoundEngineHelper, BoundPrimaryExecutable,
};
use crate::addons::game_analysis::topology::metadata::{
    BoundEngineMetadata, BoundProjectMetadata, MetadataReadError, read_capped_utf8,
};

/// Regression test 1: Overlap buffer boundary protection when match_pos == 0
#[test]
fn test_overlap_start_invalid_predecessor() {
    let chunk_offset = 65536u64;
    let section_start = 4096u64;
    let slice = b"++UE5+Release-5.4.3-CL-12345\0";
    let res = validate_marker_boundaries(slice, 0, 28, chunk_offset, section_start, true);
    assert_eq!(res, BoundaryCheckResult::SkipAlreadyScanned);

    let res_start = validate_marker_boundaries(slice, 0, 28, section_start, section_start, true);
    assert_eq!(res_start, BoundaryCheckResult::Valid);
}

/// Regression test 5: Transactional marker scan rollback on incomplete scan (BudgetExhausted)
#[test]
fn test_transactional_incomplete_scan_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let primary_path = bin_dir.join("TestGame.exe");

    let rdata_payload =
        b"PADDING_PRELUDE_BYTES_HERE_FOR_TESTING\0++UE5+Release-5.4.3-CL-12345\0TRAILING";
    write_synthetic_pe(&primary_path, &[(".rdata", 0x4000_0040, rdata_payload)]);

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };
    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();

    // Severely constrained budget forces exhaustion midway
    let mut budget = AnalysisBudget {
        section_stream_bytes_remaining: 10,
        ..Default::default()
    };
    let mut coverage = Vec::new();

    let scan_res = scan_installation_release_markers(
        &context,
        Some(&mut primary),
        &mut [],
        &mut budget,
        &mut coverage,
        false,
    );

    assert!(matches!(
        scan_res,
        Err(MarkerScanIncomplete::BudgetExhausted { scanned_bytes }) if scanned_bytes == 10
    ));
    assert_eq!(budget.section_stream_bytes_remaining, 0);
    assert!(!coverage.is_empty());
    assert_eq!(
        coverage[0].terminal_condition,
        ScanTerminalCondition::BudgetExhausted
    );
}

/// Regression test 7: Installation marker scan lifecycle via Result: atomic success and clean abort on error
#[test]
fn test_installation_marker_scan_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    let engine_bin_dir = root.join("Engine").join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&engine_bin_dir).unwrap();

    let primary_path = bin_dir.join("TestGame.exe");
    let helper_path = engine_bin_dir.join("CrashReportClient.exe");

    let marker_bytes = b"++UE5+Release-5.4.3-CL-12345\0";
    write_synthetic_pe(&primary_path, &[(".rdata", 0x4000_0040, marker_bytes)]);
    write_synthetic_pe(&helper_path, &[(".rdata", 0x4000_0040, marker_bytes)]);

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    // 1. Success on complete scan
    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
    let mut helper = BoundEngineHelper::open(&context, &helper_path, Architecture::X64).unwrap();
    let mut budget = AnalysisBudget::default();
    let mut coverage = Vec::new();

    let ok_res = scan_installation_release_markers(
        &context,
        Some(&mut primary),
        std::slice::from_mut(&mut helper),
        &mut budget,
        &mut coverage,
        false,
    );
    let markers = ok_res.expect("both targets should scan cleanly");
    assert_eq!(markers.len(), 2);

    let mut set = GameEvidenceSet::new(&context);
    for m in markers {
        set.insert(m).expect("context matches");
    }
    assert_eq!(set.evidences().len(), 2);

    // 2. Failure: helper budget exhausted rolls back staged primary markers
    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
    let mut helper = BoundEngineHelper::open(&context, &helper_path, Architecture::X64).unwrap();
    let mut budget = AnalysisBudget {
        section_stream_bytes_remaining: 35,
        ..Default::default()
    };
    let mut coverage = Vec::new();
    let fail_res = scan_installation_release_markers(
        &context,
        Some(&mut primary),
        std::slice::from_mut(&mut helper),
        &mut budget,
        &mut coverage,
        false,
    );
    assert!(matches!(
        fail_res,
        Err(MarkerScanIncomplete::BudgetExhausted { .. })
    ));

    // 3. Truncated helper file produces TruncatedFile error with helper path
    let trunc_helper_path = engine_bin_dir.join("TruncatedHelper.exe");
    let large_payload = vec![0x41u8; 1024];
    write_synthetic_pe_advanced(
        &trunc_helper_path,
        &[(".rdata", 0x4000_0040, &large_payload)],
        Some(600),
    );
    let mut trunc_helper =
        BoundEngineHelper::open(&context, &trunc_helper_path, Architecture::X64).unwrap();
    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
    let mut budget = AnalysisBudget::default();
    let mut coverage = Vec::new();
    let trunc_res = scan_installation_release_markers(
        &context,
        Some(&mut primary),
        std::slice::from_mut(&mut trunc_helper),
        &mut budget,
        &mut coverage,
        false,
    );
    match trunc_res {
        Err(MarkerScanIncomplete::TruncatedFile { path }) => {
            assert_eq!(path, std::fs::canonicalize(&trunc_helper_path).unwrap());
        }
        other => panic!("expected TruncatedFile, got: {:?}", other),
    }

    // 4. Foreign context yields ContextMismatch
    let temp2 = tempfile::tempdir().unwrap();
    let foreign_context = GameInstallationContext::new(temp2.path()).unwrap();
    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
    let mut budget = AnalysisBudget::default();
    let mut coverage = Vec::new();
    let mismatch_res = scan_installation_release_markers(
        &foreign_context,
        Some(&mut primary),
        &mut [],
        &mut budget,
        &mut coverage,
        false,
    );
    assert_eq!(mismatch_res, Err(MarkerScanIncomplete::ContextMismatch));
}

/// Regression test: Dynamically activated Phase 2 failure (budget exhaustion or truncation) rolls back staged Phase 1 markers
#[test]
fn test_dynamic_phase2_failure_rolls_back_staged_markers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let primary_path = bin_dir.join("TestGame.exe");
    let primary_rdata = b"++UE5+Release-5.4\0";
    let primary_text = b"NOPNOPNOPPADDINGPADDING_LONG_TEXT_PAYLOAD_FOR_PHASE_2";
    write_synthetic_pe(
        &primary_path,
        &[
            (".rdata", 0x4000_0040, primary_rdata),
            (".text", 0x6000_0020, primary_text),
        ],
    );

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    // Scenario A: Phase 1 succeeds in .rdata, dynamically activates Phase 2 on .text, but Phase 2 exhausts budget:
    // Confirmed Phase 1 evidence is preserved, while Phase 2 partial scan is safely discarded.
    {
        let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
        let rdata_raw_size = primary_rdata.len() as u64;
        let mut budget = AnalysisBudget {
            section_stream_bytes_remaining: rdata_raw_size + 5, // Enough for .rdata, but not for .text
            ..Default::default()
        };
        let mut coverage = Vec::new();

        // has_presence_proof = false: Phase 2 is NOT initially active,
        // but Phase 1 marker discovery dynamically triggers Phase 2!
        let scan_res = scan_installation_release_markers(
            &context,
            Some(&mut primary),
            &mut [],
            &mut budget,
            &mut coverage,
            false,
        );

        let markers =
            scan_res.expect("Phase 1 marker must be preserved when Phase 2 exhausts budget");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].claim().major(), 5);
        assert_eq!(markers[0].claim().minor(), Some(4));
        assert_eq!(markers[0].claim().patch(), None);
        assert_eq!(budget.section_stream_bytes_remaining, 0);
        let text_cov = coverage
            .iter()
            .find(|c| c.section_name == ".text")
            .expect(".text coverage must be present");
        assert_eq!(
            text_cov.terminal_condition,
            ScanTerminalCondition::BudgetExhausted
        );
    }

    // Scenario B: Phase 1 succeeds in .rdata, dynamically activates Phase 2 on .text, but Phase 2 encounters truncation
    {
        let trunc_path = bin_dir.join("TruncGame.exe");
        let large_text = vec![0x90u8; 1024];
        write_synthetic_pe_advanced(
            &trunc_path,
            &[
                (".rdata", 0x4000_0040, primary_rdata),
                (".text", 0x6000_0020, &large_text),
            ],
            Some(1200), // Truncate inside .text section (starts at offset 1024)
        );

        let trunc_resolved = crate::game_executable::ResolvedExecutable {
            path: PathRef::new(&*trunc_path.to_string_lossy()).unwrap(),
            file_name: "TruncGame.exe".to_string(),
            graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
            source: crate::game_executable::ExeSource::Auto,
        };

        let mut trunc_primary =
            BoundPrimaryExecutable::from_resolved(&context, &trunc_resolved).unwrap();
        let mut budget = AnalysisBudget::default();
        let mut coverage = Vec::new();

        let trunc_res = scan_installation_release_markers(
            &context,
            Some(&mut trunc_primary),
            &mut [],
            &mut budget,
            &mut coverage,
            false,
        );

        match trunc_res {
            Err(MarkerScanIncomplete::TruncatedFile { path }) => {
                assert_eq!(path, std::fs::canonicalize(&trunc_path).unwrap());
            }
            other => panic!("expected TruncatedFile from Phase 2, got: {:?}", other),
        }
    }
}

/// Regression test 8: Bounded metadata reading with BOM stripping and size limits
#[test]
fn test_metadata_capped_read_with_bom_stripping_and_limits() {
    // 1. Successful read with UTF-8 BOM
    let mut bom_bytes = b"\xEF\xBB\xBF".to_vec();
    bom_bytes.extend_from_slice(br#"{"EngineAssociation":"5.4"}"#);
    let mut with_bom = Cursor::new(bom_bytes);
    let read_bom = read_capped_utf8(&mut with_bom, 64 * 1024).expect("must parse with BOM");
    assert_eq!(read_bom, r#"{"EngineAssociation":"5.4"}"#);

    // 2. Successful clean read without BOM
    let mut without_bom = Cursor::new(br#"{"EngineAssociation":"5.4"}"#.to_vec());
    let read_clean = read_capped_utf8(&mut without_bom, 64 * 1024).expect("must parse clean");
    assert_eq!(read_clean, r#"{"EngineAssociation":"5.4"}"#);

    // 3. Rejection when byte limit is exceeded (MetadataReadError::TooLarge)
    let large_payload = vec![b'a'; 100];
    let mut large_cursor = Cursor::new(large_payload);
    assert_eq!(
        read_capped_utf8(&mut large_cursor, 50),
        Err(MetadataReadError::TooLarge {
            actual_bytes: 51,
            max_bytes: 50
        })
    );

    // 4. Rejection on invalid UTF-8 payload
    let mut invalid_utf8 = Cursor::new(vec![0xFF, 0xFE, 0xFD]);
    assert_eq!(
        read_capped_utf8(&mut invalid_utf8, 64 * 1024),
        Err(MetadataReadError::InvalidUtf8)
    );
}

/// Regression test 9: Phase 2 (.text) scanning triggers when presence proof is established
#[test]
fn test_phase2_text_scanning_triggers_on_presence_proof() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let primary_path = bin_dir.join("TestGame.exe");

    let rdata_bytes = b"Just some normal read-only strings\0";
    let text_bytes = b"\x90\x90++UE5+Release-5.4.3-CL-12345\0\x90\x90";
    write_synthetic_pe(
        &primary_path,
        &[
            (".rdata", 0x4000_0040, rdata_bytes),
            (".text", 0x6000_0020, text_bytes),
        ],
    );

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    // Case A: has_presence_proof = false -> Phase 2 (.text) is NOT scanned
    {
        let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
        let mut budget = AnalysisBudget::default();
        let mut coverage = Vec::new();
        let res = scan_installation_release_markers(
            &context,
            Some(&mut primary),
            &mut [],
            &mut budget,
            &mut coverage,
            false,
        )
        .unwrap();
        assert!(
            res.is_empty(),
            ".text must not be scanned without presence proof when .rdata has no markers"
        );
        assert!(
            coverage.iter().all(|c| c.section_name != ".text"),
            ".text section must not appear in coverage"
        );
    }

    // Case B: has_presence_proof = true -> Phase 2 (.text) is scanned and discovers the marker
    {
        let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
        let mut budget = AnalysisBudget::default();
        let mut coverage = Vec::new();
        let res = scan_installation_release_markers(
            &context,
            Some(&mut primary),
            &mut [],
            &mut budget,
            &mut coverage,
            true,
        )
        .unwrap();
        assert_eq!(
            res.len(),
            1,
            ".text must be scanned when presence proof is established"
        );
        assert_eq!(res[0].claim().major(), 5);
        assert_eq!(res[0].claim().minor(), Some(4));
        assert_eq!(res[0].claim().patch(), Some(3));
        assert!(
            coverage.iter().any(|c| c.section_name == ".text"),
            ".text section must appear in coverage"
        );
    }
}

/// Regression test 10: Strict release marker grammar rejects malformed candidate strings
#[test]
fn test_release_marker_grammar_rejects_malformed_tokens() {
    // Rejected malformed tokens
    let invalid_cases = [
        b"++UE5+Release-5.4.123-CL-12345\0".as_slice(), // 3-digit patch
        b"++UE5+Release-5.4.-CL-12345\0".as_slice(),    // dot without patch digits
        b"++UE5+Release-5.4.CL-12345\0".as_slice(),     // dot followed immediately by CL
        b"++UE5+Release-5.4-CL-\0".as_slice(),          // -CL- without digits
        b"++UE5+Release-5.4-CL-abc\0".as_slice(),       // -CL- followed by non-digits
        b"++UE5+Release-5.4.3-CL-\0".as_slice(),        // patch followed by empty CL
        b"++UE5+Release-5.4.3-CL-xyz\0".as_slice(),     // patch followed by non-digit CL
        b"++UE5+Release-5.4.999\0".as_slice(),          // 3-digit patch
        b"++UE5+Release-5.\0".as_slice(),               // missing minor
        b"++UE5+Release-\0".as_slice(),                 // missing minor
        b"++UE6+Release-6.0.0\0".as_slice(),            // unsupported generation 6 prefix
    ];

    for &case in &invalid_cases {
        let mut results = Vec::new();
        scan_chunk_for_release_markers(case, 0, 0, true, &mut results);
        assert!(
            results.is_empty(),
            "Malformed token {:?} must be rejected, but yielded: {:?}",
            std::str::from_utf8(case),
            results
        );
    }

    // Accepted valid tokens
    let valid_cases: [(&[u8], u32, u32, Option<u32>); 6] = [
        (b"++UE5+Release-5.4.3-CL-12345\0".as_slice(), 5, 4, Some(3)),
        (b"++UE5+Release-5.4-CL-12345\0".as_slice(), 5, 4, None),
        (b"++UE5+Release-5.4\0".as_slice(), 5, 4, None),
        (b"++UE4+Release-4.27.2-CL-0\0".as_slice(), 4, 27, Some(2)),
        (b"++UE5+Release-5.0.0-CL-100\0".as_slice(), 5, 0, Some(0)),
        (b"++UE5+Release-5.4.99-CL-9999\0".as_slice(), 5, 4, Some(99)),
    ];

    for &(case, expected_maj, expected_min, expected_pat) in &valid_cases {
        let mut results = Vec::new();
        scan_chunk_for_release_markers(case, 0, 0, true, &mut results);
        assert_eq!(
            results.len(),
            1,
            "Valid token {:?} must yield exactly 1 result",
            std::str::from_utf8(case)
        );
        assert_eq!(results[0].major, expected_maj);
        assert_eq!(results[0].minor, expected_min);
        assert_eq!(results[0].patch, expected_pat);
    }
}

fn to_utf16le_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// Regression test 10b: UTF-16LE canonical release markers are parsed accurately
#[test]
fn test_utf16le_release_marker_valid_tokens() {
    let valid_cases: [(&str, u32, u32, Option<u32>); 6] = [
        ("++UE4+Release-4.23\0", 4, 23, None),
        ("++UE4+Release-4.23-CL-0\0", 4, 23, None),
        ("++UE4+Release-4.27.2-CL-0\0", 4, 27, Some(2)),
        ("++UE5+Release-5.4.3-CL-12345\0", 5, 4, Some(3)),
        ("++UE5+Release-5.4-CL-12345\0", 5, 4, None),
        ("++UE5+Release-5.0.0-CL-100\0", 5, 0, Some(0)),
    ];

    for &(case, expected_maj, expected_min, expected_pat) in &valid_cases {
        let wide_bytes = to_utf16le_bytes(case);
        let mut results = Vec::new();
        scan_chunk_for_release_markers(&wide_bytes, 0, 0, true, &mut results);
        assert_eq!(
            results.len(),
            1,
            "Valid UTF-16LE token {:?} must yield exactly 1 result",
            case
        );
        assert_eq!(results[0].major, expected_maj);
        assert_eq!(results[0].minor, expected_min);
        assert_eq!(results[0].patch, expected_pat);
        let expected_canonical = case.trim_end_matches('\0');
        assert_eq!(
            results[0].raw,
            crate::addons::game_analysis::parsers::tokens::BoundedMarker::try_from_bytes(
                expected_canonical.as_bytes()
            )
            .unwrap()
        );
    }
}

/// Regression test 10c: UTF-16LE canonical release markers reject malformed tokens identically to ASCII
#[test]
fn test_utf16le_release_marker_grammar_rejects_malformed() {
    let invalid_cases = [
        "++UE5+Release-5.4.123-CL-12345\0", // 3-digit patch
        "++UE5+Release-5.4.-CL-12345\0",    // dot without patch digits
        "++UE5+Release-5.4.CL-12345\0",     // dot followed immediately by CL
        "++UE5+Release-5.4-CL-\0",          // -CL- without digits
        "++UE5+Release-5.4-CL-abc\0",       // -CL- followed by non-digits
        "++UE5+Release-5.4.3-CL-\0",        // patch followed by empty CL
        "++UE5+Release-5.4.3-CL-xyz\0",     // patch followed by non-digit CL
        "++UE5+Release-5.4.999\0",          // 3-digit patch
        "++UE5+Release-5.\0",               // missing minor
        "++UE5+Release-\0",                 // missing minor
        "++UE6+Release-6.0.0\0",            // unsupported generation 6 prefix
    ];

    for &case in &invalid_cases {
        let wide_bytes = to_utf16le_bytes(case);
        let mut results = Vec::new();
        scan_chunk_for_release_markers(&wide_bytes, 0, 0, true, &mut results);
        assert!(
            results.is_empty(),
            "Malformed UTF-16LE token {:?} must be rejected, but yielded: {:?}",
            case,
            results
        );
    }
}

/// Regression test 10d: UTF-16LE word boundary validation
#[test]
fn test_utf16le_release_marker_word_boundaries() {
    let marker = "++UE4+Release-4.23";
    let wide_marker = to_utf16le_bytes(marker);
    let marker_len = wide_marker.len(); // 36 bytes

    // 1. Valid: surrounded by null terminators \0\0
    let mut buf = to_utf16le_bytes("\0");
    buf.extend_from_slice(&wide_marker);
    buf.extend_from_slice(&to_utf16le_bytes("\0"));
    assert_eq!(
        validate_wide_marker_boundaries(&buf, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::Valid
    );

    // 2. Valid: surrounded by quotes "\""
    let mut buf_quotes = to_utf16le_bytes("\"");
    buf_quotes.extend_from_slice(&wide_marker);
    buf_quotes.extend_from_slice(&to_utf16le_bytes("\""));
    assert_eq!(
        validate_wide_marker_boundaries(&buf_quotes, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::Valid
    );

    // 3. Invalid left boundary: preceded by alphanumeric 'A'
    let mut buf_left_invalid = to_utf16le_bytes("A");
    buf_left_invalid.extend_from_slice(&wide_marker);
    buf_left_invalid.extend_from_slice(&to_utf16le_bytes("\0"));
    assert_eq!(
        validate_wide_marker_boundaries(&buf_left_invalid, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::InvalidLeftBoundary
    );

    // 4. Invalid right boundary: followed by alphanumeric '0'
    let mut buf_right_invalid = to_utf16le_bytes("\0");
    buf_right_invalid.extend_from_slice(&wide_marker);
    buf_right_invalid.extend_from_slice(&to_utf16le_bytes("0"));
    assert_eq!(
        validate_wide_marker_boundaries(&buf_right_invalid, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::InvalidRightBoundary
    );

    // 5. Symmetric fail-closed check: non-ASCII code unit (high != 0) rejected on both sides
    let mut buf_left_non_ascii = vec![0x00, 0x01]; // U+0100
    buf_left_non_ascii.extend_from_slice(&wide_marker);
    buf_left_non_ascii.extend_from_slice(&to_utf16le_bytes("\0"));
    assert_eq!(
        validate_wide_marker_boundaries(&buf_left_non_ascii, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::InvalidLeftBoundary
    );

    let mut buf_right_non_ascii = to_utf16le_bytes("\0");
    buf_right_non_ascii.extend_from_slice(&wide_marker);
    buf_right_non_ascii.extend_from_slice(&[0x00, 0x01]); // U+0100
    assert_eq!(
        validate_wide_marker_boundaries(&buf_right_non_ascii, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::InvalidRightBoundary
    );

    // 6. Section start boundary alignment:
    // match_pos == 1 is an unaligned odd byte before the wide marker -> InvalidLeftBoundary
    let mut buf_left_odd = vec![0x00];
    buf_left_odd.extend_from_slice(&wide_marker);
    buf_left_odd.extend_from_slice(&to_utf16le_bytes("\0"));
    assert_eq!(
        validate_wide_marker_boundaries(&buf_left_odd, 1, marker_len, 0, 0, true),
        BoundaryCheckResult::InvalidLeftBoundary
    );

    // match_pos == 0 at section start is valid
    let mut buf_exact_start = wide_marker.clone();
    buf_exact_start.extend_from_slice(&to_utf16le_bytes("\0"));
    assert_eq!(
        validate_wide_marker_boundaries(&buf_exact_start, 0, marker_len, 0, 0, true),
        BoundaryCheckResult::Valid
    );

    // 7. Right boundary at section EOF vs non-final chunk:
    // Succeeded by an odd byte at EOF -> InvalidRightBoundary
    let mut buf_odd_eof = to_utf16le_bytes("\0");
    buf_odd_eof.extend_from_slice(&wide_marker);
    buf_odd_eof.push(0x00);
    assert_eq!(
        validate_wide_marker_boundaries(&buf_odd_eof, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::InvalidRightBoundary
    );
    // Succeeded by an odd byte in non-final chunk -> DeferredToNextChunk
    assert_eq!(
        validate_wide_marker_boundaries(&buf_odd_eof, 2, marker_len, 0, 0, false),
        BoundaryCheckResult::DeferredToNextChunk
    );

    // Exactly at section EOF -> Valid
    let mut buf_exact_eof = to_utf16le_bytes("\0");
    buf_exact_eof.extend_from_slice(&wide_marker);
    assert_eq!(
        validate_wide_marker_boundaries(&buf_exact_eof, 2, marker_len, 0, 0, true),
        BoundaryCheckResult::Valid
    );
    // Exactly at chunk boundary in non-final chunk -> DeferredToNextChunk
    assert_eq!(
        validate_wide_marker_boundaries(&buf_exact_eof, 2, marker_len, 0, 0, false),
        BoundaryCheckResult::DeferredToNextChunk
    );

    // 8. Overlap buffer start: match_pos < 2 in subsequent chunk -> SkipAlreadyScanned
    assert_eq!(
        validate_wide_marker_boundaries(&wide_marker, 0, marker_len, 65536, 4096, true),
        BoundaryCheckResult::SkipAlreadyScanned
    );
    assert_eq!(
        validate_wide_marker_boundaries(&wide_marker, 1, marker_len, 65536, 4096, true),
        BoundaryCheckResult::SkipAlreadyScanned
    );
}

/// Regression test 10e: Streaming chunk overlap correctly detects UTF-16LE canonical release markers crossing chunk boundary
#[test]
fn test_utf16le_release_marker_streaming_chunk_overlap_split() {
    let marker_str = "++UE4+Release-4.23\0";
    let wide_marker = to_utf16le_bytes(marker_str);
    let marker_len = wide_marker.len(); // 38 bytes

    let chunk_size = 64 * 1024; // 65536 bytes (STREAM_CHUNK_SIZE)
    let total_size = 128 * 1024; // 131072 bytes (2 chunks)
    let sec_offset = 0x1000u64; // 4096

    // Parameterize across all split positions where the marker crosses the 64 KiB chunk boundary
    for split_pos in 0..marker_len {
        let marker_file_offset = sec_offset + (chunk_size as u64) - (split_pos as u64);

        // Construct section buffer
        let mut section_bytes = vec![0u8; total_size];
        let local_offset = chunk_size - split_pos;
        section_bytes[local_offset..local_offset + marker_len].copy_from_slice(&wide_marker);

        // Prepend padding to simulate file offset before section
        let mut file_bytes = vec![0u8; sec_offset as usize];
        file_bytes.extend_from_slice(&section_bytes);

        let mut cursor = Cursor::new(file_bytes);
        let sec = PeSectionHeader {
            name: ".rdata".to_string(),
            virtual_size: total_size as u32,
            virtual_address: 0x2000,
            size_of_raw_data: total_size as u32,
            pointer_to_raw_data: sec_offset as u32,
            characteristics: 0x4000_0040,
        };

        let mut budget = u64::MAX;
        let mut results = Vec::new();
        let cov =
            scan_version_section_all(&mut cursor, &sec, &mut budget, |chunk, offset, is_final| {
                scan_chunk_for_release_markers(chunk, offset, sec_offset, is_final, &mut results);
            })
            .expect("streaming scan must succeed");

        assert_eq!(
            cov.terminal_condition,
            ScanTerminalCondition::Completed,
            "Scan must complete for split_pos = {split_pos}"
        );
        assert_eq!(
            results.len(),
            1,
            "Marker must be found exactly once when split at position {split_pos}, but found {}",
            results.len()
        );
        assert_eq!(
            results[0].file_offset, marker_file_offset,
            "file_offset mismatch for split_pos = {split_pos}"
        );
        assert_eq!(results[0].major, 4);
        assert_eq!(results[0].minor, 23);
        assert_eq!(results[0].patch, None);
        assert_eq!(
            results[0].raw,
            crate::addons::game_analysis::parsers::tokens::BoundedMarker::try_from_bytes(
                b"++UE4+Release-4.23"
            )
            .unwrap()
        );
    }
}

/// Regression test 11: .uproject GUID/custom branch association establishes presence proof without version claim
#[test]
fn test_uproject_guid_establishes_presence_proof_without_version() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    std::fs::create_dir_all(&root).unwrap();
    let context = GameInstallationContext::new(&root).unwrap();

    // 1. GUID EngineAssociation
    let guid_uproject = root.join("GameGuid.uproject");
    std::fs::write(
        &guid_uproject,
        br#"{"EngineAssociation": "{12345678-ABCD-1234-ABCD-1234567890AB}"}"#,
    )
    .unwrap();
    let mut bound_guid = BoundProjectMetadata::open(&context, &guid_uproject).unwrap();
    let outcome_guid = parse_project_descriptor(&mut bound_guid).unwrap();
    assert!(outcome_guid.is_valid_descriptor);
    assert!(outcome_guid.token.is_none());

    // 2. Custom branch EngineAssociation
    let custom_uproject = root.join("GameCustom.uproject");
    std::fs::write(
        &custom_uproject,
        br#"{"EngineAssociation": "Custom-UE5-Fork"}"#,
    )
    .unwrap();
    let mut bound_custom = BoundProjectMetadata::open(&context, &custom_uproject).unwrap();
    let outcome_custom = parse_project_descriptor(&mut bound_custom).unwrap();
    assert!(outcome_custom.is_valid_descriptor);
    assert!(outcome_custom.token.is_none());

    // 3. Blank or whitespace association is invalid
    let blank_uproject = root.join("GameBlank.uproject");
    std::fs::write(&blank_uproject, br#"{"EngineAssociation": "   "}"#).unwrap();
    let mut bound_blank = BoundProjectMetadata::open(&context, &blank_uproject).unwrap();
    let outcome_blank = parse_project_descriptor(&mut bound_blank).unwrap();
    assert!(!outcome_blank.is_valid_descriptor);
    assert!(outcome_blank.token.is_none());

    // 4. Missing EngineAssociation is invalid
    let missing_uproject = root.join("GameMissing.uproject");
    std::fs::write(&missing_uproject, br#"{"Modules": []}"#).unwrap();
    let mut bound_missing = BoundProjectMetadata::open(&context, &missing_uproject).unwrap();
    let outcome_missing = parse_project_descriptor(&mut bound_missing).unwrap();
    assert!(!outcome_missing.is_valid_descriptor);
    assert!(outcome_missing.token.is_none());

    // 5. Facade integration: GUID uproject establishes Unreal presence proof
    //    and produces Unreal(Unknown), NOT UnknownEngine!
    let bin_dir = root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let primary_path = bin_dir.join("TestGame.exe");
    write_synthetic_pe(&primary_path, &[(".rdata", 0x4000_0040, b"nothing here")]);

    let _ = std::fs::remove_file(&custom_uproject);
    let _ = std::fs::remove_file(&blank_uproject);
    let _ = std::fs::remove_file(&missing_uproject);

    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    let mut budget = AnalysisBudget::default();
    let report = analyze_unreal_installation(&context, Some(&resolved), &mut budget);

    assert!(
        report
            .presence_proofs
            .iter()
            .any(|p| matches!(p, UnrealPresenceProof::ProjectDescriptor { .. })),
        "Must have ProjectDescriptor presence proof"
    );
    assert_eq!(
        report.engine,
        EngineDetection::Unreal(UnrealDetection::Unknown),
        "GUID association must yield Unreal(Unknown)"
    );
}

/// Regression test 13: Build.version schema validation (unbounded PatchVersion, strict types)
#[test]
fn test_build_version_schema_validation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let build_dir = root.join("Engine").join("Build");
    std::fs::create_dir_all(&build_dir).unwrap();
    let context = GameInstallationContext::new(&root).unwrap();
    let build_version_path = build_dir.join("Build.version");

    // 1. Valid Build.version with unbounded PatchVersion (e.g. 1050)
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "PatchVersion": 1050,
            "Changelist": 1234567,
            "BranchName": "++UE5+Release-5.4"
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    let token = parse_build_version(&mut bound)
        .unwrap()
        .expect("must parse");
    let (_, _, claim) = token.into_parts();
    assert_eq!(
        claim,
        VersionClaim::Exact {
            major: 5,
            minor: 4,
            patch: 1050
        }
    );

    // 2. Changelist as string fails schema validation
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "PatchVersion": 0,
            "Changelist": "1234567",
            "BranchName": "++UE5+Release-5.4"
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    assert!(parse_build_version(&mut bound).unwrap().is_none());

    // 3. Changelist as negative integer fails
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "PatchVersion": 0,
            "Changelist": -1,
            "BranchName": "++UE5+Release-5.4"
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    assert!(parse_build_version(&mut bound).unwrap().is_none());

    // 4. BranchName as non-string fails schema validation
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "PatchVersion": 0,
            "Changelist": 123456,
            "BranchName": 12345
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    assert!(parse_build_version(&mut bound).unwrap().is_none());

    // 5. Explicit null on PatchVersion fails schema validation
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "PatchVersion": null
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    assert!(parse_build_version(&mut bound).unwrap().is_none());

    // 6. Explicit null on Changelist fails schema validation
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "Changelist": null
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    assert!(parse_build_version(&mut bound).unwrap().is_none());

    // 7. Explicit null on BranchName fails schema validation
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4,
            "BranchName": null
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    assert!(parse_build_version(&mut bound).unwrap().is_none());

    // 8. Omitted optional fields are valid
    std::fs::write(
        &build_version_path,
        br#"{
            "MajorVersion": 5,
            "MinorVersion": 4
        }"#,
    )
    .unwrap();
    let mut bound = BoundEngineMetadata::open(&context, &build_version_path).unwrap();
    let token = parse_build_version(&mut bound)
        .unwrap()
        .expect("must parse with omitted optional fields");
    let (_, _, claim) = token.into_parts();
    assert_eq!(claim, VersionClaim::MajorMinor { major: 5, minor: 4 });
}

/// Regression test 16: Installation-wide two-pass marker scan triggers Phase 2 across executables
#[test]
fn test_installation_wide_two_pass_marker_scan_cross_trigger() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    let helper_bin_dir = root.join("Engine").join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&helper_bin_dir).unwrap();

    // Primary only has marker in .rdata (Phase 1 discovery)
    let primary_path = bin_dir.join("TestGame.exe");
    write_synthetic_pe(
        &primary_path,
        &[(".rdata", 0x4000_0040, b"++UE5+Release-5.4\0")],
    );

    // Helper only has marker in .text (requires Phase 2 discovery!)
    let helper_path = helper_bin_dir.join("CrashReportClient.exe");
    write_synthetic_pe(
        &helper_path,
        &[(".text", 0x6000_0020, b"++UE4+Release-4.27\0")],
    );

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
    let mut helper = BoundEngineHelper::open(&context, &helper_path, Architecture::X64).unwrap();
    let mut budget = AnalysisBudget::default();
    let mut coverage = Vec::new();

    // Initially has_presence_proof = false
    let staged = scan_installation_release_markers(
        &context,
        Some(&mut primary),
        std::slice::from_mut(&mut helper),
        &mut budget,
        &mut coverage,
        false,
    )
    .expect("installation scan must succeed");

    // Primary's Phase 1 marker MUST trigger Phase 2 for Helper!
    // Staged must contain both Primary (5.4) and Helper (4.27)
    assert_eq!(
        staged.len(),
        2,
        "Both primary and helper markers must be collected"
    );
    let has_ue5 = staged
        .iter()
        .any(|e| e.claim().major() == 5 && e.claim().minor() == Some(4));
    let has_ue4 = staged
        .iter()
        .any(|e| e.claim().major() == 4 && e.claim().minor() == Some(27));
    assert!(has_ue5, "UE5.4 from Primary .rdata must be present");
    assert!(
        has_ue4,
        "UE4.27 from Helper .text must be discovered via installation-wide Phase 2 activation"
    );
}

/// Regression test 17: Scanner scans multiple candidate executable sections without name-based deduplication
#[test]
fn test_scanner_scans_multiple_text_sections_without_name_dedup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();

    // Write synthetic PE with TWO .text sections having the same name but different offsets and conflicting markers
    let primary_path = bin_dir.join("TestGame.exe");
    write_synthetic_pe(
        &primary_path,
        &[
            (".text", 0x6000_0020, b"++UE5+Release-5.4\0"),
            (".text", 0x6000_0020, b"++UE4+Release-4.27\0"),
        ],
    );

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
    let mut budget = AnalysisBudget::default();
    let mut coverage = Vec::new();

    let markers = scan_installation_release_markers(
        &context,
        Some(&mut primary),
        &mut [],
        &mut budget,
        &mut coverage,
        true, // Presence proof triggers Phase 2
    )
    .expect("marker scan must succeed");

    // Without name-based deduplication, BOTH .text sections are scanned
    assert_eq!(
        markers.len(),
        2,
        "Both .text sections must be scanned without name deduplication"
    );
    assert!(
        markers
            .iter()
            .any(|m| m.claim().major() == 5 && m.claim().minor() == Some(4))
    );
    assert!(
        markers
            .iter()
            .any(|m| m.claim().major() == 4 && m.claim().minor() == Some(27))
    );
}

/// Regression test 18: Strict Helper CodeView PDB establishes CanonicalPdb presence proof
#[test]
fn test_helper_codeview_pdb_establishes_canonical_pdb_presence_proof() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    let helper_bin_dir = root.join("Engine").join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&helper_bin_dir).unwrap();

    // Primary PE has no PDB and no release markers
    let primary_path = bin_dir.join("TestGame.exe");
    write_synthetic_pe(
        &primary_path,
        &[(".rdata", 0x4000_0040, b"DUMMY_NO_SIGNATURES")],
    );

    // Helper PE has CodeView PDB entry pointing to CrashReportClient-Win64-Shipping-UE_5.4.pdb
    let helper_path = helper_bin_dir.join("CrashReportClient.exe");
    write_synthetic_pe_with_pdb(
        &helper_path,
        "D:\\Build\\CrashReportClient-Win64-Shipping-UE_5.4.pdb",
    );

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    let mut budget = AnalysisBudget::default();
    let report = analyze_unreal_installation(&context, Some(&resolved), &mut budget);

    // Presence proof must contain CanonicalPdb from helper
    let has_helper_pdb_proof = report.presence_proofs.iter().any(|p| {
        matches!(
            p,
            UnrealPresenceProof::CanonicalPdb { pdb_name, .. }
            if pdb_name == "UE_5.4"
        )
    });
    assert!(
        has_helper_pdb_proof,
        "Helper strict PDB must produce CanonicalPdb presence proof"
    );
}

/// Regression test 19: UE3 package probe canonical containment and normalized lexical sort
#[test]
fn test_ue3_probe_lexical_sort_and_canonical_containment() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestUE3Game");
    let bin_dir = root.join("Binaries").join("Win32");
    let cooked_pc = root.join("Game").join("CookedPC");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&cooked_pc).unwrap();

    let primary_path = bin_dir.join("TestUE3Game.exe");
    write_synthetic_pe(&primary_path, &[(".rdata", 0x4000_0040, b"NO_MARKERS")]);

    // Helper function to write synthetic UE3 package
    let write_ue3_pkg = |path: &std::path::Path, version: u16| {
        let mut buf = vec![0u8; 64];
        buf[0..4].copy_from_slice(&0x9E2A_83C1u32.to_le_bytes());
        buf[4..6].copy_from_slice(&version.to_le_bytes());
        buf[6..8].copy_from_slice(&0u16.to_le_bytes());
        buf[8..12].copy_from_slice(&32i32.to_le_bytes());
        std::fs::write(path, buf).unwrap();
    };

    // Outside directory hosting an escaped package
    let outside_dir = temp.path().join("OutsideCooked");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let outside_pkg = outside_dir.join("Outside.upk");
    write_ue3_pkg(&outside_pkg, 800);

    // Symlink 00_OutsideLink.upk inside CookedPC pointing outside installation root.
    // If canonical containment fails, 00_OutsideLink.upk sorts first and claims version 800.
    // With canonical containment active, it is strictly rejected and A_Package (867) is chosen.
    let symlink_path = cooked_pc.join("00_OutsideLink.upk");
    #[cfg(windows)]
    let symlink_created = match std::os::windows::fs::symlink_file(&outside_pkg, &symlink_path) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("symlink containment subcase skipped: {error}");
            false
        }
    };
    #[cfg(unix)]
    let symlink_created = {
        std::os::unix::fs::symlink(&outside_pkg, &symlink_path)
            .expect("create escaped-package symlink");
        true
    };

    // B_Package has version 868, A_Package has version 867
    // Lexical sorting by relative path ensures A_Package is probed before B_Package
    write_ue3_pkg(&cooked_pc.join("B_Package.upk"), 868);
    write_ue3_pkg(&cooked_pc.join("A_Package.upk"), 867);

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestUE3Game.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X86)),
        source: crate::game_executable::ExeSource::Auto,
    };

    let mut budget = AnalysisBudget::default();
    let report = analyze_unreal_installation(&context, Some(&resolved), &mut budget);

    assert!(matches!(
        report.engine,
        EngineDetection::Unreal(UnrealDetection::Generation { major: 3 })
    ));

    // Must NOT match the escaped package version 800
    let has_outside_proof = report.presence_proofs.iter().any(|p| {
        matches!(
            p,
            UnrealPresenceProof::Ue3Package {
                file_version: 800,
                ..
            }
        )
    });
    if symlink_created {
        assert!(
            !has_outside_proof,
            "Escaped package outside root must never establish presence proof"
        );
    }

    let has_ue3_proof = report.presence_proofs.iter().any(|p| {
        matches!(
            p,
            UnrealPresenceProof::Ue3Package {
                file_version: 867,
                ..
            }
        )
    });
    assert!(
        has_ue3_proof,
        "Lexicographical ordering must select A_Package (version 867) first"
    );

    // Direct invariant assertion for canonical containment boundary
    let outside_canonical = std::fs::canonicalize(&outside_pkg).unwrap();
    assert!(
        !outside_canonical.starts_with(context.root_path()),
        "Outside canonical package must not be contained in installation context root"
    );
    let inside_canonical = std::fs::canonicalize(cooked_pc.join("A_Package.upk")).unwrap();
    assert!(
        inside_canonical.starts_with(context.root_path()),
        "Inside canonical package must be contained in installation context root"
    );
}

/// Regression test: Duplicate search directory traversal does not starve unique UE3 candidates
#[test]
fn test_ue3_probe_duplicate_search_dirs_does_not_starve_unique_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestUE3DupGame");
    let bin_dir = root.join("Binaries").join("Win32");
    // Engine/CookedPC is added explicitly AND through root directory traversal
    let engine_cooked = root.join("Engine").join("CookedPC");
    let game_cooked = root.join("Game").join("CookedPC");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&engine_cooked).unwrap();
    std::fs::create_dir_all(&game_cooked).unwrap();

    let primary_path = bin_dir.join("TestGame.exe");
    write_synthetic_pe(&primary_path, &[(".rdata", 0x4000_0040, b"NO_MARKERS")]);

    let write_ue3_pkg = |path: &std::path::Path, version: u16| {
        let mut buf = vec![0u8; 64];
        buf[0..4].copy_from_slice(&0x9E2A_83C1u32.to_le_bytes());
        buf[4..6].copy_from_slice(&version.to_le_bytes());
        buf[6..8].copy_from_slice(&0u16.to_le_bytes());
        buf[8..12].copy_from_slice(&32i32.to_le_bytes());
        std::fs::write(path, buf).unwrap();
    };

    // 6 packages in Engine/CookedPC: A..F (invalid version 0)
    // In buggy version, Engine/CookedPC traversed twice put A..E twice, filling top 10 with A..E!
    // F..J were starved and dropped before dedup.
    for name in ["A", "B", "C", "D", "E", "F"] {
        write_ue3_pkg(&engine_cooked.join(format!("{name}.upk")), 0);
    }
    // 4 packages in Game/CookedPC: G..I are invalid (version 0), J is the ONLY valid package (version 867).
    // If duplicate search dirs starve F..J, J is never visited and probe fails.
    for name in ["G", "H", "I"] {
        write_ue3_pkg(&game_cooked.join(format!("{name}.upk")), 0);
    }
    write_ue3_pkg(&game_cooked.join("J.upk"), 867);

    let context = GameInstallationContext::new(&root).unwrap();
    let probe = probe_ue3_installation(&context);
    assert!(probe.is_some(), "UE3 installation probe must succeed");
    let (proof, detection) = probe.unwrap();
    assert_eq!(detection, UnrealDetection::Generation { major: 3 });
    assert!(matches!(proof, UnrealPresenceProof::Ue3Package { .. }));
}

#[test]
fn test_iostore_version_whitelist_mapping() {
    // Whitelist rules:
    // 2 | 3 => Some(4)
    assert_eq!(generation_from_iostore_version(2), Some(4));
    assert_eq!(generation_from_iostore_version(3), Some(4));

    // 5 | 6 | 8 => Some(5)
    assert_eq!(generation_from_iostore_version(5), Some(5));
    assert_eq!(generation_from_iostore_version(6), Some(5));
    assert_eq!(generation_from_iostore_version(8), Some(5));

    // 1, 4, 7 and others => None
    assert_eq!(generation_from_iostore_version(0), None);
    assert_eq!(generation_from_iostore_version(1), None);
    assert_eq!(generation_from_iostore_version(4), None);
    assert_eq!(generation_from_iostore_version(7), None);
    assert_eq!(generation_from_iostore_version(9), None);
    assert_eq!(generation_from_iostore_version(10), None);
    assert_eq!(generation_from_iostore_version(255), None);
}

fn write_test_utoc(path: &std::path::Path, version: u8, valid_tables: bool) {
    use renderpilot_detection::pe::IOSTORE_TOC_MAGIC;
    let mut buf = vec![0u8; 1024];
    buf[0..16].copy_from_slice(&IOSTORE_TOC_MAGIC);
    buf[16] = version;
    buf[20..24].copy_from_slice(&144u32.to_le_bytes());
    if valid_tables {
        buf[24..28].copy_from_slice(&10u32.to_le_bytes());
        buf[28..32].copy_from_slice(&5u32.to_le_bytes());
        buf[32..36].copy_from_slice(&12u32.to_le_bytes());
    } else {
        buf[0..16].copy_from_slice(b"CORRUPT_MAGIC!!!");
    }
    std::fs::write(path, buf).unwrap();
}

#[test]
fn test_probe_iostore_installation_reconciliation_matrix() {
    struct IostoreTestCase<'a> {
        files: Vec<(&'a str, u8, bool)>,
        expected_gen: Option<u32>,
        desc: &'static str,
    }

    let cases = vec![
        IostoreTestCase {
            files: vec![("global.utoc", 6, true)],
            expected_gen: Some(5),
            desc: "one valid raw6 -> UE5",
        },
        IostoreTestCase {
            files: vec![("global.utoc", 3, true)],
            expected_gen: Some(4),
            desc: "one valid raw3 -> UE4",
        },
        IostoreTestCase {
            files: vec![("global.utoc", 7, true)],
            expected_gen: None,
            desc: "raw7 only -> presence without generation",
        },
        IostoreTestCase {
            files: vec![("A.utoc", 7, true), ("global.utoc", 6, true)],
            expected_gen: Some(5),
            desc: "raw7 first + raw6 later -> UE5",
        },
        IostoreTestCase {
            files: vec![("A.utoc", 3, true), ("global.utoc", 6, true)],
            expected_gen: None,
            desc: "raw3 + raw6 -> conflict / no generation",
        },
        IostoreTestCase {
            files: vec![("A.utoc", 6, true), ("global.utoc", 8, true)],
            expected_gen: Some(5),
            desc: "raw6 + raw8 -> UE5",
        },
        IostoreTestCase {
            files: vec![("A.utoc", 2, true), ("global.utoc", 3, true)],
            expected_gen: Some(4),
            desc: "raw2 + raw3 -> UE4",
        },
        IostoreTestCase {
            files: vec![("0_invalid.utoc", 6, false), ("global.utoc", 6, true)],
            expected_gen: Some(5),
            desc: "invalid .utoc before valid .utoc",
        },
    ];

    for tc in cases {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let paks = root.join("Game").join("Content").join("Paks");
        std::fs::create_dir_all(&paks).unwrap();

        for (name, ver, valid) in tc.files {
            write_test_utoc(&paks.join(name), ver, valid);
        }

        let context = GameInstallationContext::new(&root).unwrap();
        let probe = probe_iostore_installation(&context);
        assert!(
            probe.is_some(),
            "IoStore presence must be detected for case: {}",
            tc.desc
        );
        let (_proof, detected_gen) = probe.unwrap();
        assert_eq!(
            detected_gen, tc.expected_gen,
            "Generation mismatch for case: {}",
            tc.desc
        );
    }
}

#[test]
fn test_probe_iostore_bounded_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let paks = root.join("Game").join("Content").join("Paks");
    std::fs::create_dir_all(&paks).unwrap();

    // Create 15 candidates: A00..A14
    // First 10 ("A00".."A09") are corrupt.
    // Last 5 ("A10".."A14") are valid.
    // The bounded heap of size MAX_IOSTORE_PROBE_CANDIDATES (10) retains only "A00".."A09".
    for i in 0..10 {
        write_test_utoc(&paks.join(format!("A{:02}.utoc", i)), 0, false);
    }
    for i in 10..15 {
        write_test_utoc(&paks.join(format!("A{:02}.utoc", i)), 6, true);
    }

    let context = GameInstallationContext::new(&root).unwrap();
    let probe = probe_iostore_installation(&context);
    assert!(
        probe.is_none(),
        "Candidates beyond MAX_IOSTORE_PROBE_CANDIDATES must not be probed when first 10 exhaust budget"
    );
}

#[test]
fn test_probe_iostore_rejects_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let paks = root.join("Game").join("Content").join("Paks");
    let outside = dir.path().join("Outside");
    std::fs::create_dir_all(&paks).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    let real_in_root = paks.join("Real.utoc");
    write_test_utoc(&real_in_root, 6, true);

    let real_outside = outside.join("Outside.utoc");
    write_test_utoc(&real_outside, 3, true);

    let symlink_in_root = paks.join("00_LinkInRoot.utoc");
    let symlink_outside = paks.join("01_LinkOutside.utoc");

    #[cfg(unix)]
    let (link1, link2) = (
        std::os::unix::fs::symlink(&real_in_root, &symlink_in_root).is_ok(),
        std::os::unix::fs::symlink(&real_outside, &symlink_outside).is_ok(),
    );
    #[cfg(windows)]
    let (link1, link2) = (
        std::os::windows::fs::symlink_file(&real_in_root, &symlink_in_root).is_ok(),
        std::os::windows::fs::symlink_file(&real_outside, &symlink_outside).is_ok(),
    );

    if !link1 || !link2 {
        // Unprivileged symlink creation may not be enabled on Windows CI without Developer Mode.
        return;
    }

    assert!(symlink_in_root.is_symlink());
    assert!(symlink_outside.is_symlink());

    let context = GameInstallationContext::new(&root).unwrap();
    let probe = probe_iostore_installation(&context);
    assert!(probe.is_some(), "Real utoc file must still be detected");
    let (proof, detected_gen) = probe.unwrap();
    assert_eq!(
        detected_gen,
        Some(5),
        "Symlinks must be rejected; only Real.utoc (v6 -> UE5) should be considered"
    );
    match proof {
        UnrealPresenceProof::IoStoreContainer { source_file, .. } => {
            assert_eq!(
                source_file, real_in_root,
                "Presence proof must point to real file, not symlink"
            );
        }
        other => panic!("Expected IoStoreContainer proof, got {:?}", other),
    }

    // Now test a directory containing ONLY symlinks: must return None
    let dir_symlinks_only = tempfile::tempdir().unwrap();
    let root_symlinks = dir_symlinks_only.path().canonicalize().unwrap();
    let paks_symlinks = root_symlinks.join("Game").join("Content").join("Paks");
    std::fs::create_dir_all(&paks_symlinks).unwrap();

    let only_link = paks_symlinks.join("Link.utoc");
    #[cfg(unix)]
    let link_only_ok = std::os::unix::fs::symlink(&real_in_root, &only_link).is_ok();
    #[cfg(windows)]
    let link_only_ok = std::os::windows::fs::symlink_file(&real_in_root, &only_link).is_ok();

    if link_only_ok {
        assert!(only_link.is_symlink());
        let context_symlinks = GameInstallationContext::new(&root_symlinks).unwrap();
        assert!(
            probe_iostore_installation(&context_symlinks).is_none(),
            "Installation with only symlinked utoc files must be rejected"
        );
    }

    // Test parent directory symlink:
    // <root>/Game/Content/Paks -> symlink to <root>/RealPaks (which contains Real.utoc)
    let dir_parent_symlink = tempfile::tempdir().unwrap();
    let root_parent_symlink = dir_parent_symlink.path().canonicalize().unwrap();
    let real_paks_dir = root_parent_symlink.join("RealPaks");
    std::fs::create_dir_all(&real_paks_dir).unwrap();
    write_test_utoc(&real_paks_dir.join("Real.utoc"), 6, true);

    let game_content = root_parent_symlink.join("Game").join("Content");
    std::fs::create_dir_all(&game_content).unwrap();
    let symlinked_paks = game_content.join("Paks");

    #[cfg(unix)]
    let dir_link_ok = std::os::unix::fs::symlink(&real_paks_dir, &symlinked_paks).is_ok();
    #[cfg(windows)]
    let dir_link_ok = std::os::windows::fs::symlink_dir(&real_paks_dir, &symlinked_paks).is_ok();

    if dir_link_ok {
        assert!(symlinked_paks.is_symlink());
        let context_parent = GameInstallationContext::new(&root_parent_symlink).unwrap();
        assert!(
            probe_iostore_installation(&context_parent).is_none(),
            "Installation where Paks parent directory is a symlink must be rejected"
        );
    }
}

#[cfg(windows)]
#[test]
fn test_probe_iostore_rejects_windows_directory_junction() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let real_paks = root.join("RealPaks");
    std::fs::create_dir_all(&real_paks).unwrap();
    write_test_utoc(&real_paks.join("Real.utoc"), 6, true);

    let game_content = root.join("Game").join("Content");
    std::fs::create_dir_all(&game_content).unwrap();
    let junction_paks = game_content.join("Paks");

    // Attempt creating an NTFS directory junction using cmd /C mklink /J (unprivileged)
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction_paks)
        .arg(&real_paks)
        .status();

    if status.is_ok_and(|st| st.success()) {
        struct JunctionGuard(std::path::PathBuf);
        impl Drop for JunctionGuard {
            fn drop(&mut self) {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "rmdir"])
                    .arg(&self.0)
                    .status();
            }
        }
        let _guard = JunctionGuard(junction_paks.clone());

        // 1. Prove symlink_metadata().file_type().is_symlink() evaluates to true on Windows junctions:
        let meta =
            std::fs::symlink_metadata(&junction_paks).expect("junction metadata must succeed");
        assert!(
            meta.file_type().is_symlink(),
            "std::fs::symlink_metadata().file_type().is_symlink() must be true for Windows directory junctions"
        );

        // 2. Prove probe_iostore_installation rejects the junctioned Paks directory:
        let context = GameInstallationContext::new(&root).unwrap();
        let probe = probe_iostore_installation(&context);
        assert!(
            probe.is_none(),
            "probe_iostore_installation must reject directory junction Paks"
        );
    }
}

/// Regression test 20: Phase 1 marker is committed and preserved when supplemental Phase 2 exhausts budget
#[test]
fn test_phase1_marker_preserved_when_supplemental_phase2_exhausts_budget() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("TestGame");
    let bin_dir = root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let primary_path = bin_dir.join("TestGame.exe");
    let rdata_bytes = b"PRELUDE\0++UE4+Release-4.27\0TRAILING";
    let text_bytes = vec![0x90u8; 1024]; // 1 KiB .text

    write_synthetic_pe(
        &primary_path,
        &[
            (".rdata", 0x4000_0040, rdata_bytes),
            (".text", 0x6000_0020, &text_bytes),
        ],
    );

    let context = GameInstallationContext::new(&root).unwrap();
    let resolved = crate::game_executable::ResolvedExecutable {
        path: PathRef::new(&*primary_path.to_string_lossy()).unwrap(),
        file_name: "TestGame.exe".to_string(),
        graphics: renderpilot_domain::ExeGraphicsInfo::new(vec![], Some(Architecture::X64)),
        source: crate::game_executable::ExeSource::Auto,
    };

    // 1. Low-level scanner test: Phase 2 exhausts budget, Phase 1 marker preserved
    {
        let mut primary = BoundPrimaryExecutable::from_resolved(&context, &resolved).unwrap();
        // Budget covers .rdata, but exhausts before .text completes
        let mut budget = AnalysisBudget {
            section_stream_bytes_remaining: rdata_bytes.len() as u64 + 100,
            ..Default::default()
        };
        let mut coverage = Vec::new();

        let scan_res = scan_installation_release_markers(
            &context,
            Some(&mut primary),
            &mut [],
            &mut budget,
            &mut coverage,
            false,
        );

        let markers =
            scan_res.expect("Phase 1 marker must be returned despite Phase 2 budget exhaustion");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].claim().major(), 4);
        assert_eq!(markers[0].claim().minor(), Some(27));
        assert_eq!(budget.section_stream_bytes_remaining, 0);
    }

    // 2. High-level facade test: analyze_unreal_installation yields Unreal detection, not UnknownEngine
    {
        let mut budget = AnalysisBudget {
            section_stream_bytes_remaining: rdata_bytes.len() as u64 + 100,
            ..Default::default()
        };
        let report = analyze_unreal_installation(&context, Some(&resolved), &mut budget);

        assert_eq!(
            report.engine,
            EngineDetection::Unreal(UnrealDetection::MajorMinor {
                major: 4,
                minor: 27
            })
        );
        assert_eq!(report.presence_proofs.len(), 1);
        assert!(matches!(
            report.presence_proofs[0],
            UnrealPresenceProof::ReleaseMarker { .. }
        ));
        assert!(report.diagnostics.iter().any(|d| matches!(
            d,
            DetectionDiagnostic::SectionScanBudgetExceeded { section_name, .. } if section_name == ".text"
        )));
    }
}
