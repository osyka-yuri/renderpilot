//! Graphics API and architecture detection from a PE executable.
//!
//! Reads two signals from a Windows executable without loading it: the COFF
//! machine type yields the [`Architecture`], and the names in the import
//! directories yield the set of imported [`GraphicsApi`]s. Both the static
//! import directory (index 1) and the optional delay-load import directory
//! (index 13) are walked: UE5 and other engines delay-load `d3d12.dll` /
//! `dxgi.dll`, so the static table alone is not enough. The shared
//! header/section walk lives in [`super::header`]; this module reads only the
//! import data directories and walks their descriptors.
//!
//! This module reports detection facts only — the deduplicated set of imported
//! graphics APIs, without any product-specific ranking. The orchestration layer
//! applies policy (e.g. "pick the most capable DirectX API for RenoDX") on top
//! of the set.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use renderpilot_domain::{Architecture, ExeGraphicsInfo, GraphicsApi};

use super::binary::{read_u16, read_u32};
use super::header::{
    COFF_HEADER_LEN, DOS_PE_POINTER_OFFSET, MAX_SECTIONS, PeHeaders, SECTION_HEADER_LEN,
    rva_to_offset,
};

const IMPORT_DESCRIPTOR_LEN: usize = 20;
const IMPORT_DESCRIPTOR_NAME_OFFSET: usize = 12;
const IMPORT_DIRECTORY_INDEX: usize = 1;

/// The optional delay-load import directory (data directory index 13). Some
/// engines (notably Unreal Engine 5) delay-load `d3d12.dll` / `dxgi.dll`
/// instead of importing them statically, so reading only the static import
/// directory leaves the detected API set empty. A delay-load descriptor is
/// 32 bytes with the DLL name RVA at offset 4; the table is null-terminated
/// by an all-zero descriptor.
const DELAY_DIRECTORY_INDEX: usize = 13;
const DELAY_DESCRIPTOR_LEN: usize = 32;
const DELAY_DESCRIPTOR_NAME_OFFSET: usize = 4;

const MACHINE_I386: u16 = 0x014c;
const MACHINE_AMD64: u16 = 0x8664;

/// Cap on import descriptors walked, guarding against malformed binaries that
/// never present a null terminator. Real executables import from far fewer DLLs.
const MAX_IMPORT_DESCRIPTORS: usize = 4096;
/// Cap on a DLL name length, guarding against a missing string terminator.
const MAX_DLL_NAME_LEN: usize = 256;

/// Analyzes the executable at `path`, returning its imported graphics API set
/// and architecture. Returns an empty API set / `None` when the file cannot be
/// read or parsed. Unexpected I/O failures (permissions, locks) are logged via
/// `log::warn!`; a missing file is silent (a stale candidate path is normal).
///
/// Reads only the PE header region and the specific import/delay-import
/// descriptor + name-string bytes from disk (a few KB total), never the whole
/// file — a multi-hundred-MB game executable is not slurped into memory just to
/// inspect its first few KB of headers.
#[must_use]
pub fn analyze_executable(path: &Path) -> ExeGraphicsInfo {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return ExeGraphicsInfo::new(Vec::new(), None);
        }
        Err(err) => {
            log::warn!(
                "failed to open executable for graphics detection at {}: {err}",
                path.display()
            );
            return ExeGraphicsInfo::new(Vec::new(), None);
        }
    };

    let Some(header_buf) = read_header_region(&mut file) else {
        return ExeGraphicsInfo::new(Vec::new(), None);
    };
    // `PeHeaders::parse` only indexes within the header region (DOS/PE/COFF/
    // optional header + section table), so a buffer truncated to `header_end`
    // is sufficient and the import directory is walked separately via targeted
    // reads against `file`.
    let Some(headers) = PeHeaders::parse(&header_buf) else {
        return ExeGraphicsInfo::new(Vec::new(), None);
    };

    let arch = architecture_from_machine(headers.machine());
    let (apis, dlls) = collect_graphics_apis(&headers, &mut file);
    ExeGraphicsInfo::new(apis, arch).with_graphics_dlls(dlls)
}

