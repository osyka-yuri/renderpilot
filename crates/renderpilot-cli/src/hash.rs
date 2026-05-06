//! SHA-256 helpers.

use std::io::Read;

use sha2::{Digest, Sha256};

const HASH_BUFFER_SIZE: usize = 64 * 1024;

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub(crate) fn sha256_hex_parts(parts: &[&[u8]]) -> String {
    if let [part] = parts {
        return sha256_hex(part);
    }

    let mut hasher = Sha256::new();

    for part in parts {
        hasher.update(*part);
    }

    hex::encode(hasher.finalize())
}

pub(crate) fn sha256_reader_hex(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{sha256_hex, sha256_hex_parts, sha256_reader_hex};

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn hashes_bytes_as_lowercase_hex() {
        assert_eq!(sha256_hex(b"abc"), ABC_SHA256);
    }

    #[test]
    fn hashes_parts_as_contiguous_bytes() {
        assert_eq!(sha256_hex_parts(&[b"a", b"b", b"c"]), ABC_SHA256);
    }

    #[test]
    fn hashes_reader_as_lowercase_hex() {
        let reader = Cursor::new(b"abc");

        assert_eq!(
            sha256_reader_hex(reader).expect("hash should compute"),
            ABC_SHA256
        );
    }
}
