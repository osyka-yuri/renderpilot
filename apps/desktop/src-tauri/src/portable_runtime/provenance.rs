//! Portable-local provenance for supervisor-owned mutations.
//!
//! The key and authenticated head live inside the portable authority namespace,
//! so moving the complete portable root preserves its recovery state. This
//! layer detects corrupted or inconsistent receipts; executable authenticity
//! remains the signed-RPU boundary.

#![expect(
    unsafe_code,
    reason = "volume capability checks are constrained to this Win32 boundary"
)]

use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation},
    Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, GetVolumeInformationW, GetVolumePathNameW,
    },
};

use super::{
    error::{PortableRuntimeError, Result},
    random::{bytes_32, hex},
    signature::sha256_hex,
    win32::{
        directory::{verify_admission_handle, verify_directory_no_reparse},
        process::path_wide_nul,
    },
};

const PROVENANCE_PROTOCOL: u16 = 12;
const HEAD_DOMAIN: &str = "portable-trust-head-v1.2";
const PROVENANCE_DIRECTORY: &str = "provenance";
const MASTER_KEY_FILE: &str = "master-key-v1.bin";
const LOCK_FILE: &str = "provenance.lock";
const HEAD_SLOTS: [&str; 2] = ["head-a.sealed", "head-b.sealed"];

static AUTHORITY: OnceLock<Mutex<AnchorState>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SealDomain {
    Journal,
    Object,
    Selection,
    Snapshot,
    Migration,
    Terminal,
    Cleanup,
}

impl SealDomain {
    fn label(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::Object => "object",
            Self::Selection => "selection",
            Self::Snapshot => "snapshot",
            Self::Migration => "migration",
            Self::Terminal => "terminal",
            Self::Cleanup => "cleanup",
        }
    }
}

#[derive(Debug)]
struct AnchorState {
    _lock: Option<File>,
    root: Option<PathBuf>,
    master_key: [u8; 32],
    generation: u64,
    head_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SealedEnvelopeV12 {
    protocol: u16,
    domain: String,
    object_id: String,
    generation: u64,
    payload_hex: String,
    tag_hex: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuthenticatedHeadV12 {
    protocol: u16,
    generation: u64,
    head_sha256: String,
    tag_hex: String,
}

/// Installs the sole mutation authority after the supervisor has authenticated
/// its portable root and retained D18 admission.
pub(super) fn install(authority_root: &Path) -> Result<()> {
    require_supported_volume(authority_root)?;
    let anchor = provenance_root(authority_root);
    std::fs::create_dir_all(&anchor)?;
    verify_directory_no_reparse(&anchor)?;
    let lock = open_provenance_lock(&anchor.join(LOCK_FILE))?;
    let master_key = load_or_create_master_key(&anchor)?;
    let (generation, head_sha256) = load_head(&anchor, &master_key)?;
    AUTHORITY
        .set(Mutex::new(AnchorState {
            _lock: Some(lock),
            root: Some(anchor),
            master_key,
            generation,
            head_sha256,
        }))
        .map_err(|_| {
            PortableRuntimeError::new(
                "portable_provenance",
                "portable provenance was already installed",
            )
        })
}

fn provenance_root(authority_root: &Path) -> PathBuf {
    authority_root.join(PROVENANCE_DIRECTORY)
}

/// The provenance anchor alone retains this path-based share-zero lock. It is
/// intentionally not a reusable Win32 authority API.
fn open_provenance_lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    verify_admission_handle(file.as_raw_handle().cast())?;
    // SAFETY: provenance retains this one file handle and clearing inheritance
    // neither transfers nor duplicates ownership.
    if unsafe { SetHandleInformation(file.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "provenance lock inheritance could not be cleared",
        ));
    }
    Ok(file)
}