/// Walks both the static and delay-load import directories of `headers` against
/// `source`, returning the deduplicated set of imported graphics APIs in
/// first-seen order (no ranking — the orchestration layer applies policy). The
/// `source` abstraction lets the same walk serve the streaming
/// (`analyze_executable`, a [`File`]) and slice (`analyze_executable_bytes`, a
/// `&[u8]`) entry points from one implementation.
fn collect_graphics_apis<S: ByteSource>(
    headers: &PeHeaders<'_>,
    source: &mut S,
) -> (Vec<GraphicsApi>, Vec<String>) {
    let mut apis: Vec<GraphicsApi> = Vec::new();
    let mut dlls: Vec<String> = Vec::new();
    let mut classify = |name: &str| {
        if let Some(api) = classify_dll(name) {
            if !apis.contains(&api) {
                apis.push(api);
            }
            // `name` is already lowercased by `read_ascii_dll_name`; keep the
            // exact DLL so the proxy can be the one the game actually loads.
            if !dlls.iter().any(|existing| existing == name) {
                dlls.push(name.to_owned());
            }
        }
    };
    walk_import_dir(
        headers,
        source,
        IMPORT_DIRECTORY_INDEX,
        IMPORT_DESCRIPTOR_LEN,
        IMPORT_DESCRIPTOR_NAME_OFFSET,
        &mut classify,
    );
    walk_import_dir(
        headers,
        source,
        DELAY_DIRECTORY_INDEX,
        DELAY_DESCRIPTOR_LEN,
        DELAY_DESCRIPTOR_NAME_OFFSET,
        &mut classify,
    );
    (apis, dlls)
}

/// Reads just the PE header region (DOS stub + PE/COFF + optional header +
/// section table) from `file`, returning a buffer spanning `[0, header_end)`.
/// Returns `None` for anything that is not a well-formed PE or is truncated.
///
/// Two small probes (DOS header, then PE+COFF) establish the sizes; a third
/// read pulls exactly the header region. Total bytes read is a few KB for any
/// real executable regardless of its on-disk size.
fn read_header_region(file: &mut File) -> Option<Vec<u8>> {
    // DOS header: need at least the PE offset at 0x3c.
    let dos = file.read_exact_at(0, DOS_HEADER_PROBE_LEN)?;
    if &dos[..2] != b"MZ" {
        return None;
    }
    let pe_offset = usize::try_from(read_u32(&dos, DOS_PE_POINTER_OFFSET)?).ok()?;

    // PE signature + COFF header (4 + 20 bytes) at `pe_offset`.
    let coff_region_len = 4 + COFF_HEADER_LEN;
    let coff = file.read_exact_at(pe_offset, coff_region_len)?;
    if &coff[..4] != b"PE\0\0" {
        return None;
    }
    let section_count = usize::from(read_u16(&coff, 4 + 2)?);
    if section_count > MAX_SECTIONS {
        return None;
    }
    let optional_header_size = usize::from(read_u16(&coff, 4 + 16)?);
    let optional_header_offset = pe_offset + 4 + COFF_HEADER_LEN;
    let header_end = optional_header_offset
        .checked_add(optional_header_size)?
        .checked_add(section_count.checked_mul(SECTION_HEADER_LEN)?)?;

    file.read_exact_at(0, header_end)
}

/// A random-access byte source for the import-directory walk. Implemented for an
/// in-memory slice and a [`File`] so the descriptor/name reads are written once
/// and shared by the streaming ([`analyze_executable`]) and slice
/// ([`analyze_executable_bytes`]) entry points.
trait ByteSource {
    /// Reads exactly `len` bytes at `offset`, or `None` on an I/O error or a
    /// short read (truncated/EOF). A short read is treated as malformed rather
    /// than silently partial so callers never reason about a truncated region.
    fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>>;
    /// Reads up to `len` bytes at `offset`, returning whatever is available
    /// (possibly fewer, including empty at EOF). `None` only on an I/O error.
    /// Used for import name strings, which terminate at a NUL and may sit near
    /// the end where a strict full read would fail.
    fn read_at_most(&mut self, offset: usize, len: usize) -> Option<Vec<u8>>;
}

