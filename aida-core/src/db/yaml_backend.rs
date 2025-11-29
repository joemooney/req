//! YAML file storage backend
//!
//! This backend stores all data in a single YAML file, using the existing
//! Storage implementation with file locking support.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::RequirementsStore;
use crate::storage::Storage;
use super::traits::{BackendType, DatabaseBackend};

/// YAML file backend implementation
///
/// This wraps the existing Storage class to implement the DatabaseBackend trait,
/// providing compatibility with the existing codebase while enabling the new
/// abstraction layer.
pub struct YamlBackend {
    storage: Storage,
    path: PathBuf,
}

impl YamlBackend {
    /// Creates a new YAML backend for the given file path
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        Self {
            storage: Storage::new(&path),
            path,
        }
    }

    /// Gets a reference to the underlying Storage
    pub fn storage(&self) -> &Storage {
        &self.storage
    }
}

impl DatabaseBackend for YamlBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Yaml
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<RequirementsStore> {
        self.storage.load()
    }

    fn save(&self, store: &RequirementsStore) -> Result<()> {
        self.storage.save(store)
    }

    fn update_atomically<F>(&self, update_fn: F) -> Result<RequirementsStore>
    where
        F: FnOnce(&mut RequirementsStore),
    {
        self.storage.update_atomically(update_fn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_yaml_backend_create_and_load() {
        // Use a path that doesn't exist yet
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.yaml");
        let backend = YamlBackend::new(&file_path);

        // Should create file with empty store
        backend.create_if_not_exists().unwrap();

        let store = backend.load().unwrap();
        assert!(store.requirements.is_empty());
        assert!(store.users.is_empty());
    }

    #[test]
    fn test_yaml_backend_save_and_load() {
        let temp_file = NamedTempFile::new().unwrap();
        let backend = YamlBackend::new(temp_file.path());

        let mut store = RequirementsStore::new();
        store.name = "Test DB".to_string();
        store.title = "Test Database".to_string();

        backend.save(&store).unwrap();

        let loaded = backend.load().unwrap();
        assert_eq!(loaded.name, "Test DB");
        assert_eq!(loaded.title, "Test Database");
    }
}
