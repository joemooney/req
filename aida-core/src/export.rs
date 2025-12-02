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

/// Export requirements specification (excludes IMPL tasks and implementation details)
pub fn export_requirements_spec(store: &RequirementsStore, output_path: &Path) -> Result<()> {
    let mut output = String::new();

    // Title
    let title = if !store.title.is_empty() {
        &store.title
    } else if !store.name.is_empty() {
        &store.name
    } else {
        "Requirements Specification"
    };
    output.push_str(&format!("# {}\n\n", title));

    if !store.description.is_empty() {
        output.push_str(&format!("{}\n\n", store.description));
    }

    // Group requirements by type, excluding IMPL
    let mut by_type: HashMap<String, Vec<&crate::models::Requirement>> = HashMap::new();

    for req in &store.requirements {
        // Skip IMPL tasks
        let spec_id = req.spec_id.as_deref().unwrap_or("");
        if spec_id.starts_with("IMPL-") {
            continue;
        }

        let type_name = format!("{:?}", req.req_type);
        by_type.entry(type_name).or_default().push(req);
    }

    // Sort types for consistent output
    let mut type_names: Vec<_> = by_type.keys().cloned().collect();
    type_names.sort();

    for type_name in type_names {
        if let Some(reqs) = by_type.get(&type_name) {
            output.push_str(&format!("## {} Requirements\n\n", type_name));

            let mut sorted_reqs = reqs.clone();
            sorted_reqs.sort_by(|a, b| a.spec_id.cmp(&b.spec_id));

            for req in sorted_reqs {
                let spec_id = req.spec_id.as_deref().unwrap_or("N/A");
                output.push_str(&format!("### {} - {}\n\n", spec_id, req.title));
                output.push_str(&format!("**Status:** {:?} | **Priority:** {:?}\n\n", req.status, req.priority));

                if !req.description.is_empty() {
                    output.push_str(&format!("{}\n\n", req.description));
                }

                // Show parent relationship if exists
                for rel in &req.relationships {
                    if rel.rel_type == crate::models::RelationshipType::Parent {
                        if let Some(parent) = store.requirements.iter().find(|r| r.id == rel.target_id) {
                            let parent_spec_id = parent.spec_id.as_deref().unwrap_or("N/A");
                            output.push_str(&format!("**Parent:** {} - {}\n\n", parent_spec_id, parent.title));
                        }
                    }
                }
            }
        }
    }

    fs::write(output_path, output)?;

    let req_count = store.requirements.iter()
        .filter(|r| !r.spec_id.as_deref().unwrap_or("").starts_with("IMPL-"))
        .count();

    println!("Exported requirements specification: {}", output_path.display());
    println!("  Total requirements: {} (excluding IMPL tasks)", req_count);

    Ok(())
}

/// Export implementation records (IMPL tasks only)
pub fn export_implementation_records(store: &RequirementsStore, output_path: &Path) -> Result<()> {
    let mut output = String::new();

    // Title
    let title = if !store.title.is_empty() {
        &store.title
    } else if !store.name.is_empty() {
        &store.name
    } else {
        "Project"
    };
    output.push_str(&format!("# {} - Implementation Records\n\n", title));
    output.push_str("This document contains implementation details and design records.\n\n");

    // Get all IMPL tasks, sorted
    let mut impl_tasks: Vec<_> = store.requirements.iter()
        .filter(|r| r.spec_id.as_deref().unwrap_or("").starts_with("IMPL-"))
        .collect();

    impl_tasks.sort_by(|a, b| a.spec_id.cmp(&b.spec_id));

    for req in &impl_tasks {
        let spec_id = req.spec_id.as_deref().unwrap_or("N/A");
        output.push_str(&format!("## {} - {}\n\n", spec_id, req.title));
        output.push_str(&format!("**Status:** {:?} | **Date:** {}\n\n",
            req.status,
            req.created_at.format("%Y-%m-%d")
        ));

        // Show parent requirement
        for rel in &req.relationships {
            if rel.rel_type == crate::models::RelationshipType::Parent {
                if let Some(parent) = store.requirements.iter().find(|r| r.id == rel.target_id) {
                    let parent_spec_id = parent.spec_id.as_deref().unwrap_or("N/A");
                    output.push_str(&format!("**Implements:** {} - {}\n\n", parent_spec_id, parent.title));
                }
            }
        }

        if !req.description.is_empty() {
            output.push_str(&format!("{}\n\n", req.description));
        }

        // Include custom fields (implementation_summary, files_changed, etc.)
        if !req.custom_fields.is_empty() {
            for (field_name, value) in &req.custom_fields {
                if !value.is_empty() {
                    let label = match field_name.as_str() {
                        "implementation_summary" => "Implementation Summary",
                        "files_changed" => "Files Changed",
                        "session_date" => "Session Date",
                        _ => field_name,
                    };
                    output.push_str(&format!("### {}\n\n{}\n\n", label, value));
                }
            }
        }

        output.push_str("---\n\n");
    }

    fs::write(output_path, output)?;

    println!("Exported implementation records: {}", output_path.display());
    println!("  Total IMPL tasks: {}", impl_tasks.len());

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
