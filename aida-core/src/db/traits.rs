//! Database abstraction traits
//!
//! This module defines the core trait that all storage backends must implement.

use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

use crate::models::{Requirement, RequirementsStore, User};

/// Types of database backends available
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// YAML file storage (single file)
    Yaml,
    /// SQLite database storage
    Sqlite,
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Yaml => write!(f, "YAML"),
            BackendType::Sqlite => write!(f, "SQLite"),
        }
    }
}

/// Configuration for database backends
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Path to the database file
    pub path: PathBuf,
    /// Backend type
    pub backend_type: BackendType,
    /// Whether to enable write-ahead logging (SQLite only)
    pub wal_mode: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("requirements.yaml"),
            backend_type: BackendType::Yaml,
            wal_mode: true,
        }
    }
}

/// Core trait for database backends
///
/// This trait provides a unified interface for storing and retrieving
/// requirements data, regardless of the underlying storage mechanism.
///
/// The design philosophy is:
/// - `load()` and `save()` work with the full `RequirementsStore` for compatibility
/// - Individual CRUD operations are provided for more efficient database access
/// - Backends can choose to implement efficient versions or delegate to load/save
pub trait DatabaseBackend: Send + Sync {
    /// Returns the backend type
    fn backend_type(&self) -> BackendType;

    /// Returns the path to the database file
    fn path(&self) -> &std::path::Path;

    // =========================================================================
    // Full Store Operations (for compatibility with existing code)
    // =========================================================================

    /// Loads the entire requirements store from the database
    fn load(&self) -> Result<RequirementsStore>;

    /// Saves the entire requirements store to the database
    fn save(&self, store: &RequirementsStore) -> Result<()>;

    /// Performs an atomic update operation
    /// Default implementation loads, applies changes, and saves
    fn update_atomically<F>(&self, update_fn: F) -> Result<RequirementsStore>
    where
        F: FnOnce(&mut RequirementsStore),
        Self: Sized,
    {
        let mut store = self.load()?;
        update_fn(&mut store);
        self.save(&store)?;
        Ok(store)
    }

    // =========================================================================
    // Requirement CRUD Operations
    // =========================================================================

    /// Gets a requirement by its UUID
    fn get_requirement(&self, id: &Uuid) -> Result<Option<Requirement>> {
        let store = self.load()?;
        Ok(store.requirements.iter().find(|r| &r.id == id).cloned())
    }

    /// Gets a requirement by its spec_id (e.g., "FR-001")
    fn get_requirement_by_spec_id(&self, spec_id: &str) -> Result<Option<Requirement>> {
        let store = self.load()?;
        Ok(store
            .requirements
            .iter()
            .find(|r| r.spec_id.as_deref() == Some(spec_id))
            .cloned())
    }

    /// Lists all requirements (non-archived by default)
    fn list_requirements(&self, include_archived: bool) -> Result<Vec<Requirement>> {
        let store = self.load()?;
        Ok(store
            .requirements
            .iter()
            .filter(|r| include_archived || !r.archived)
            .cloned()
            .collect())
    }

    /// Adds a new requirement
    /// Returns the requirement with assigned spec_id
    /// Note: This uses the simple SPEC-XXX format for ID generation.
    /// For more complex ID generation (with feature/type prefixes), use update_atomically
    fn add_requirement(&self, requirement: Requirement) -> Result<Requirement> {
        let mut store = self.load()?;
        let mut req = requirement;

        // Assign spec_id if not set using the simple format
        if req.spec_id.is_none() {
            req.spec_id = Some(format!("SPEC-{:03}", store.next_spec_number));
            store.next_spec_number += 1;
        }

        store.requirements.push(req.clone());
        self.save(&store)?;
        Ok(req)
    }

    /// Updates an existing requirement
    fn update_requirement(&self, requirement: &Requirement) -> Result<()> {
        let mut store = self.load()?;
        if let Some(pos) = store.requirements.iter().position(|r| r.id == requirement.id) {
            store.requirements[pos] = requirement.clone();
            self.save(&store)?;
            Ok(())
        } else {
            anyhow::bail!("Requirement not found: {}", requirement.id)
        }
    }