/// Seals a complete canonical payload.  Creating an envelope records the
/// anchored mutation intent first; callers must report its observed immutable
/// result after successful publication.
pub(super) fn seal(domain: SealDomain, object_id: &str, payload: &[u8]) -> Result<Vec<u8>> {
    intent(domain, object_id, payload)?;
    with_authority(|state| {
        let generation = state.generation;
        let tag_hex = hmac_hex(
            &state.master_key,
            &envelope_mac_input(domain, object_id, generation, payload),
        );
        serde_json::to_vec(&SealedEnvelopeV12 {
            protocol: PROVENANCE_PROTOCOL,
            domain: domain.label().to_owned(),
            object_id: object_id.to_owned(),
            generation,
            payload_hex: hex(payload),
            tag_hex,
        })
        .map_err(|error| PortableRuntimeError::new("portable_provenance", error.to_string()))
    })
}

/// Records an anchored intent for a mutation whose bytes are not themselves a
/// serializable portable envelope (for example SQLite backup/restore).
pub(super) fn intent(domain: SealDomain, object_id: &str, intended: &[u8]) -> Result<()> {
    if !canonical_object_id(object_id) {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "sealed object id was not canonical",
        ));
    }
    with_authority(|state| advance_head(state, "intent", domain, object_id, intended))
}

/// Opens only a v1.2 envelope for the requested domain/object.  Raw JSON and
/// unsealed legacy bytes are deliberately retained and rejected; no repair,
/// rewrite, or cleanup is attempted on that path.
pub(super) fn open(domain: SealDomain, object_id: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let envelope: SealedEnvelopeV12 = serde_json::from_slice(bytes).map_err(|_| {
        PortableRuntimeError::new(
            "portable_legacy_unsealed",
            "legacy or malformed unsealed portable state was retained",
        )
    })?;
    if envelope.protocol != PROVENANCE_PROTOCOL
        || envelope.domain != domain.label()
        || envelope.object_id != object_id
        || !canonical_object_id(object_id)
    {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "sealed envelope version, domain, or object identity was invalid",
        ));
    }
    let payload = decode_hex(&envelope.payload_hex)?;
    with_authority(|state| {
        let expected = hmac_hex(
            &state.master_key,
            &envelope_mac_input(domain, object_id, envelope.generation, &payload),
        );
        if !constant_time_eq(expected.as_bytes(), envelope.tag_hex.as_bytes()) {
            return Err(PortableRuntimeError::new(
                "portable_provenance",
                "sealed envelope authentication failed",
            ));
        }
        Ok(payload)
    })
}

/// Commits the post-effect observation to the alternating authenticated head.
/// This is intentionally explicit at each mutation site: intent precedes a
/// filesystem/database action and observed state follows it.
pub(super) fn observe(domain: SealDomain, object_id: &str, observed: &[u8]) -> Result<()> {
    if !canonical_object_id(object_id) {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "observed object id was not canonical",
        ));
    }
    with_authority(|state| advance_head(state, "observed", domain, object_id, observed))
}

fn with_authority<T>(operation: impl FnOnce(&mut AnchorState) -> Result<T>) -> Result<T> {
    if let Some(authority) = AUTHORITY.get() {
        let mut state = authority.lock().map_err(|_| {
            PortableRuntimeError::new("portable_provenance", "provenance mutex was poisoned")
        })?;
        return operation(&mut state);
    }
    #[cfg(test)]
    {
        static TEST_AUTHORITY: OnceLock<Mutex<AnchorState>> = OnceLock::new();
        let authority = TEST_AUTHORITY.get_or_init(|| {
            Mutex::new(AnchorState {
                _lock: None,
                root: None,
                master_key: [0x5a; 32],
                generation: 0,
                head_sha256: sha256_hex(b"renderpilot-portable-trust-genesis-v1.2"),
            })
        });
        let mut state = authority.lock().map_err(|_| {
            PortableRuntimeError::new("portable_provenance", "test provenance mutex was poisoned")
        })?;
        operation(&mut state)
    }
    #[cfg(not(test))]
    Err(PortableRuntimeError::new(
        "portable_provenance_missing",
        "portable mutation attempted without authenticated provenance",
    ))
}

