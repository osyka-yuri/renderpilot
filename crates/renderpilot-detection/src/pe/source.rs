//! Bounded random-access reads shared by streaming PE inspectors.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use super::binary::{read_u16, read_u32};
use super::header::{COFF_HEADER_LEN, DOS_PE_POINTER_OFFSET, MAX_SECTIONS, SECTION_HEADER_LEN};

/// Bytes required to cover the DOS signature and the PE-header pointer.
const DOS_HEADER_PROBE_LEN: usize = 64;

/// Hard ceiling for the complete PE header region loaded into memory.
///
/// Normal images use only a few KiB. One MiB leaves generous room for unusual
/// section tables while preventing a crafted `e_lfanew` from turning a bounded
/// inspector into a multi-gigabyte allocation.
const MAX_HEADER_REGION_LEN: usize = 1024 * 1024;

/// A bounded random-access byte source.
///
/// Both on-disk and in-memory PE entry points use this abstraction so format
/// validation is implemented once without loading a potentially huge image.
pub(super) trait ByteSource {
    /// Reads exactly `len` bytes at `offset`.
    fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>>;
}

impl ByteSource for File {
    fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let start = u64::try_from(offset).ok()?;
        self.seek(SeekFrom::Start(start)).ok()?;
        let mut buffer = vec![0; len];
        self.read_exact(&mut buffer).ok()?;
        Some(buffer)
    }
}

impl ByteSource for &[u8] {
    fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let end = offset.checked_add(len)?;
        self.get(offset..end).map(<[u8]>::to_vec)
    }
}

/// Reads only the DOS/PE/COFF/optional-header/section-table region.
pub(super) fn read_header_region(source: &mut impl ByteSource) -> Option<Vec<u8>> {
    let dos = source.read_exact_at(0, DOS_HEADER_PROBE_LEN)?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    let pe_offset = usize::try_from(read_u32(&dos, DOS_PE_POINTER_OFFSET)?).ok()?;

    let coff_region_len = 4 + COFF_HEADER_LEN;
    if pe_offset.checked_add(coff_region_len)? > MAX_HEADER_REGION_LEN {
        return None;
    }
    let coff = source.read_exact_at(pe_offset, coff_region_len)?;
    if &coff[..4] != b"PE\0\0" {
        return None;
    }
    let section_count = usize::from(read_u16(&coff, 4 + 2)?);
    if section_count > MAX_SECTIONS {
        return None;
    }
    let optional_header_size = usize::from(read_u16(&coff, 4 + 16)?);
    let optional_header_offset = pe_offset.checked_add(4 + COFF_HEADER_LEN)?;
    let header_end = optional_header_offset
        .checked_add(optional_header_size)?
        .checked_add(section_count.checked_mul(SECTION_HEADER_LEN)?)?;
    if header_end > MAX_HEADER_REGION_LEN {
        return None;
    }

    source.read_exact_at(0, header_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct HeaderProbe {
        pe_offset: usize,
        optional_header_size: u16,
        reads: Vec<(usize, usize)>,
    }

    impl HeaderProbe {
        fn new(pe_offset: usize, optional_header_size: u16) -> Self {
            Self {
                pe_offset,
                optional_header_size,
                reads: Vec::new(),
            }
        }
    }

    impl ByteSource for HeaderProbe {
        fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
            self.reads.push((offset, len));
            if offset == 0 && len == DOS_HEADER_PROBE_LEN {
                let mut dos = vec![0; DOS_HEADER_PROBE_LEN];
                dos[..2].copy_from_slice(b"MZ");
                dos[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
                    .copy_from_slice(&u32::try_from(self.pe_offset).ok()?.to_le_bytes());
                return Some(dos);
            }
            if offset == self.pe_offset && len == 4 + COFF_HEADER_LEN {
                let mut coff = vec![0; len];
                coff[..4].copy_from_slice(b"PE\0\0");
                coff[4 + 2..4 + 4].copy_from_slice(&1_u16.to_le_bytes());
                coff[4 + 16..4 + 18].copy_from_slice(&self.optional_header_size.to_le_bytes());
                return Some(coff);
            }
            None
        }
    }

    #[test]
    fn rejects_oversized_pe_offset_before_seeking_to_it() {
        let mut source = HeaderProbe::new(MAX_HEADER_REGION_LEN, 0);

        assert_eq!(read_header_region(&mut source), None);
        assert_eq!(source.reads, vec![(0, DOS_HEADER_PROBE_LEN)]);
    }

    #[test]
    fn rejects_oversized_header_before_allocating_it() {
        let coff_region_len = 4 + COFF_HEADER_LEN;
        let pe_offset = MAX_HEADER_REGION_LEN - coff_region_len;
        let mut source = HeaderProbe::new(pe_offset, 1);

        assert_eq!(read_header_region(&mut source), None);
        assert_eq!(
            source.reads,
            vec![(0, DOS_HEADER_PROBE_LEN), (pe_offset, coff_region_len)]
        );
    }
}