    /// Deletes a requirement by UUID
    fn delete_requirement(&self, id: &Uuid) -> Result<()> {
        let mut store = self.load()?;
        let original_len = store.requirements.len();
        store.requirements.retain(|r| &r.id != id);
        if store.requirements.len() == original_len {
            anyhow::bail!("Requirement not found: {}", id)
        }
        self.save(&store)
    }

    // =========================================================================
    // User CRUD Operations
    // =========================================================================

    /// Gets a user by UUID
    fn get_user(&self, id: &Uuid) -> Result<Option<User>> {
        let store = self.load()?;
        Ok(store.users.iter().find(|u| &u.id == id).cloned())
    }

    /// Gets a user by handle
    fn get_user_by_handle(&self, handle: &str) -> Result<Option<User>> {
        let store = self.load()?;
        Ok(store.users.iter().find(|u| u.handle == handle).cloned())
    }

    /// Lists all users
    fn list_users(&self, include_archived: bool) -> Result<Vec<User>> {
        let store = self.load()?;
        Ok(store
            .users
            .iter()
            .filter(|u| include_archived || !u.archived)
            .cloned()
            .collect())
    }

    /// Adds a new user
    fn add_user(&self, user: User) -> Result<User> {
        let mut store = self.load()?;
        let mut u = user;

        // Assign spec_id if not set
        if u.spec_id.is_none() {
            u.spec_id = Some(store.next_meta_id(crate::models::META_PREFIX_USER));
        }

        store.users.push(u.clone());
        self.save(&store)?;
        Ok(u)
    }

    /// Updates an existing user
    fn update_user(&self, user: &User) -> Result<()> {
        let mut store = self.load()?;
        if let Some(pos) = store.users.iter().position(|u| u.id == user.id) {
            store.users[pos] = user.clone();
            self.save(&store)?;
            Ok(())
        } else {
            anyhow::bail!("User not found: {}", user.id)
        }
    }

    /// Deletes a user by UUID
    fn delete_user(&self, id: &Uuid) -> Result<()> {
        let mut store = self.load()?;
        let original_len = store.users.len();
        store.users.retain(|u| &u.id != id);
        if store.users.len() == original_len {
            anyhow::bail!("User not found: {}", id)
        }
        self.save(&store)
    }

    // =========================================================================
    // Metadata Operations
    // =========================================================================

    /// Gets the database name
    fn get_name(&self) -> Result<String> {
        Ok(self.load()?.name)
    }

    /// Sets the database name
    fn set_name(&self, name: &str) -> Result<()> {
        let mut store = self.load()?;
        store.name = name.to_string();
        self.save(&store)
    }

    /// Gets the database title
    fn get_title(&self) -> Result<String> {
        Ok(self.load()?.title)
    }

    /// Sets the database title
    fn set_title(&self, title: &str) -> Result<()> {
        let mut store = self.load()?;
        store.title = title.to_string();
        self.save(&store)
    }

    /// Gets the database description
    fn get_description(&self) -> Result<String> {
        Ok(self.load()?.description)
    }

    /// Sets the database description
    fn set_description(&self, description: &str) -> Result<()> {
        let mut store = self.load()?;
        store.description = description.to_string();
        self.save(&store)
    }

    // =========================================================================
    // Utility Operations
    // =========================================================================

    /// Returns true if the database file exists
    fn exists(&self) -> bool {
        self.path().exists()
    }

    /// Creates the database with default/empty data if it doesn't exist
    fn create_if_not_exists(&self) -> Result<()> {
        if !self.exists() {
            self.save(&RequirementsStore::new())?;
        }
        Ok(())
    }

    /// Returns statistics about the database
    fn stats(&self) -> Result<DatabaseStats> {
        let store = self.load()?;
        Ok(DatabaseStats {
            requirement_count: store.requirements.len(),
            user_count: store.users.len(),
            feature_count: store.features.len(),
            backend_type: self.backend_type(),
        })
    }
}

/// Statistics about a database
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub requirement_count: usize,
    pub user_count: usize,
    pub feature_count: usize,
    pub backend_type: BackendType,
}
