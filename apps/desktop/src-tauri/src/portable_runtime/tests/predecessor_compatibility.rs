use std::{fs, path::Path};

use serde::Serialize;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    generation::{
        InitialSelectedGeneration, StoredGeneration, inspect_initial_selection, load_selected,
        publish,
    },
    provenance::{self, SealDomain},
    rpu::{
        MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL,
        PORTABLE_SUPERVISOR_CAPABILITY, RPU_PROTOCOL, RpuManifest, VerifiedRpu,
    },
    selection::{SelectionRecordV2, append_selected},
    signature::sha256_hex,
    supervisor::select_initial_generation,
};

#[derive(Clone, Copy)]
enum ReceiptFixture {
    V2,
    V3,
}

fn portable_app() -> Vec<u8> {
    let mut app = vec![0_u8; 0x46];
    app[..2].copy_from_slice(b"MZ");
    app[0x3c..0x40].copy_from_slice(&0x40_u32.to_le_bytes());
    app[0x40..0x46].copy_from_slice(b"PE\0\0\x64\x86");
    app
}

fn current_rpu(version: &str, rpu_sha256: String, app: &[u8]) -> VerifiedRpu {
    VerifiedRpu {
        manifest: RpuManifest {
            protocol: RPU_PROTOCOL.to_owned(),
            platform: "windows-x86_64-portable".to_owned(),
            version: version.to_owned(),
            app_sha256: sha256_hex(app),
            app_length: app.len() as u64,
            minimum_supervisor_protocol: PORTABLE_SUPERVISOR_CAPABILITY,
            app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL.to_owned(),
            minimum_schema: MINIMUM_SCHEMA,
            maximum_schema: MAXIMUM_SCHEMA,
            portable_role: "app".to_owned(),
        },
        app_bytes: app.to_vec(),
        rpu_sha256,
    }
}

fn publish_fixture(store: &Path, version: &str) -> (Vec<u8>, String, StoredGeneration) {
    let app = portable_app();
    let generation_sha256 = hash('d');
    publish(
        store,
        &current_rpu(version, generation_sha256.clone(), &app),
    )
    .expect("publish predecessor fixture object");
    let stored = load_selected(store, &generation_sha256).expect("load predecessor fixture object");
    (app, generation_sha256, stored)
}

fn receipt_value(
    kind: ReceiptFixture,
    generation_sha256: &str,
    version: &str,
    app_sha256: &str,
    minimum_schema: u32,
    maximum_schema: u32,
) -> serde_json::Value {
    match kind {
        ReceiptFixture::V2 => serde_json::json!({
            "protocol": 2,
            "rpu_sha256": generation_sha256,
            "version": version,
            "app_sha256": app_sha256,
            "minimum_schema": minimum_schema,
            "maximum_schema": maximum_schema,
        }),
        ReceiptFixture::V3 => serde_json::json!({
            "protocol": 3,
            "rpu_sha256": generation_sha256,
            "version": version,
            "app_sha256": app_sha256,
            "minimum_supervisor_protocol": 2,
            "app_session_protocol": "renderpilot-portable-app-session-v1",
            "minimum_schema": minimum_schema,
            "maximum_schema": maximum_schema,
        }),
    }
}

fn replace_receipt(stored: &StoredGeneration, value: &serde_json::Value) {
    let plaintext = serde_json::to_vec(value).expect("encode predecessor receipt");
    let receipt = provenance::seal(
        SealDomain::Object,
        &format!("object:{}", stored.rpu_sha256),
        &plaintext,
    )
    .expect("seal predecessor receipt");
    fs::write(stored.generation_root.join("generation.json"), receipt)
        .expect("replace predecessor receipt");
}

fn publish_v2_selection(root: &Path, generation_sha256: &str) {
    let mut record = SelectionRecordV2 {
        protocol: 2,
        sequence: 1,
        generation_sha256: generation_sha256.to_owned(),
        previous_record_sha256: None,
        journal_sequence: 1,
        record_sha256: String::new(),
    };
    record.record_sha256 =
        sha256_hex(&serde_json::to_vec(&record).expect("encode unsigned v2 selection"));
    publish_sealed_selection_record(root, &record);
}

fn publish_sealed_selection_record(root: &Path, record: &impl Serialize) {
    let plaintext = serde_json::to_vec(record).expect("encode selection fixture");
    let digest = sha256_hex(&plaintext);
    let bytes = provenance::seal(
        SealDomain::Selection,
        &format!("selection:{digest}"),
        &plaintext,
    )
    .expect("seal selection fixture");
    fs::create_dir_all(root).expect("create selection root");
    fs::write(root.join(format!("{digest}.json")), bytes).expect("write selection fixture");
}

