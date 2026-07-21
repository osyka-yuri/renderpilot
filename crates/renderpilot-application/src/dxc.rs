//! Shared Microsoft DXC file-name invariants.

/// DXC compiler runtime file.
pub(crate) const COMPILER_FILE_NAME: &str = "dxcompiler.dll";
/// DXC validator runtime file.
pub(crate) const VALIDATOR_FILE_NAME: &str = "dxil.dll";
/// Complete file set distributed by the supported DXC package.
pub(crate) const PACKAGE_FILE_NAMES: [&str; 2] = [COMPILER_FILE_NAME, VALIDATOR_FILE_NAME];
