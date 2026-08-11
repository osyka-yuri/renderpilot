use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};

pub fn wide_nul(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

/// Encodes a filesystem path for raw Win32 APIs without inheriting the legacy
/// `MAX_PATH` boundary. Relative paths remain relative; absolute drive and UNC
/// paths use their canonical verbatim forms.
pub fn path_wide_nul(path: &Path) -> Vec<u16> {
    const SEPARATOR: u16 = b'\\' as u16;
    const VERBATIM_PREFIX: &[u16] = &[SEPARATOR, SEPARATOR, b'?' as u16, SEPARATOR];
    const DEVICE_PREFIX: &[u16] = &[SEPARATOR, SEPARATOR, b'.' as u16, SEPARATOR];
    const UNC_PREFIX: &[u16] = &[
        SEPARATOR,
        SEPARATOR,
        b'?' as u16,
        SEPARATOR,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        SEPARATOR,
    ];

    let encoded: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .map(|unit| if unit == b'/' as u16 { SEPARATOR } else { unit })
        .collect();
    let mut extended = if encoded.starts_with(VERBATIM_PREFIX)
        || encoded.starts_with(DEVICE_PREFIX)
        || !path.is_absolute()
    {
        encoded
    } else if encoded.starts_with(&[SEPARATOR, SEPARATOR]) {
        UNC_PREFIX
            .iter()
            .copied()
            .chain(encoded.into_iter().skip(2))
            .collect()
    } else {
        VERBATIM_PREFIX.iter().copied().chain(encoded).collect()
    };
    extended.push(0);
    extended
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(value: &[u16]) -> String {
        String::from_utf16(&value[..value.len() - 1]).expect("valid path UTF-16")
    }

    #[test]
    fn raw_filesystem_paths_use_verbatim_absolute_forms() {
        assert_eq!(
            decode(&path_wide_nul(Path::new(r"C:\portable/moved/app.exe"))),
            r"\\?\C:\portable\moved\app.exe"
        );
        assert_eq!(
            decode(&path_wide_nul(Path::new(r"\\server\share/moved/app.exe"))),
            r"\\?\UNC\server\share\moved\app.exe"
        );
        assert_eq!(
            decode(&path_wide_nul(Path::new(r"relative/moved/app.exe"))),
            r"relative\moved\app.exe"
        );
        assert_eq!(
            decode(&path_wide_nul(Path::new(r"\\?\C:\portable\app.exe"))),
            r"\\?\C:\portable\app.exe"
        );
    }
}