#[test]
fn receipt_v2_predecessor_accepts_only_the_exact_released_identity() {
    let root = temp_root("receipt-v2-predecessor");
    let store = root.path().join("store");
    let (app, generation_sha256, stored) = publish_fixture(&store, "1.9.0");
    let app_sha256 = sha256_hex(&app);

    replace_receipt(
        &stored,
        &receipt_value(
            ReceiptFixture::V2,
            &generation_sha256,
            "1.9.0",
            &app_sha256,
            4,
            16,
        ),
    );
    assert_eq!(
        error_code(load_selected(&store, &generation_sha256)),
        "portable_generation_receipt",
        "metadata-only predecessors are never launchable"
    );
    assert!(matches!(
        inspect_initial_selection(&store, &generation_sha256)
            .expect("inspect exact receipt-v2 predecessor"),
        InitialSelectedGeneration::MetadataOnly(predecessor)
            if predecessor.version.to_string() == "1.9.0"
    ));

    for (version, minimum, maximum, digest) in [
        ("1.9.1", 4, 16, app_sha256.clone()),
        ("1.9.0", 5, 16, app_sha256.clone()),
        ("1.9.0", 4, 17, app_sha256),
        ("1.9.0", 4, 16, hash('f')),
    ] {
        replace_receipt(
            &stored,
            &receipt_value(
                ReceiptFixture::V2,
                &generation_sha256,
                version,
                &digest,
                minimum,
                maximum,
            ),
        );
        assert_eq!(
            error_code(inspect_initial_selection(&store, &generation_sha256)),
            "portable_generation_receipt",
            "receipt-v2 version, schema, and App identity are one sealed contract"
        );
    }
}

#[test]
fn receipt_v3_predecessor_accepts_only_the_exact_session_bound_identity() {
    let root = temp_root("receipt-v3-predecessor");
    let store = root.path().join("store");
    let (app, generation_sha256, stored) = publish_fixture(&store, "1.9.1");
    let app_sha256 = sha256_hex(&app);

    let seal = |version: &str,
                supervisor_capability: u16,
                app_session_protocol: &str,
                minimum_schema: u32,
                maximum_schema: u32,
                digest: &str| {
        serde_json::json!({
            "protocol": 3,
            "rpu_sha256": generation_sha256,
            "version": version,
            "app_sha256": digest,
            "minimum_supervisor_protocol": supervisor_capability,
            "app_session_protocol": app_session_protocol,
            "minimum_schema": minimum_schema,
            "maximum_schema": maximum_schema,
        })
    };

    replace_receipt(
        &stored,
        &seal(
            "1.9.1",
            2,
            "renderpilot-portable-app-session-v1",
            4,
            16,
            &app_sha256,
        ),
    );
    assert!(matches!(
        inspect_initial_selection(&store, &generation_sha256)
            .expect("inspect exact receipt-v3 predecessor"),
        InitialSelectedGeneration::MetadataOnly(predecessor)
            if predecessor.version.to_string() == "1.9.1"
    ));

    for (version, capability, session, minimum, maximum, digest) in [
        (
            "1.9.0",
            2,
            "renderpilot-portable-app-session-v1",
            4,
            16,
            app_sha256.clone(),
        ),
        (
            "1.9.1",
            3,
            "renderpilot-portable-app-session-v1",
            4,
            16,
            app_sha256.clone(),
        ),
        (
            "1.9.1",
            2,
            "renderpilot-portable-app-session-v2",
            4,
            16,
            app_sha256.clone(),
        ),
        (
            "1.9.1",
            2,
            "renderpilot-portable-app-session-v1",
            5,
            16,
            app_sha256.clone(),
        ),
        (
            "1.9.1",
            2,
            "renderpilot-portable-app-session-v1",
            4,
            17,
            app_sha256,
        ),
        (
            "1.9.1",
            2,
            "renderpilot-portable-app-session-v1",
            4,
            16,
            hash('f'),
        ),
    ] {
        replace_receipt(
            &stored,
            &seal(version, capability, session, minimum, maximum, &digest),
        );
        assert_eq!(
            error_code(inspect_initial_selection(&store, &generation_sha256)),
            "portable_generation_receipt",
            "receipt-v3 version, session, schema, and App identity are one sealed contract"
        );
    }
}

#[test]
fn metadata_only_predecessors_require_a_v3_tip_and_a_newer_embedded_generation() {
    for (label, kind, predecessor_version) in [
        ("receipt-v2-bridge", ReceiptFixture::V2, "1.9.0"),
        ("receipt-v3-bridge", ReceiptFixture::V3, "1.9.1"),
    ] {
        let root = temp_root(label);
        let store = root.path().join("store");
        let (app, generation_sha256, stored) = publish_fixture(&store, predecessor_version);
        replace_receipt(
            &stored,
            &receipt_value(
                kind,
                &generation_sha256,
                predecessor_version,
                &sha256_hex(&app),
                4,
                16,
            ),
        );

        let v2_selection_root = store.join("v2-selection");
        publish_v2_selection(&v2_selection_root, &generation_sha256);
        assert_eq!(
            error_code(select_initial_generation(
                &store,
                &v2_selection_root,
                current_rpu("2.0.0", hash('g'), &app),
            )),
            "portable_selection_invalid",
            "metadata-only bridges accept only the released protocol-3 selection authority"
        );

        let v3_selection_root = store.join("v3-selection");
        append_selected(&v3_selection_root, &generation_sha256, &hash('1'), 1)
            .expect("select metadata-only predecessor");
        assert_eq!(
            error_code(select_initial_generation(
                &store,
                &v3_selection_root,
                current_rpu(predecessor_version, hash('h'), &app),
            )),
            "portable_full_package_upgrade_required",
            "a metadata-only predecessor cannot authorize an equal-version replacement"
        );

        let selected = select_initial_generation(
            &store,
            &v3_selection_root,
            current_rpu("2.0.0", hash('e'), &app),
        )
        .expect("newer embedded generation supersedes metadata-only predecessor");
        assert_eq!(selected.generation_sha256, hash('e'));
        assert_eq!(
            selected.selection_predecessor_generation_sha256.as_deref(),
            Some(generation_sha256.as_str())
        );
    }
}
