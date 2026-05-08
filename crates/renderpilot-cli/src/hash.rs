//! SHA-256 helpers.

use std::io::{self, ErrorKind, Read};

use sha2::{Digest, Sha256};

const HASH_BUFFER_SIZE: usize = 64 * 1024;

fn finalize_hex(hasher: Sha256) -> String {
    hex::encode(hasher.finalize())
}

#[cfg(test)]
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    finalize_hex(hasher)
}

#[cfg(test)]
pub(crate) fn sha256_hex_parts(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();

    for part in parts {
        hasher.update(part);
    }

    finalize_hex(hasher)
}

pub(crate) fn sha256_reader_hex(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => hasher.update(&buffer[..bytes_read]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(finalize_hex(hasher))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, ErrorKind, Read};

    use super::{sha256_hex, sha256_hex_parts, sha256_reader_hex};

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn hashes_bytes_as_lowercase_hex() {
        assert_eq!(sha256_hex(b"abc"), ABC_SHA256);
    }

    #[test]
    fn hashes_empty_bytes() {
        assert_eq!(sha256_hex(b""), EMPTY_SHA256);
    }

    #[test]
    fn hashes_parts_as_contiguous_bytes() {
        assert_eq!(sha256_hex_parts(&[b"a", b"b", b"c"]), ABC_SHA256);
    }

    #[test]
    fn hashes_empty_parts() {
        assert_eq!(sha256_hex_parts(&[]), EMPTY_SHA256);
    }

    #[test]
    fn hashes_reader_as_lowercase_hex() {
        let reader = Cursor::new(b"abc");

        assert_eq!(
            sha256_reader_hex(reader).expect("hash should compute"),
            ABC_SHA256
        );
    }

    #[test]
    fn retries_interrupted_reader_reads() {
        let reader = InterruptedOnce {
            inner: Cursor::new(b"abc"),
            interrupted: false,
        };

        assert_eq!(
            sha256_reader_hex(reader).expect("hash should compute after interrupted read"),
            ABC_SHA256
        );
    }

    struct InterruptedOnce<R> {
        inner: R,
        interrupted: bool,
    }

    impl<R: Read> Read for InterruptedOnce<R> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::Error::new(ErrorKind::Interrupted, "interrupted"));
            }

            self.inner.read(buffer)
        }
    }
}
