use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::PathRef;

const PATH_SEPARATOR: char = '/';
const WINDOWS_DRIVE_ROOT_LEN: usize = 3;

/// Persisted v1 identity of one physical installation root.
///
/// This representation is part of SQLite schema v14. Changing its byte output
/// requires a new schema migration rather than an edit to this algorithm.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstallKey(String);

impl InstallKey {
    /// Builds the schema-v14 comparison key for a normalized installation path.
    #[must_use]
    pub fn from_path(path: &PathRef) -> Self {
        Self(persisted_install_key_v1(path.as_str()))
    }

    /// Returns the persisted storage representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Pure lexical value object for a normalized installation root.
///
/// Filesystem canonicalization and junction resolution must happen in a
/// platform adapter before constructing this value.
#[derive(Debug, Clone)]
pub struct InstallRoot {
    path: PathRef,
    key: InstallKey,
}

impl InstallRoot {
    /// Creates an installation root from an already validated path reference.
    #[must_use]
    pub fn new(path: PathRef) -> Self {
        let key = InstallKey::from_path(&path);
        Self { path, key }
    }

    /// Returns the normalized path.
    pub const fn path(&self) -> &PathRef {
        &self.path
    }

    /// Returns this root's persisted v1 identity.
    #[must_use]
    pub const fn key(&self) -> &InstallKey {
        &self.key
    }

    /// Whether `path` is this root or a boundary-safe descendant.
    #[must_use]
    pub fn contains_path(&self, path: &PathRef) -> bool {
        key_contains(self.key.as_str(), InstallKey::from_path(path).as_str())
    }

    /// Whether `candidate` is this root or a boundary-safe descendant root.
    #[must_use]
    pub fn contains_root(&self, candidate: &Self) -> bool {
        key_contains(self.key.as_str(), candidate.key.as_str())
    }
}

impl PartialEq for InstallRoot {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for InstallRoot {}

impl Hash for InstallRoot {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl PartialOrd for InstallRoot {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InstallRoot {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl Serialize for InstallRoot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.path.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for InstallRoot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        PathRef::deserialize(deserializer).map(Self::new)
    }
}

fn persisted_install_key_v1(path: &str) -> String {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let mut key = if let Some(rest) = normalized.strip_prefix("//?/unc/") {
        format!("//{rest}")
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        rest.to_owned()
    } else {
        normalized
    };

    while has_redundant_trailing_separator(&key) {
        key.pop();
    }
    key
}

fn has_redundant_trailing_separator(path: &str) -> bool {
    path.len() > 1 && path.ends_with(PATH_SEPARATOR) && !is_windows_drive_root(path)
}

fn is_windows_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() == WINDOWS_DRIVE_ROOT_LEN
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'/'
}

fn key_contains(root: &str, candidate: &str) -> bool {
    if root == candidate {
        return true;
    }
    if !candidate.starts_with(root) {
        return false;
    }

    root.ends_with(PATH_SEPARATOR)
        || candidate
            .as_bytes()
            .get(root.len())
            .is_some_and(|separator| *separator == PATH_SEPARATOR as u8)
}

#[cfg(test)]
mod tests {
    use super::{InstallKey, InstallRoot};
    use crate::PathRef;

    fn root(path: &str) -> InstallRoot {
        InstallRoot::new(PathRef::new(path).expect("valid root"))
    }

    #[test]
    fn persisted_v1_key_covers_windows_path_spellings() {
        let cases = [
            (r"C:\Games\Example", "c:/games/example"),
            ("c:/Games/Example/", "c:/games/example"),
            (r"\\?\C:\Games\Example", "c:/games/example"),
            (r"\\?\UNC\Server\Share\Example\\", "//server/share/example"),
            (r"\\Server\Share\Example", "//server/share/example"),
            ("D:/", "d:/"),
            ("/", "/"),
        ];

        for (path, expected) in cases {
            assert_eq!(
                InstallKey::from_path(&PathRef::new(path).expect("valid path")).as_str(),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn root_equality_uses_persisted_identity() {
        assert_eq!(root(r"\\?\C:\Games\Example"), root("c:/games/example/"));
    }

    #[test]
    fn containment_is_boundary_safe() {
        let install = root("C:/Games/Game");

        assert!(install.contains_path(&PathRef::new("c:/games/game/bin/x.dll").unwrap()));
        assert!(!install.contains_path(&PathRef::new("C:/Games/GameExtra/x.dll").unwrap()));
        assert!(root("D:/").contains_path(&PathRef::new("d:/Games/Game/x.dll").unwrap()));
        assert!(root("//server/share").contains_root(&root("//SERVER/share/Game")));
        assert!(!root("//server/share").contains_root(&root("//server/share-extra/Game")));
    }
}
