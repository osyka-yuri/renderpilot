//! Retained raw and selected-generation image capabilities.

use super::{
    error::{PortableRuntimeError, Result},
    root_authority::PortableRootAuthority,
    win32::object::{
        ObjectIdentity, RawSupervisorImageObject, SelectedGenerationObject,
        open_raw_supervisor_image, open_selected_generation,
    },
};

#[derive(Debug)]
pub(crate) struct RawSupervisorImage {
    image: RawSupervisorImageObject,
}

impl RawSupervisorImage {
    /// The raw executable is opened as one fixed leaf relative to the retained
    /// root, so its identity and RPU bytes are object-bound together.
    pub(super) fn open(root: &PortableRootAuthority, raw_name: &str) -> Result<Self> {
        Ok(Self {
            image: open_raw_supervisor_image(root.object(), raw_name)?,
        })
    }

    pub(crate) fn identity(&self) -> &ObjectIdentity {
        self.image.identity()
    }

    pub(crate) fn rpu_bytes(&mut self) -> Result<Vec<u8>> {
        self.image.read_all()
    }
}

#[derive(Debug)]
pub(crate) struct RetainedAppImage {
    selected: SelectedGenerationObject,
}

impl RetainedAppImage {
    pub(crate) fn identity(&self) -> &ObjectIdentity {
        self.selected.app_identity()
    }

    pub(crate) fn read_all(&mut self) -> Result<Vec<u8>> {
        self.selected.read_app()
    }
}

#[derive(Debug)]
pub(crate) struct SelectedGenerationImage {
    app: RetainedAppImage,
}

impl SelectedGenerationImage {
    pub(super) fn open(root: &PortableRootAuthority, generation_sha256: &str) -> Result<Self> {
        if !is_lower_hex_64(generation_sha256) {
            return Err(PortableRuntimeError::new(
                "portable_generation_contract",
                "selected generation name was not a canonical SHA-256 leaf",
            ));
        }
        let selected = open_selected_generation(root.object(), generation_sha256)?;
        Ok(Self {
            app: RetainedAppImage { selected },
        })
    }

    pub(crate) fn generation_identity(&self) -> &ObjectIdentity {
        self.app.selected.generation_identity()
    }

    pub(crate) fn app(&self) -> &RetainedAppImage {
        &self.app
    }

    pub(crate) fn app_mut(&mut self) -> &mut RetainedAppImage {
        &mut self.app
    }
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