fn advance_head(
    state: &mut AnchorState,
    verb: &str,
    domain: SealDomain,
    object_id: &str,
    bytes: &[u8],
) -> Result<()> {
    let next_generation = state.generation.checked_add(1).ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_provenance",
            "authenticated head generation overflowed",
        )
    })?;
    let next_head = sha256_hex(&head_input(
        verb,
        domain,
        object_id,
        next_generation,
        &state.head_sha256,
        bytes,
    ));
    if let Some(root) = &state.root {
        write_head_slot(root, &state.master_key, next_generation, &next_head)?;
    }
    state.generation = next_generation;
    state.head_sha256 = next_head;
    Ok(())
}

fn load_or_create_master_key(anchor: &Path) -> Result<[u8; 32]> {
    let path = anchor.join(MASTER_KEY_FILE);
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut key = [0_u8; 32];
            key.copy_from_slice(&bytes);
            Ok(key)
        }
        Ok(_) => Err(PortableRuntimeError::new(
            "portable_provenance",
            "portable provenance master key had an invalid length",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = bytes_32()?;
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            file.write_all(&key)?;
            file.sync_all()?;
            Ok(key)
        }
        Err(error) => Err(error.into()),
    }
}

fn load_head(anchor: &Path, key: &[u8; 32]) -> Result<(u64, String)> {
    let mut valid = Vec::new();
    let mut invalid_seen = false;
    for slot in HEAD_SLOTS {
        let path = anchor.join(slot);
        match std::fs::read(&path) {
            Ok(bytes) => match parse_head(key, &bytes) {
                Ok(head) => valid.push(head),
                Err(_) => invalid_seen = true,
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    valid.sort_by_key(|head| head.0);
    match valid.pop() {
        Some(head) => Ok(head),
        None if !invalid_seen => Ok((0, sha256_hex(b"renderpilot-portable-trust-genesis-v1.2"))),
        None => Err(PortableRuntimeError::new(
            "portable_provenance",
            "all authenticated head slots were invalid; state was retained",
        )),
    }
}

fn parse_head(key: &[u8; 32], bytes: &[u8]) -> Result<(u64, String)> {
    let head: AuthenticatedHeadV12 = serde_json::from_slice(bytes).map_err(|error| {
        PortableRuntimeError::new(
            "portable_provenance",
            format!("head decode failed: {error}"),
        )
    })?;
    if head.protocol != PROVENANCE_PROTOCOL
        || head.head_sha256.len() != 64
        || !head
            .head_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "head was invalid",
        ));
    }
    let expected = hmac_hex(key, &head_mac_input(head.generation, &head.head_sha256));
    if !constant_time_eq(expected.as_bytes(), head.tag_hex.as_bytes()) {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "head authentication failed",
        ));
    }
    Ok((head.generation, head.head_sha256))
}

