//! Generates the Rust projection of the shared portable runtime release
//! contract.

use std::{env, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PortableRuntimeReleaseContract {
    contract_version: u16,
    supervisor_capability: u16,
    app_session_protocol: String,
    minimum_portable_schema: u32,
    current_schema: u32,
}

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be available"),
    );
    let contract_path = manifest_dir.join("../../data/contracts/portable-runtime-release.json");
    println!("cargo:rerun-if-changed={}", contract_path.display());

    let source = fs::read_to_string(&contract_path)
        .expect("portable runtime release contract must be readable");
    let contract: PortableRuntimeReleaseContract = serde_json::from_str(&source)
        .expect("portable runtime release contract must be valid JSON");
    assert_eq!(
        contract.contract_version, 1,
        "portable runtime release contract version must be supported"
    );
    assert!(
        contract.supervisor_capability > 0
            && contract.app_session_protocol == "renderpilot-portable-app-session-v1"
            && contract.minimum_portable_schema > 0
            && contract.minimum_portable_schema <= contract.current_schema
            && i32::try_from(contract.current_schema).is_ok(),
        "portable runtime release contract range must be valid"
    );

    let generated = format!(
        "/// Version of the shared portable runtime release contract.\n\
         pub const PORTABLE_RUNTIME_RELEASE_CONTRACT_VERSION: u16 = {};\n\
         /// Stable supervisor capability required by this release.\n\
         pub const PORTABLE_SUPERVISOR_CAPABILITY: u16 = {};\n\
         /// Private App-session identity shared by the supervisor and App.\n\
         pub const PORTABLE_APP_SESSION_PROTOCOL: &str = {:?};\n\
         /// Oldest released catalog schema accepted by portable migration.\n\
         pub const MINIMUM_PORTABLE_SCHEMA_VERSION: u32 = {};\n\
         /// Current catalog schema compiled into this storage generation.\n\
         pub const CURRENT_PORTABLE_SCHEMA_VERSION: u32 = {};\n\
         pub(crate) const CURRENT_SCHEMA_VERSION: i32 = {};\n",
        contract.contract_version,
        contract.supervisor_capability,
        contract.app_session_protocol,
        contract.minimum_portable_schema,
        contract.current_schema,
        contract.current_schema
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be available"))
        .join("portable_runtime_release_contract.rs");
    fs::write(output, generated).expect("portable runtime release contract must be generated");
}