impl ByteSource for File {
    fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let start = u64::try_from(offset).ok()?;
        self.seek(SeekFrom::Start(start)).ok()?;
        let mut buf = vec![0u8; len];
        let mut filled = 0;
        while filled < len {
            let n = self.read(&mut buf[filled..]).ok()?;
            if n == 0 {
                return None;
            }
            filled += n;
        }
        Some(buf)
    }

    fn read_at_most(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let start = u64::try_from(offset).ok()?;
        self.seek(SeekFrom::Start(start)).ok()?;
        let mut buf = vec![0u8; len];
        let mut filled = 0;
        while filled < len {
            let n = self.read(&mut buf[filled..]).ok()?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        buf.truncate(filled);
        Some(buf)
    }
}

impl ByteSource for &[u8] {
    fn read_exact_at(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let end = offset.checked_add(len)?;
        self.get(offset..end).map(<[u8]>::to_vec)
    }

    fn read_at_most(&mut self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let start = offset.min(self.len());
        let end = start.saturating_add(len).min(self.len());
        Some(self[start..end].to_vec())
    }
}

/// Walks one import directory (static or delay-load) from `source` on demand,
/// invoking `f` with each imported DLL name. Reads only the descriptor table and
/// each name string via targeted reads, so a file source never loads the body
/// whole. Bounded by [`MAX_IMPORT_DESCRIPTORS`] and the directory's declared
/// size, and null-terminated by an all-zero descriptor.
fn walk_import_dir<S: ByteSource>(
    headers: &PeHeaders<'_>,
    source: &mut S,
    dir_index: usize,
    descriptor_len: usize,
    name_field_offset: usize,
    f: &mut dyn FnMut(&str),
) {
    let Some((dir_rva, dir_size)) = headers.data_directory(dir_index) else {
        return;
    };
    if dir_rva == 0 || dir_size == 0 {
        return;
    }
    let Some(start) = rva_to_offset(headers.sections(), dir_rva) else {
        return;
    };
    let dir_size_us = usize::try_from(dir_size).unwrap_or(usize::MAX);
    let dir_end = start.saturating_add(dir_size_us);

    let mut offset = start;
    for _ in 0..MAX_IMPORT_DESCRIPTORS {
        let Some(descriptor_end) = offset.checked_add(descriptor_len) else {
            break;
        };
        if descriptor_end > dir_end {
            break;
        }
        let Some(desc) = source.read_exact_at(offset, descriptor_len) else {
            break;
        };
        let first_field = read_u32(&desc, 0).unwrap_or(0);
        let name_rva = read_u32(&desc, name_field_offset).unwrap_or(0);
        // An all-zero descriptor (first field and name RVA both zero) terminates
        // the table — true for both the static (OriginalFirstThunk @0, name @12)
        // and delay-load (Attributes @0, name @4) descriptor layouts.
        if first_field == 0 && name_rva == 0 {
            break;
        }
        if name_rva != 0 {
            if let Some(name_offset) = rva_to_offset(headers.sections(), name_rva) {
                // Read up to MAX_DLL_NAME_LEN bytes; a name near the end of the
                // source may be truncated, which `read_ascii_dll_name` handles
                // (it stops at NUL or buffer end). A strict full read would
                // wrongly drop such a name.
                if let Some(chunk) = source.read_at_most(name_offset, MAX_DLL_NAME_LEN) {
                    if let Some(name) = read_ascii_dll_name(&chunk, 0) {
                        f(&name);
                    }
                }
            }
        }
        offset = descriptor_end;
    }
}

/// Maps a COFF machine type to a [`Architecture`].
pub(super) fn architecture_from_machine(machine: u16) -> Option<Architecture> {
    match machine {
        MACHINE_I386 => Some(Architecture::X86),
        MACHINE_AMD64 => Some(Architecture::X64),
        _ => None,
    }
}

