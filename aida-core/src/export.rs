use crate::models::RequirementsStore;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct MappingFile {
    pub mappings: HashMap<String, String>, // UUID -> SPEC-ID
    pub next_spec_number: u32,
}

impl MappingFile {
    /// Load existing mapping file or create new
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let mapping: MappingFile = serde_yaml::from_str(&content)?;
            Ok(mapping)
        } else {
            Ok(MappingFile {
                mappings: HashMap::new(),
                next_spec_number: 1,
            })
        }
    }

    /// Save mapping file to disk
    pub fn save(&self, path: &Path) -> Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        fs::write(path, yaml)?;
        Ok(())
    }

    /// Get or create SPEC-ID for a UUID
    pub fn get_or_create_spec_id(&mut self, uuid: &str) -> String {
        if let Some(spec_id) = self.mappings.get(uuid) {
            spec_id.clone()
        } else {
            let spec_id = format!("SPEC-{:03}", self.next_spec_number);
            self.mappings.insert(uuid.to_string(), spec_id.clone());
            self.next_spec_number += 1;
            spec_id
        }
    }

    /// Get UUID for SPEC-ID (reverse lookup)
    pub fn get_uuid(&self, spec_id: &str) -> Option<String> {
        for (uuid, sid) in &self.mappings {
            if sid == spec_id {
                return Some(uuid.clone());
            }
        }
        None
    }
}

/// Generate mapping file (UUID -> SPEC-ID)
pub fn generate_mapping_file(store: &RequirementsStore, output_path: &Path) -> Result<()> {
    // Load existing mapping or create new
    let mut mapping = MappingFile::load_or_create(output_path)?;

    // Generate SPEC-IDs for all requirements
    for req in &store.requirements {
        let uuid = req.id.to_string();
        mapping.get_or_create_spec_id(&uuid);
    }

    // Save mapping
    mapping.save(output_path)?;

    println!("Generated mapping file: {}", output_path.display());
    println!("  Total mappings: {}", mapping.mappings.len());
    println!("  Next SPEC number: {}", mapping.next_spec_number);

    Ok(())
}

/// Export requirements to JSON format
pub fn export_json(store: &RequirementsStore, output_path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(store)?;
    fs::write(output_path, json)?;

    println!("Exported to JSON: {}", output_path.display());
    println!("  Total requirements: {}", store.requirements.len());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_mapping_file_new() {
        let mapping = MappingFile {
            mappings: HashMap::new(),
            next_spec_number: 1,
        };

        assert_eq!(mapping.mappings.len(), 0);
        assert_eq!(mapping.next_spec_number, 1);
    }

    #[test]
    fn test_get_or_create_spec_id_new() {
        let mut mapping = MappingFile {
            mappings: HashMap::new(),
            next_spec_number: 1,
        };

        let uuid = "f7d250bf-5b3e-4ec3-8bd5-2bee2c4b7bb9";
        let spec_id = mapping.get_or_create_spec_id(uuid);

        assert_eq!(spec_id, "SPEC-001");
        assert_eq!(mapping.next_spec_number, 2);
        assert_eq!(mapping.mappings.get(uuid), Some(&"SPEC-001".to_string()));
    }

    #[test]
    fn test_get_or_create_spec_id_existing() {
        let mut mappings = HashMap::new();
        mappings.insert(
            "f7d250bf-5b3e-4ec3-8bd5-2bee2c4b7bb9".to_string(),
            "SPEC-001".to_string(),
        );

        let mut mapping = MappingFile {
            mappings,
            next_spec_number: 2,
        };

        let uuid = "f7d250bf-5b3e-4ec3-8bd5-2bee2c4b7bb9";
        let spec_id = mapping.get_or_create_spec_id(uuid);

        assert_eq!(spec_id, "SPEC-001");
        assert_eq!(mapping.next_spec_number, 2); // Should not increment
    }

    #[test]
    fn test_get_uuid() {
        let mut mappings = HashMap::new();
        mappings.insert(
            "f7d250bf-5b3e-4ec3-8bd5-2bee2c4b7bb9".to_string(),
            "SPEC-001".to_string(),
        );

        let mapping = MappingFile {
            mappings,
            next_spec_number: 2,
        };

        let uuid = mapping.get_uuid("SPEC-001");
        assert_eq!(
            uuid,
            Some("f7d250bf-5b3e-4ec3-8bd5-2bee2c4b7bb9".to_string())
        );

        let uuid = mapping.get_uuid("SPEC-999");
        assert_eq!(uuid, None);
    }

    #[test]
    fn test_save_and_load() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("test-mapping.yaml");

        // Create and save
        let mut mapping = MappingFile {
            mappings: HashMap::new(),
            next_spec_number: 1,
        };
        mapping.get_or_create_spec_id("uuid-1");
        mapping.get_or_create_spec_id("uuid-2");
        mapping.save(&path)?;

        // Load and verify
        let loaded = MappingFile::load_or_create(&path)?;
        assert_eq!(loaded.mappings.len(), 2);
        assert_eq!(loaded.next_spec_number, 3);
        assert_eq!(loaded.mappings.get("uuid-1"), Some(&"SPEC-001".to_string()));
        assert_eq!(loaded.mappings.get("uuid-2"), Some(&"SPEC-002".to_string()));

        Ok(())
    }
}
