use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::RequirementsStore;

/// Error type for storage operations
#[derive(Debug)]
pub enum StorageError {
    /// File is locked by another process
    FileLocked,
    /// Other IO error
    IoError(std::io::Error),
    /// Parse error
    ParseError(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::FileLocked => write!(f, "File is locked by another user/process"),
            StorageError::IoError(e) => write!(f, "IO error: {}", e),
            StorageError::ParseError(s) => write!(f, "Parse error: {}", s),
        }
    }
}

impl std::error::Error for StorageError {}

/// Handles saving and loading requirements from disk with file locking
/// for rudimentary multi-user support
pub struct Storage {
    file_path: PathBuf,
    lock_file_path: PathBuf,
}

impl Storage {
    /// Creates a new Storage instance
    pub fn new<P: AsRef<Path>>(file_path: P) -> Self {
        let file_path = file_path.as_ref().to_path_buf();
        let lock_file_path = file_path.with_extension("yaml.lock");
        Self {
            file_path,
            lock_file_path,
        }
    }

    /// Returns the path to the storage file
    pub fn path(&self) -> &Path {
        &self.file_path
    }

    /// Acquire an exclusive lock on the file for writing
    /// Returns the lock file handle which must be held during the operation
    fn acquire_write_lock(&self) -> Result<File> {
        // Create parent directories if needed
        if let Some(parent) = self.lock_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.lock_file_path)
            .with_context(|| format!("Failed to create lock file: {:?}", self.lock_file_path))?;

        // Try to acquire exclusive lock with timeout
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(5);

        loop {
            match lock_file.try_lock_exclusive() {
                Ok(()) => return Ok(lock_file),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > timeout {
                        anyhow::bail!(
                            "Timeout waiting for file lock - another user may be editing: {:?}",
                            self.file_path
                        );
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("Failed to acquire lock on {:?}", self.lock_file_path)
                    })
                }
            }
        }
    }

    /// Acquire a shared lock on the file for reading
    fn acquire_read_lock(&self) -> Result<Option<File>> {
        if !self.lock_file_path.exists() {
            return Ok(None);
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .open(&self.lock_file_path)
            .with_context(|| format!("Failed to open lock file: {:?}", self.lock_file_path))?;

        // Try to acquire shared lock with timeout
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(5);

        loop {
            match lock_file.try_lock_shared() {
                Ok(()) => return Ok(Some(lock_file)),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > timeout {
                        anyhow::bail!(
                            "Timeout waiting for file lock - another user may be editing: {:?}",
                            self.file_path
                        );
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("Failed to acquire lock on {:?}", self.lock_file_path)
                    })
                }
            }
        }
    }

    /// Loads requirements from the YAML file with file locking
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

        // Acquire shared lock for reading
        let _lock = self.acquire_read_lock()?;

        // Open and read the file
        let file = File::open(&self.file_path)
            .with_context(|| format!("Failed to open file: {:?}", self.file_path))?;
        let reader = BufReader::new(file);

        // Parse the YAML content
        let mut store: crate::models::RequirementsStore = serde_yaml::from_reader(reader)
            .with_context(|| format!("Failed to parse YAML from {:?}", self.file_path))?;

        // Migrate existing features to numbered format if needed
        store.migrate_features();

        // Assign SPEC-IDs to requirements that don't have them
        let had_missing_spec_ids = store.requirements.iter().any(|r| r.spec_id.is_none());
        store.assign_spec_ids();

        // Migrate existing users to have $USER-XXX spec_ids
        let had_missing_user_spec_ids = store.users.iter().any(|u| u.spec_id.is_none());
        store.migrate_users_to_spec_ids();

        // Drop read lock before acquiring write lock for migration save
        drop(_lock);

        // Save back if we assigned any SPEC-IDs (migration)
        if had_missing_spec_ids || had_missing_user_spec_ids {
            self.save(&store)?;
        }

        // Validate SPEC-ID uniqueness
        store.validate_unique_spec_ids()?;

        Ok(store)
    }

    /// Saves requirements to the YAML file with file locking
    pub fn save(&self, store: &RequirementsStore) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Acquire exclusive lock for writing
        let mut lock_file = self.acquire_write_lock()?;

        // Write lock holder info (optional, for debugging)
        let _ = writeln!(
            lock_file,
            "Locked by PID {} at {}",
            std::process::id(),
            chrono::Utc::now().to_rfc3339()
        );

        // Serialize and write to file
        let yaml = serde_yaml::to_string(store)?;
        fs::write(&self.file_path, yaml)?;

        // Lock is automatically released when lock_file is dropped
        Ok(())
    }

    /// Reload file from disk, detecting external changes
    /// Returns (store, changed) where changed indicates if the file was modified externally
    pub fn reload_if_changed(
        &self,
        current_store: &RequirementsStore,
    ) -> Result<(RequirementsStore, bool)> {
        let new_store = self.load()?;

        // Simple check: compare requirement counts and last modification
        // For more sophisticated detection, we could compare hashes
        let changed = new_store.requirements.len() != current_store.requirements.len()
            || new_store.users.len() != current_store.users.len()
            || new_store.features.len() != current_store.features.len();

        Ok((new_store, changed))
    }

    /// Perform an atomic update operation with proper locking
    /// This reloads the file, applies changes, and saves atomically
    pub fn update_atomically<F>(&self, update_fn: F) -> Result<RequirementsStore>
    where
        F: FnOnce(&mut RequirementsStore),
    {
        // Acquire exclusive lock
        let mut lock_file = self.acquire_write_lock()?;

        // Write lock holder info
        let _ = writeln!(
            lock_file,
            "Locked by PID {} at {}",
            std::process::id(),
            chrono::Utc::now().to_rfc3339()
        );

        // Load latest version from disk
        let file = File::open(&self.file_path)
            .with_context(|| format!("Failed to open file: {:?}", self.file_path))?;
        let reader = BufReader::new(file);
        let mut store: RequirementsStore = serde_yaml::from_reader(reader)
            .with_context(|| format!("Failed to parse YAML from {:?}", self.file_path))?;

        // Apply the update
        update_fn(&mut store);

        // Save back
        let yaml = serde_yaml::to_string(&store)?;
        fs::write(&self.file_path, yaml)?;

        // Lock is released when lock_file is dropped
        Ok(store)
    }
}