fn write_head_slot(
    anchor: &Path,
    key: &[u8; 32],
    generation: u64,
    head_sha256: &str,
) -> Result<()> {
    let slot = anchor.join(HEAD_SLOTS[(generation as usize) % HEAD_SLOTS.len()]);
    let head = AuthenticatedHeadV12 {
        protocol: PROVENANCE_PROTOCOL,
        generation,
        head_sha256: head_sha256.to_owned(),
        tag_hex: hmac_hex(key, &head_mac_input(generation, head_sha256)),
    };
    let bytes = serde_json::to_vec(&head)
        .map_err(|error| PortableRuntimeError::new("portable_provenance", error.to_string()))?;
    // A torn replacement can damage only the selected slot; the alternate
    // authenticated monotonic slot remains a valid recovery head.
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(slot)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn require_supported_volume(path: &Path) -> Result<()> {
    let display = path.as_os_str().to_string_lossy();
    if display.starts_with(r"\\") {
        return Err(PortableRuntimeError::new(
            "portable_storage_unsupported",
            "UNC portable roots cannot provide required local durability/recovery",
        ));
    }
    let path_wide = path_wide_nul(path);
    let mut volume_root = vec![0_u16; 32768];
    // SAFETY: input and output UTF-16 buffers remain valid through the call.
    if unsafe {
        GetVolumePathNameW(
            path_wide.as_ptr(),
            volume_root.as_mut_ptr(),
            volume_root.len() as u32,
        )
    } == 0
    {
        return Err(PortableRuntimeError::new(
            "portable_storage_unsupported",
            "could not determine portable-root volume",
        ));
    }
    let mut filesystem = vec![0_u16; 64];
    // SAFETY: only the filesystem-name output buffer is requested.
    if unsafe {
        GetVolumeInformationW(
            volume_root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Err(PortableRuntimeError::new(
            "portable_storage_unsupported",
            "could not inspect portable-root filesystem capability",
        ));
    }
    let end = filesystem.iter().position(|unit| *unit == 0).unwrap_or(0);
    let filesystem = String::from_utf16_lossy(&filesystem[..end]);
    if !matches!(filesystem.as_str(), "NTFS" | "ReFS") {
        return Err(PortableRuntimeError::new(
            "portable_storage_unsupported",
            "portable mutation/recovery requires local NTFS or ReFS",
        ));
    }
    Ok(())
}

fn envelope_mac_input(
    domain: SealDomain,
    object_id: &str,
    generation: u64,
    payload: &[u8],
) -> Vec<u8> {
    canonical_parts(&[
        b"renderpilot-portable-envelope-v1.2",
        domain.label().as_bytes(),
        object_id.as_bytes(),
        &generation.to_le_bytes(),
        payload,
    ])
}

fn head_mac_input(generation: u64, head_sha256: &str) -> Vec<u8> {
    canonical_parts(&[
        HEAD_DOMAIN.as_bytes(),
        &generation.to_le_bytes(),
        head_sha256.as_bytes(),
    ])
}

fn head_input(
    verb: &str,
    domain: SealDomain,
    object_id: &str,
    generation: u64,
    previous_head: &str,
    bytes: &[u8],
) -> Vec<u8> {
    canonical_parts(&[
        HEAD_DOMAIN.as_bytes(),
        verb.as_bytes(),
        domain.label().as_bytes(),
        object_id.as_bytes(),
        &generation.to_le_bytes(),
        previous_head.as_bytes(),
        bytes,
    ])
}

fn canonical_parts(parts: &[&[u8]]) -> Vec<u8> {
    let mut output = Vec::new();
    for part in parts {
        output.extend_from_slice(&(part.len() as u64).to_le_bytes());
        output.extend_from_slice(part);
    }
    output
}

fn hmac_hex(key: &[u8; 32], message: &[u8]) -> String {
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    hex(&outer.finalize())
}

fn decode_hex(input: &str) -> Result<Vec<u8>> {
    if !input.len().is_multiple_of(2) || !input.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PortableRuntimeError::new(
            "portable_provenance",
            "sealed payload hex was invalid",
        ));
    }
    (0..input.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&input[index..index + 2], 16).map_err(|error| {
                PortableRuntimeError::new("portable_provenance", error.to_string())
            })
        })
        .collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn canonical_object_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 384
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/' | b':')
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{load_head, load_or_create_master_key, provenance_root, write_head_slot};
    use crate::portable_runtime::{signature::sha256_hex, tests::temp_root};

    #[test]
    fn provenance_key_and_head_move_with_the_complete_portable_root() {
        let sandbox = temp_root("provenance-move");
        let original_portable = sandbox.path().join("portable-original");
        let original_authority = original_portable
            .join(".renderpilot-runtime-authority")
            .join("v1");
        let original_provenance = provenance_root(&original_authority);
        fs::create_dir_all(&original_provenance).expect("create original provenance root");

        let key = load_or_create_master_key(&original_provenance)
            .expect("create portable provenance key");
        let head = sha256_hex(b"portable relocation head");
        write_head_slot(&original_provenance, &key, 7, &head).expect("persist authenticated head");

        let moved_portable = sandbox.path().join("portable-moved");
        fs::rename(&original_portable, &moved_portable).expect("move complete portable root");
        let moved_authority = moved_portable
            .join(".renderpilot-runtime-authority")
            .join("v1");
        let moved_provenance = provenance_root(&moved_authority);

        assert_eq!(moved_provenance, moved_authority.join("provenance"));
        assert!(!original_provenance.exists());
        assert_eq!(
            load_or_create_master_key(&moved_provenance).expect("load moved provenance key"),
            key
        );
        assert_eq!(
            load_head(&moved_provenance, &key).expect("load moved authenticated head"),
            (7, head)
        );
    }
}
