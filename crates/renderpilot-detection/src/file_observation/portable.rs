//! Strong file observation for Unix-like filesystems.

use std::{
    fs::{self, File},
    io,
    path::Path,
};

use renderpilot_application::AppResult;

use super::common::{read_and_hash, unavailable_or_error, unavailable_probe};
use super::{
    FileIdentityProbeResult, FileObservationResult, StableFileSnapshot, StrongFileCacheKey,
};

pub(super) fn probe_system_identity(path: &Path) -> AppResult<FileIdentityProbeResult> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(FileIdentityProbeResult::Missing);
        }
        Err(error) => return Ok(unavailable_probe(path, error)),
    };
    let before = match file.metadata() {
        Ok(metadata) if metadata.is_file() => portable_cache_key(&metadata),
        Ok(_) => return Ok(FileIdentityProbeResult::Unavailable),
        Err(error) => return Ok(unavailable_probe(path, error)),
    };
    let after = match file.metadata() {
        Ok(metadata) if metadata.is_file() => portable_cache_key(&metadata),
        Ok(_) => return Ok(FileIdentityProbeResult::Unavailable),
        Err(error) => return Ok(unavailable_probe(path, error)),
    };
    let reopened = match File::open(path).and_then(|file| file.metadata()) {
        Ok(metadata) if metadata.is_file() => portable_cache_key(&metadata),
        Ok(_) => return Ok(FileIdentityProbeResult::Unavailable),
        Err(error) => return Ok(unavailable_probe(path, error)),
    };
    if before == after && before == reopened {
        Ok(FileIdentityProbeResult::Available(before))
    } else {
        Ok(FileIdentityProbeResult::Unavailable)
    }
}

pub(super) fn observe_system_file(path: &Path) -> AppResult<FileObservationResult> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(FileObservationResult::Missing);
        }
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let before = match file.metadata() {
        Ok(metadata) if metadata.is_file() => portable_cache_key(&metadata),
        Ok(_) => return Ok(FileObservationResult::Unavailable),
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let (bytes, sha256) = match read_and_hash(&mut file, path) {
        Ok(value) => value,
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let after = match file.metadata() {
        Ok(metadata) if metadata.is_file() => portable_cache_key(&metadata),
        Ok(_) => return Ok(FileObservationResult::Unavailable),
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let reopened = match File::open(path).and_then(|file| file.metadata()) {
        Ok(metadata) if metadata.is_file() => portable_cache_key(&metadata),
        Ok(_) => return Ok(FileObservationResult::Unavailable),
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    if before != after || before != reopened {
        return Ok(FileObservationResult::Unavailable);
    }
    Ok(FileObservationResult::Available(StableFileSnapshot {
        cache_key: Some(before),
        sha256,
        bytes,
    }))
}

fn portable_cache_key(metadata: &fs::Metadata) -> StrongFileCacheKey {
    use std::os::unix::fs::MetadataExt;

    StrongFileCacheKey {
        kind: "linux_dev_inode_ctime".to_owned(),
        object_identity: format!("{:x}:{:x}", metadata.dev(), metadata.ino()),
        change_token: format!("{}:{}", metadata.ctime(), metadata.ctime_nsec()),
        size: metadata.len(),
    }
}
