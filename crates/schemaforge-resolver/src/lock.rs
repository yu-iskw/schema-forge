//! Lock-file types: [`LockFile`] and [`LockEntry`].
//!
//! [`LockFile`] is serialised to `schemaforge.lock.toml` by the CLI and
//! records every externally resolved schema URI so that builds remain
//! reproducible.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A single entry in the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockEntry {
    /// The resolved absolute URI.
    pub uri: String,
    /// Hex-encoded SHA-256 digest of the serialised schema bytes.
    pub digest: String,
    /// Byte length of the serialised schema.
    pub size: usize,
}

/// The contents of a `schemaforge.lock.toml` file.
///
/// The lockfile records every externally resolved schema URI so that builds
/// remain reproducible.  It is human-readable TOML and is consumed by the CLI
/// lock workflow.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockFile {
    /// Ordered list of locked schema entries.
    #[serde(default)]
    pub entries: Vec<LockEntry>,
}

impl LockFile {
    /// Create an empty lock file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a lock entry.
    ///
    /// If an entry with the same URI already exists it is replaced.
    pub fn upsert(&mut self, entry: LockEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.uri == entry.uri) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Serialise the lock file to TOML.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] when serialisation fails.
    pub fn to_toml(&self) -> Result<String, std::io::Error> {
        toml::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Deserialise a lock file from TOML text.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] when the content is not valid TOML.
    pub fn from_toml(s: &str) -> Result<Self, std::io::Error> {
        toml::from_str(s).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Write the lock file to `path`.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] on IO or serialisation failure.
    pub fn write_to_path(&self, path: &Path) -> Result<(), std::io::Error> {
        let content = self.to_toml()?;
        std::fs::write(path, content)
    }

    /// Read a lock file from `path`.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] on IO or deserialisation failure.
    pub fn read_from_path(path: &Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockfile_roundtrip_toml() {
        let mut lf = LockFile::new();
        lf.upsert(LockEntry {
            uri: "https://example.com/schema.json".to_owned(),
            digest: "abc123".to_owned(),
            size: 42,
        });
        let toml = lf.to_toml().unwrap();
        let restored = LockFile::from_toml(&toml).unwrap();
        assert_eq!(lf, restored);
    }

    #[test]
    fn lockfile_upsert_replaces_existing() {
        let mut lf = LockFile::new();
        lf.upsert(LockEntry {
            uri: "https://example.com/s.json".to_owned(),
            digest: "old".to_owned(),
            size: 1,
        });
        lf.upsert(LockEntry {
            uri: "https://example.com/s.json".to_owned(),
            digest: "new".to_owned(),
            size: 2,
        });
        assert_eq!(lf.entries.len(), 1);
        assert_eq!(lf.entries[0].digest, "new");
    }

    #[test]
    fn lockfile_write_and_read_path() {
        let dir = std::env::temp_dir();
        let path = dir.join("schemaforge_test.lock.toml");
        let mut lf = LockFile::new();
        lf.upsert(LockEntry {
            uri: "https://example.com/schema.json".to_owned(),
            digest: "deadbeef".to_owned(),
            size: 100,
        });
        lf.write_to_path(&path).unwrap();
        let restored = LockFile::read_from_path(&path).unwrap();
        assert_eq!(lf, restored);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn lockfile_from_toml_invalid_returns_error() {
        let result = LockFile::from_toml("this is not toml!!! [[[");
        assert!(result.is_err());
    }
}
