use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    win32::object::handle::{
        RelativeDirectoryOpen, RelativeFileOpen, VerifiedDirectory, VerifiedFile,
        open_initial_file, open_relative_directory, open_relative_file,
    },
};

use super::{ObjectIdentity, PortableRoot};

#[derive(Debug)]
pub(crate) struct RawSupervisorImageObject(VerifiedFile);

impl RawSupervisorImageObject {
    pub(crate) fn identity(&self) -> &ObjectIdentity {
        self.0.identity()
    }
    pub(crate) fn read_all(&mut self) -> Result<Vec<u8>> {
        self.0.read_all()
    }
}

#[derive(Debug)]
pub(crate) struct SelectedGenerationObject {
    generation: VerifiedDirectory,
    app: VerifiedFile,
}

impl SelectedGenerationObject {
    pub(crate) fn generation_identity(&self) -> &ObjectIdentity {
        self.generation.identity()
    }
    pub(crate) fn app_identity(&self) -> &ObjectIdentity {
        self.app.identity()
    }
    pub(crate) fn read_app(&mut self) -> Result<Vec<u8>> {
        self.app.read_all()
    }
}

pub(crate) fn open_raw_supervisor_image(
    root: &PortableRoot,
    name: &str,
) -> Result<RawSupervisorImageObject> {
    Ok(RawSupervisorImageObject(open_relative_file(
        &root.0,
        name,
        RelativeFileOpen::SharedRead,
    )?))
}

pub(crate) fn running_app_identity(path: &std::path::Path) -> Result<ObjectIdentity> {
    Ok(open_initial_file(path)?.into_identity())
}

pub(crate) fn open_selected_generation(
    root: &PortableRoot,
    generation_sha256: &str,
) -> Result<SelectedGenerationObject> {
    if !is_lower_hex_64(generation_sha256) {
        return Err(PortableRuntimeError::new(
            "portable_generation_contract",
            "selected generation name was not a canonical SHA-256 leaf",
        ));
    }
    let generations = open_relative_directory(
        &root.0,
        ".renderpilot-generations",
        RelativeDirectoryOpen::Traverse,
    )?;
    let version = open_relative_directory(&generations, "v1", RelativeDirectoryOpen::Traverse)?;
    let objects = open_relative_directory(&version, "objects", RelativeDirectoryOpen::Traverse)?;
    let generation =
        open_relative_directory(&objects, generation_sha256, RelativeDirectoryOpen::Traverse)?;
    let app = open_relative_file(
        &generation,
        "renderpilot-app.exe",
        RelativeFileOpen::SharedRead,
    )?;
    Ok(SelectedGenerationObject { generation, app })
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