/// Bytes read for the initial DOS header probe: enough to cover `MZ` and the
/// PE offset at `0x3c` for any standard PE.
const DOS_HEADER_PROBE_LEN: usize = 64;

/// Analyzes raw executable `bytes`, returning the detected graphics API set and
/// architecture on a best-effort basis. Never fails: anything unparseable
/// resolves to an empty API set / `None`.
#[must_use]
pub fn analyze_executable_bytes(bytes: &[u8]) -> ExeGraphicsInfo {
    let Some(headers) = PeHeaders::parse(bytes) else {
        return ExeGraphicsInfo::new(Vec::new(), None);
    };
    let arch = architecture_from_machine(headers.machine());
    // The slice is both the header buffer and the byte source; the same walk
    // serves the file path in `analyze_executable`.
    let mut source = bytes;
    let (apis, dlls) = collect_graphics_apis(&headers, &mut source);
    ExeGraphicsInfo::new(apis, arch).with_graphics_dlls(dlls)
}

fn read_ascii_dll_name(bytes: &[u8], offset: usize) -> Option<String> {
    let mut name = String::new();
    for index in 0..MAX_DLL_NAME_LEN {
        let byte = *bytes.get(offset.checked_add(index)?)?;
        if byte == 0 {
            return (!name.is_empty()).then(|| name.to_ascii_lowercase());
        }
        // Windows import names are conventionally ASCII. Rejecting non-ASCII
        // bytes keeps the classifier simple; a malformed name here simply means
        // we cannot reason about that import.
        if !byte.is_ascii() {
            return None;
        }
        name.push(byte as char);
    }
    None
}

