use super::super::*;

impl VerifiedDir {
    /// Acquire an absolute directory capability by walking every component
    /// without following links or reparse points.
    pub(crate) fn open_absolute_components(
        path: &Path,
        expected_identity: Option<&str>,
    ) -> Result<Self, ServiceError> {
        if !path.is_absolute() {
            return Err(crate::failed(format!(
                "directory authority path must be absolute: {}",
                path.display()
            )));
        }
        #[cfg(target_os = "linux")]
        {
            Self::open_linux(path, expected_identity)
        }
        #[cfg(windows)]
        {
            Self::open_windows(path, expected_identity)
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            feature = "development-host-fallback"
        ))]
        {
            if expected_identity.is_some() {
                return Err(crate::failed(
                    "development fallback cannot verify an expected directory identity",
                ));
            }
            let metadata = std::fs::symlink_metadata(path).map_err(|error| {
                crate::failed(format!(
                    "failed to inspect directory `{}`: {error}",
                    path.display()
                ))
            })?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(crate::failed("directory authority is not a real directory"));
            }
            return Ok(Self {
                metadata_path: path.to_owned(),
                identity: String::new(),
            });
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            not(feature = "development-host-fallback")
        ))]
        {
            let _ = (path, expected_identity);
            Err(crate::failed(
                "native directory authority is unsupported on this host",
            ))
        }
    }

    /// Compatibility entry point for generic atomic helpers. Relative paths
    /// are deliberately rejected: participant callers must acquire an
    /// absolute capability at their boundary.
    pub(crate) fn open(path: &Path) -> Result<Self, ServiceError> {
        Self::open_absolute_components(path, None)
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn metadata_path(&self) -> &Path {
        &self.metadata_path
    }

    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    /// Returns the native filesystem identity for this retained directory.
    ///
    /// A directory-entry identity is not sufficient for proving that an
    /// atomic rename can cross neither a device nor a volume. Callers that
    /// prepare a durable rename use this value for an explicit, fail-closed
    /// same-filesystem proof.
    pub(crate) fn filesystem_identity(&self) -> Result<String, ServiceError> {
        #[cfg(target_os = "linux")]
        {
            let metadata = rustix::fs::fstat(&self.fd).map_err(|error| {
                crate::failed(format!("failed to inspect directory filesystem: {error}"))
            })?;
            if metadata.st_dev == 0 {
                return Err(crate::failed("directory filesystem identity is unstable"));
            }
            Ok(format!("linux:{:x}", metadata.st_dev))
        }
        #[cfg(windows)]
        {
            let identity = windows_identity(&self.handle)?;
            let serial = identity
                .strip_prefix("windows:")
                .and_then(|value| value.split_once(':'))
                .map(|(serial, _)| serial)
                .filter(|serial| !serial.is_empty() && *serial != "0000000000000000")
                .ok_or_else(|| {
                    crate::failed("Windows directory filesystem identity is unstable")
                })?;
            Ok(format!("windows:{serial}"))
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            feature = "development-host-fallback"
        ))]
        {
            let _ = self;
            Err(crate::failed(
                "filesystem identity is unavailable on the development fallback host",
            ))
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            not(feature = "development-host-fallback")
        ))]
        {
            let _ = self;
            Err(crate::failed(
                "native filesystem identity is unsupported on this host",
            ))
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn as_fd(&self) -> &rustix::fd::OwnedFd {
        &self.fd
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn into_fd(self) -> rustix::fd::OwnedFd {
        self.fd
    }

    /// Open a child relative to this retained directory. The returned entry
    /// capability owns the native handle used for identity and digest.
    pub(crate) fn open_leaf(&self, name: &LeafName) -> Result<VerifiedEntry, ServiceError> {
        #[cfg(target_os = "linux")]
        {
            let fd = rustix::fs::openat(
                &self.fd,
                name.as_os_str(),
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::CLOEXEC
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK,
                rustix::fs::Mode::empty(),
            )
            .map_err(|error| crate::failed(format!("failed to open relative entry: {error}")))?;
            Ok(VerifiedEntry {
                metadata_path: self.metadata_path.join(name.as_os_str()),
                fd,
            })
        }
        #[cfg(windows)]
        {
            windows_open_entry_relative(&self.handle, name, WindowsOpenIntent::Observe)
                .map(|handle| VerifiedEntry {
                    metadata_path: self.metadata_path.join(name.as_os_str()),
                    handle,
                })
                .map_err(|error| crate::failed(format!("failed to open relative entry: {error}")))
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            feature = "development-host-fallback"
        ))]
        {
            let path = self.metadata_path.join(name.as_os_str());
            std::fs::symlink_metadata(&path).map_err(|error| crate::failed(error.to_string()))?;
            Ok(VerifiedEntry {
                metadata_path: path,
            })
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            not(feature = "development-host-fallback")
        ))]
        {
            let _ = name;
            Err(crate::failed(
                "native entry authority is unsupported on this host",
            ))
        }
    }

    /// Observes an endpoint below this already-retained root without following
    /// links or reparse points.  A missing descendant is represented by
    /// `None`; a missing root, a non-directory ancestor, or any unsafe
    /// traversal condition remains an error at the authority boundary.
    pub(crate) fn observe_descendant(
        &self,
        path: &Path,
    ) -> Result<Option<EntryObservation>, ServiceError> {
        let relative = path.strip_prefix(&self.metadata_path).map_err(|_| {
            crate::failed(format!(
                "authority descendant is outside retained root: {}",
                path.display()
            ))
        })?;
        let components = relative
            .components()
            .map(|component| match component {
                Component::Normal(value) => LeafName::parse(value),
                _ => Err(crate::failed(
                    "authority descendant is not a normal relative path",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if components.is_empty() {
            return Err(crate::failed(
                "authority descendant cannot be its retained root",
            ));
        }
        let mut parent = self.clone_capability()?;
        for (index, leaf) in components.iter().enumerate() {
            let Some(observed) = parent.observe_leaf(leaf)? else {
                return Ok(None);
            };
            if index + 1 == components.len() {
                return Ok(Some(observed));
            }
            if observed.kind != EntryKind::Directory {
                return Err(crate::failed(
                    "authority descendant has a non-directory ancestor",
                ));
            }
            parent = parent.open_leaf(leaf)?.into_observed_directory(&observed)?;
        }
        unreachable!("nonempty descendant component vector always returns")
    }

    pub(in crate::fs::authority) fn open_leaf_for_update(
        &self,
        name: &LeafName,
    ) -> Result<VerifiedEntry, ServiceError> {
        #[cfg(target_os = "linux")]
        {
            let fd = rustix::fs::openat(
                &self.fd,
                name.as_os_str(),
                rustix::fs::OFlags::RDWR
                    | rustix::fs::OFlags::CLOEXEC
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK,
                rustix::fs::Mode::empty(),
            )
            .map_err(|error| crate::failed(format!("failed to open file for update: {error}")))?;
            Ok(VerifiedEntry {
                metadata_path: self.metadata_path.join(name.as_os_str()),
                fd,
            })
        }
        #[cfg(windows)]
        {
            windows_open_entry_relative(&self.handle, name, WindowsOpenIntent::MutateEntry)
                .map(|handle| VerifiedEntry {
                    metadata_path: self.metadata_path.join(name.as_os_str()),
                    handle,
                })
                .map_err(|error| crate::failed(format!("failed to open file for update: {error}")))
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            feature = "development-host-fallback"
        ))]
        {
            let path = self.metadata_path.join(name.as_os_str());
            Ok(VerifiedEntry {
                metadata_path: path,
            })
        }
        #[cfg(all(
            not(any(target_os = "linux", windows)),
            not(feature = "development-host-fallback")
        ))]
        {
            let _ = name;
            Err(crate::failed(
                "native file update authority is unsupported on this host",
            ))
        }
    }
}

pub(crate) fn verified_parent(path: &Path) -> Result<(VerifiedDir, LeafName), ServiceError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.is_empty())
        .ok_or_else(|| {
            crate::failed(format!("path `{}` has no parent directory", path.display()))
        })?;
    Ok((VerifiedDir::open(parent)?, LeafName::from_path(path)?))
}
