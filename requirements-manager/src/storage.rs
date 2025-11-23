use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::models::RequirementsStore;

/// Handles saving and loading requirements from disk
pub struct Storage {
    file_path: PathBuf,
}

impl Storage {
    /// Creates a new Storage instance
    pub fn new<P: AsRef<Path>>(file_path: P) -> Self {
        Self {
            file_path: file_path.as_ref().to_path_buf(),
        }
    }

    /// Loads requirements from the YAML file
    pub fn load(&self) -> Result<RequirementsStore> {
        // Create the file if it doesn't exist
        if !self.file_path.exists() {
            let parent = self
                .file_path
                .parent()
                .context("Failed to get parent directory")?;
            fs::create_dir_all(parent)?;
            let default_store = RequirementsStore::new();
            self.save(&default_store)?;
            return Ok(default_store);
        }

        // Open and read the file
        let file = File::open(&self.file_path)
            .with_context(|| format!("Failed to open file: {:?}", self.file_path))?;
        let reader = BufReader::new(file);

        // Parse the YAML content
        let mut store: crate::models::RequirementsStore = serde_yaml::from_reader(reader)
            .with_context(|| format!("Failed to parse YAML from {:?}", self.file_path))?;

        // Migrate existing features to numbered format if needed
        store.migrate_features();

        Ok(store)
    }

    /// Saves requirements to the YAML file
    pub fn save(&self, store: &RequirementsStore) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize and write to file
        let yaml = serde_yaml::to_string(store)?;
        fs::write(&self.file_path, yaml)?;

        Ok(())
    }
}