/// Maps an imported DLL name to the graphics API it implies, or `None` when the
/// DLL is not a known graphics library.
///
/// `dxgi.dll` alone (no specific d3dXX import) indicates a modern DirectX game
/// that loads its device DLL dynamically. The exact 10/11/12 version is not
/// knowable from imports, but all three share the `dxgi.dll` proxy, so `D3D11`
/// is a faithful representative for proxy selection. When a `d3dXX.dll` is also
/// imported, the set contains both and the orchestration policy picks the
/// higher version.
fn classify_dll(name: &str) -> Option<GraphicsApi> {
    match name {
        "d3d12.dll" => Some(GraphicsApi::D3D12),
        "d3d11.dll" => Some(GraphicsApi::D3D11),
        "d3d10.dll" | "d3d10_1.dll" | "d3d10core.dll" => Some(GraphicsApi::D3D10),
        "d3d9.dll" => Some(GraphicsApi::D3D9),
        "dxgi.dll" => Some(GraphicsApi::D3D11),
        "vulkan-1.dll" => Some(GraphicsApi::Vulkan),
        "opengl32.dll" => Some(GraphicsApi::OpenGl),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::header::{
        COFF_HEADER_LEN, DATA_DIRECTORY_ENTRY_LEN, DOS_PE_POINTER_OFFSET,
        PE32_DATA_DIRECTORY_OFFSET, PE32_MAGIC, PE32_PLUS_DATA_DIRECTORY_OFFSET, PE32_PLUS_MAGIC,
        SECTION_HEADER_LEN,
    };

    #[test]
    fn non_pe_bytes_resolve_to_empty_apis() {
        let info = analyze_executable_bytes(b"not a pe file");
        assert_eq!(info.apis(), &[]);
        assert_eq!(info.architecture(), None);
    }

    #[test]
    fn classify_dll_maps_known_graphics_libraries() {
        assert_eq!(classify_dll("d3d12.dll"), Some(GraphicsApi::D3D12));
        assert_eq!(classify_dll("d3d11.dll"), Some(GraphicsApi::D3D11));
        assert_eq!(classify_dll("d3d9.dll"), Some(GraphicsApi::D3D9));
        assert_eq!(classify_dll("dxgi.dll"), Some(GraphicsApi::D3D11));
        assert_eq!(classify_dll("vulkan-1.dll"), Some(GraphicsApi::Vulkan));
        assert_eq!(classify_dll("opengl32.dll"), Some(GraphicsApi::OpenGl));
        assert_eq!(classify_dll("kernel32.dll"), None);
    }

    #[test]
    fn parses_synthetic_d3d12_x64_executable() {
        let exe = build_pe(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &["KERNEL32.dll", "D3D12.dll"],
            &[],
        );
        let info = analyze_executable_bytes(&exe);
        assert_eq!(info.architecture(), Some(Architecture::X64));
        assert_eq!(info.apis(), &[GraphicsApi::D3D12]);
    }

    #[test]
    fn parses_synthetic_d3d9_x86_executable() {
        let exe = build_pe(MACHINE_I386, PE32_MAGIC, &["d3d9.dll"], &[]);
        let info = analyze_executable_bytes(&exe);
        assert_eq!(info.architecture(), Some(Architecture::X86));
        assert_eq!(info.apis(), &[GraphicsApi::D3D9]);
    }

    #[test]
    fn collects_full_api_set_without_ranking() {
        let exe = build_pe(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &["dxgi.dll", "d3d11.dll", "d3d12.dll", "vulkan-1.dll"],
            &[],
        );
        let apis = analyze_executable_bytes(&exe).apis().to_vec();
        // All four are present; no "DirectX wins" collapse happens in detection.
        assert!(apis.contains(&GraphicsApi::D3D11));
        assert!(apis.contains(&GraphicsApi::D3D12));
        assert!(apis.contains(&GraphicsApi::Vulkan));
        assert_eq!(apis.len(), 3); // dxgi→D3D11 deduplicates with d3d11.dll
    }

    #[test]
    fn unknown_machine_yields_no_architecture() {
        const MACHINE_ARM64: u16 = 0xaa64;
        let exe = build_pe(MACHINE_ARM64, PE32_PLUS_MAGIC, &["d3d11.dll"], &[]);
        assert_eq!(analyze_executable_bytes(&exe).architecture(), None);
        // The graphics API is still recoverable from the import table.
        assert_eq!(analyze_executable_bytes(&exe).apis(), &[GraphicsApi::D3D11]);
    }

    #[test]
    fn detects_d3d12_from_delay_load_only() {
        // Mirrors UE5 titles (e.g. Jedi Survivor): the static import table
        // carries only a DRM/wrapper DLL while `d3d12.dll` is delay-loaded.
        let exe = build_pe(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &["Core/Activation64.dll"],
            &["d3d12.dll"],
        );
        let info = analyze_executable_bytes(&exe);
        assert_eq!(info.architecture(), Some(Architecture::X64));
        assert_eq!(info.apis(), &[GraphicsApi::D3D12]);
    }

    #[test]
    fn detects_dxgi_from_delay_load_with_empty_static_imports() {
        let exe = build_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, &[], &["dxgi.dll"]);
        let info = analyze_executable_bytes(&exe);
        assert_eq!(info.apis(), &[GraphicsApi::D3D11]);
    }

    #[test]
    fn merges_static_and_delay_load_api_set_without_ranking() {
        // A static D3D11 import plus a delay-loaded D3D12 import yields both;
        // the orchestration policy (not detection) picks D3D12 as the target.
        let exe = build_pe(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &["d3d11.dll"],
            &["d3d12.dll", "vulkan-1.dll"],
        );
        let apis = analyze_executable_bytes(&exe).apis().to_vec();
        assert!(apis.contains(&GraphicsApi::D3D11));
        assert!(apis.contains(&GraphicsApi::D3D12));
        assert!(apis.contains(&GraphicsApi::Vulkan));
        assert_eq!(apis.len(), 3);
    }

    #[test]
    fn delay_load_directory_is_optional() {
        // A binary with no delay-load directory (the data directory entry is
        // zero) must still parse and report its static imports.
        let exe = build_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, &["d3d12.dll"], &[]);
        let info = analyze_executable_bytes(&exe);
        assert_eq!(info.apis(), &[GraphicsApi::D3D12]);
    }

    #[test]
    fn streaming_path_matches_slice_path_for_static_imports() {
        // The streaming `analyze_executable` must yield the same result as the
        // slice-based `analyze_executable_bytes` on the same bytes, proving the
        // targeted reads cover exactly what the in-memory parser reads.
        let exe = build_pe(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &["KERNEL32.dll", "D3D12.dll", "dxgi.dll"],
            &[],
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("game.exe");
        std::fs::write(&path, &exe).expect("write");

        let streamed = analyze_executable(&path);
        let sliced = analyze_executable_bytes(&exe);
        assert_eq!(streamed.architecture(), sliced.architecture());
        assert_eq!(streamed.apis(), sliced.apis());
    }

    #[test]
    fn streaming_path_matches_slice_path_with_delay_load() {
        // UE5-style: static wrapper only, `d3d12.dll` delay-loaded.
        let exe = build_pe(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &["Core/Activation64.dll"],
            &["d3d12.dll", "PhysX3_x64.dll"],
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("JediSurvivor.exe");
        std::fs::write(&path, &exe).expect("write");

        let streamed = analyze_executable(&path);
        let sliced = analyze_executable_bytes(&exe);
        assert_eq!(streamed.architecture(), Some(Architecture::X64));
        assert_eq!(streamed.apis(), sliced.apis());
        assert!(streamed.apis().contains(&GraphicsApi::D3D12));
    }

    #[test]
    fn streaming_path_handles_missing_file_silently() {
        let info = analyze_executable(std::path::Path::new(
            "renderpilot-definitely-not-here-91823.exe",
        ));
        assert_eq!(info.apis(), &[]);
        assert_eq!(info.architecture(), None);
    }

    #[test]
    fn streaming_path_rejects_non_pe_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notape.exe");
        std::fs::write(&path, b"not a real pe").expect("write");
        let info = analyze_executable(&path);
        assert_eq!(info.apis(), &[]);
        assert_eq!(info.architecture(), None);
    }

    /// Builds a minimal but structurally valid PE with one section that holds
    /// the static and delay-load import descriptors plus their DLL name
    /// strings, so the parser exercises real RVA→offset resolution for both
    /// directories. Layout: headers, then a single `.rdata` section containing
    /// the static descriptor table, the delay-load descriptor table, and the
    /// concatenated name strings.
    fn build_pe(
        machine: u16,
        optional_magic: u16,
        static_dll_names: &[&str],
        delay_dll_names: &[&str],
    ) -> Vec<u8> {
        let pe_offset: usize = 0x80;
        let optional_header_size: usize = 0xF0;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + COFF_HEADER_LEN;
        let optional_header_end = optional_header_offset + optional_header_size;
        let section_table_offset = optional_header_end;
        let headers_end = section_table_offset + SECTION_HEADER_LEN;

        // Section virtual + raw layout.
        let section_rva: u32 = 0x1000;
        let section_raw_ptr = align_up(headers_end as u32, 0x200);

        // Both descriptor tables (each with a trailing null terminator) live at
        // the start of the section, followed by the concatenated name strings.
        let static_descriptors_len = (static_dll_names.len() + 1) * IMPORT_DESCRIPTOR_LEN;
        let delay_descriptors_len = (delay_dll_names.len() + 1) * DELAY_DESCRIPTOR_LEN;
        let descriptors_len = static_descriptors_len + delay_descriptors_len;
        let names_base_rva = section_rva + descriptors_len as u32;

        let mut names_blob = Vec::new();
        let mut static_name_rvas = Vec::new();
        for name in static_dll_names {
            static_name_rvas.push(names_base_rva + names_blob.len() as u32);
            names_blob.extend_from_slice(name.as_bytes());
            names_blob.push(0);
        }
        let mut delay_name_rvas = Vec::new();
        for name in delay_dll_names {
            delay_name_rvas.push(names_base_rva + names_blob.len() as u32);
            names_blob.extend_from_slice(name.as_bytes());
            names_blob.push(0);
        }

        let mut section_body = vec![0u8; descriptors_len];

        // Static import descriptors: OriginalFirstThunk (offset 0) non-zero so
        // the descriptor is "live", name RVA at IMPORT_DESCRIPTOR_NAME_OFFSET.
        for (index, name_rva) in static_name_rvas.iter().enumerate() {
            let base = index * IMPORT_DESCRIPTOR_LEN;
            section_body[base..base + 4].copy_from_slice(&(section_rva).to_le_bytes());
            section_body
                [base + IMPORT_DESCRIPTOR_NAME_OFFSET..base + IMPORT_DESCRIPTOR_NAME_OFFSET + 4]
                .copy_from_slice(&name_rva.to_le_bytes());
        }

        // Delay-load descriptors sit right after the static table. Attributes
        // (offset 0) is set to 1 (RVA fields are RVAs) so the descriptor is
        // recognized as live; name RVA at DELAY_DESCRIPTOR_NAME_OFFSET.
        let delay_base = static_descriptors_len;
        for (index, name_rva) in delay_name_rvas.iter().enumerate() {
            let base = delay_base + index * DELAY_DESCRIPTOR_LEN;
            section_body[base..base + 4].copy_from_slice(&1u32.to_le_bytes());
            section_body
                [base + DELAY_DESCRIPTOR_NAME_OFFSET..base + DELAY_DESCRIPTOR_NAME_OFFSET + 4]
                .copy_from_slice(&name_rva.to_le_bytes());
        }

        section_body.extend_from_slice(&names_blob);

        let import_rva = section_rva;
        let import_size = static_descriptors_len as u32;
        let (delay_import_rva, delay_import_size) = if delay_dll_names.is_empty() {
            (0u32, 0u32)
        } else {
            (
                section_rva + static_descriptors_len as u32,
                delay_descriptors_len as u32,
            )
        };

        let total_len = section_raw_ptr as usize + section_body.len();
        let mut bytes = vec![0u8; total_len];

        bytes[0..2].copy_from_slice(b"MZ");
        bytes[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");

        // COFF header.
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&machine.to_le_bytes());
        bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes()); // 1 section
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());

        // Optional header magic + import data directories (index 1 static,
        // index 13 delay-load). The optional header is sized to fit all 16
        // standard data directory entries.
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&optional_magic.to_le_bytes());
        let data_dir_offset = optional_header_offset
            + if optional_magic == PE32_PLUS_MAGIC {
                PE32_PLUS_DATA_DIRECTORY_OFFSET
            } else {
                PE32_DATA_DIRECTORY_OFFSET
            };
        let import_entry = data_dir_offset + IMPORT_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
        bytes[import_entry..import_entry + 4].copy_from_slice(&import_rva.to_le_bytes());
        bytes[import_entry + 4..import_entry + 8].copy_from_slice(&import_size.to_le_bytes());

        let delay_entry = data_dir_offset + DELAY_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
        bytes[delay_entry..delay_entry + 4].copy_from_slice(&delay_import_rva.to_le_bytes());
        bytes[delay_entry + 4..delay_entry + 8].copy_from_slice(&delay_import_size.to_le_bytes());

        // Single section header.
        let so = section_table_offset;
        bytes[so..so + 8].copy_from_slice(b".rdata\0\0");
        bytes[so + 8..so + 12].copy_from_slice(&(section_body.len() as u32).to_le_bytes());
        bytes[so + 12..so + 16].copy_from_slice(&section_rva.to_le_bytes());
        bytes[so + 16..so + 20].copy_from_slice(&(section_body.len() as u32).to_le_bytes());
        bytes[so + 20..so + 24].copy_from_slice(&section_raw_ptr.to_le_bytes());

        bytes[section_raw_ptr as usize..].copy_from_slice(&section_body);
        bytes
    }

    fn align_up(value: u32, alignment: u32) -> u32 {
        value.div_ceil(alignment) * alignment
    }
}
