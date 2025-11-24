use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fmt;
use uuid::Uuid;

/// Represents the status of a requirement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequirementStatus {
    Draft,
    Approved,
    Completed,
    Rejected,
}

impl fmt::Display for RequirementStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequirementStatus::Draft => write!(f, "Draft"),
            RequirementStatus::Approved => write!(f, "Approved"),
            RequirementStatus::Completed => write!(f, "Completed"),
            RequirementStatus::Rejected => write!(f, "Rejected"),
        }
    }
}

/// Represents the priority of a requirement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequirementPriority {
    High,
    Medium,
    Low,
}

impl fmt::Display for RequirementPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequirementPriority::High => write!(f, "High"),
            RequirementPriority::Medium => write!(f, "Medium"),
            RequirementPriority::Low => write!(f, "Low"),
        }
    }
}

/// Represents the type of a requirement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequirementType {
    Functional,
    NonFunctional,
    System,
    User,
}

impl fmt::Display for RequirementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequirementType::Functional => write!(f, "Functional"),
            RequirementType::NonFunctional => write!(f, "Non-Functional"),
            RequirementType::System => write!(f, "System"),
            RequirementType::User => write!(f, "User"),
        }
    }
}

/// Represents a single requirement in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    /// Unique identifier for the requirement (UUID)
    pub id: Uuid,

    /// Human-friendly specification ID (e.g., "SPEC-001")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_id: Option<String>,

    /// Short title describing the requirement
    pub title: String,

    /// Detailed description of the requirement
    pub description: String,

    /// Current status of the requirement
    pub status: RequirementStatus,

    /// Priority level of the requirement
    pub priority: RequirementPriority,

    /// Person responsible for the requirement
    pub owner: String,

    /// The feature this requirement belongs to
    pub feature: String,

    /// When the requirement was created
    pub created_at: DateTime<Utc>,

    /// When the requirement was last modified
    pub modified_at: DateTime<Utc>,

    /// Type of the requirement
    pub req_type: RequirementType,

    /// IDs of requirements this requirement depends on
    pub dependencies: Vec<Uuid>,

    /// Tags for categorizing the requirement
    pub tags: HashSet<String>,
}

impl Requirement {
    /// Creates a new requirement with the specified title and description
    pub fn new(title: String, description: String) -> Self {
        let now = Utc::now();

        // Get default feature name from environment variable
        let default_feature = env::var("REQ_FEATURE").unwrap_or_else(|_| String::from("Uncategorized"));

        Self {
            id: Uuid::new_v4(),
            spec_id: None, // Will be assigned when added to store
            title,
            description,
            status: RequirementStatus::Draft,
            priority: RequirementPriority::Medium,
            owner: String::new(),
            feature: default_feature,
            created_at: now,
            modified_at: now,
            req_type: RequirementType::Functional,
            dependencies: Vec::new(),
            tags: HashSet::new(),
        }
    }
}

/// Collection of all requirements
#[derive(Debug, Serialize, Deserialize)]
pub struct RequirementsStore {
    pub requirements: Vec<Requirement>,
    #[serde(default = "default_next_feature_number")]
    pub next_feature_number: u32,
    #[serde(default = "default_next_spec_number")]
    pub next_spec_number: u32,
}

/// Default value for next_feature_number
fn default_next_feature_number() -> u32 {
    1
}

/// Default value for next_spec_number
fn default_next_spec_number() -> u32 {
    1
}

impl RequirementsStore {
    /// Creates an empty requirements store
    pub fn new() -> Self {
        Self {
            requirements: Vec::new(),
            next_feature_number: 1,
            next_spec_number: 1,
        }
    }

    /// Adds a requirement to the store
    pub fn add_requirement(&mut self, req: Requirement) {
        self.requirements.push(req);
    }

    /// Gets a requirement by ID
    pub fn get_requirement_by_id(&self, id: &Uuid) -> Option<&Requirement> {
        self.requirements.iter().find(|r| r.id == *id)
    }

    /// Gets a mutable reference to a requirement by ID
    pub fn get_requirement_by_id_mut(&mut self, id: &Uuid) -> Option<&mut Requirement> {
        self.requirements.iter_mut().find(|r| r.id == *id)
    }

    /// Gets the next feature number and increments the counter
    pub fn get_next_feature_number(&mut self) -> u32 {
        let current_number = self.next_feature_number;
        self.next_feature_number += 1;
        current_number
    }

    /// Formats a feature with number prefix
    pub fn format_feature_with_number(&self, feature_name: &str) -> String {
        format!("{}-{}", self.next_feature_number, feature_name)
    }

    /// Gets all unique feature names
    pub fn get_feature_names(&self) -> Vec<String> {
        let mut feature_names = Vec::new();

        for req in &self.requirements {
            // Skip feature if it's already in the list
            if feature_names.contains(&req.feature) {
                continue;
            }

            feature_names.push(req.feature.clone());
        }

        // Sort features by their prefix number if they have one
        feature_names.sort_by(|a, b| {
            let a_parts: Vec<&str> = a.splitn(2, '-').collect();
            let b_parts: Vec<&str> = b.splitn(2, '-').collect();

            // If both have prefix numbers, compare them numerically
            if a_parts.len() > 1 && b_parts.len() > 1 {
                if let (Ok(a_num), Ok(b_num)) = (a_parts[0].parse::<u32>(), b_parts[0].parse::<u32>()) {
                    return a_num.cmp(&b_num);
                }
            }

            // Otherwise, lexicographical comparison
            a.cmp(b)
        });

        feature_names
    }

    /// Updates an existing feature name
    pub fn update_feature_name(&mut self, old_name: &str, new_name: &str) {
        for req in &mut self.requirements {
            if req.feature == old_name {
                req.feature = new_name.to_string();
            }
        }
    }

    /// Migrate existing features to use numbered prefixes
    pub fn migrate_features(&mut self) {
        // First, collect all unique features
        let mut unique_features: Vec<String> = Vec::new();

        for req in &self.requirements {
            // Skip if already has a number prefix (format: "1-Feature")
            if req.feature.contains('-') {
                if let Some((prefix, _)) = req.feature.split_once('-') {
                    if prefix.parse::<u32>().is_ok() {
                        continue; // Already has a number prefix
                    }
                }
            }

            if !unique_features.contains(&req.feature) {
                unique_features.push(req.feature.clone());
            }
        }

        // Assign numbers to each unique feature
        for feature in unique_features {
            let number = self.get_next_feature_number();
            let new_name = format!("{}-{}", number, feature);

            // Update all requirements with this feature
            self.update_feature_name(&feature, &new_name);
        }
    }

    /// Gets a requirement by SPEC-ID
    pub fn get_requirement_by_spec_id(&self, spec_id: &str) -> Option<&Requirement> {
        self.requirements.iter().find(|r| {
            r.spec_id.as_ref().map(|s| s.as_str()) == Some(spec_id)
        })
    }

    /// Gets a mutable reference to a requirement by SPEC-ID
    pub fn get_requirement_by_spec_id_mut(&mut self, spec_id: &str) -> Option<&mut Requirement> {
        self.requirements.iter_mut().find(|r| {
            r.spec_id.as_ref().map(|s| s.as_str()) == Some(spec_id)
        })
    }

    /// Assigns SPEC-IDs to requirements that don't have them
    pub fn assign_spec_ids(&mut self) {
        for req in &mut self.requirements {
            if req.spec_id.is_none() {
                req.spec_id = Some(format!("SPEC-{:03}", self.next_spec_number));
                self.next_spec_number += 1;
            }
        }
    }

    /// Gets the next SPEC-ID that would be assigned
    pub fn peek_next_spec_id(&self) -> String {
        format!("SPEC-{:03}", self.next_spec_number)
    }

    /// Validates that all SPEC-IDs are unique
    pub fn validate_unique_spec_ids(&self) -> anyhow::Result<()> {
        use std::collections::HashSet;
        let mut seen = HashSet::new();

        for req in &self.requirements {
            if let Some(spec_id) = &req.spec_id {
                if !seen.insert(spec_id) {
                    anyhow::bail!("Duplicate SPEC-ID found: {}", spec_id);
                }
            }
        }

        Ok(())
    }

    /// Adds a requirement and assigns it a SPEC-ID
    pub fn add_requirement_with_spec_id(&mut self, mut req: Requirement) {
        if req.spec_id.is_none() {
            req.spec_id = Some(format!("SPEC-{:03}", self.next_spec_number));
            self.next_spec_number += 1;
        }
        self.requirements.push(req);
    }
}

impl Default for RequirementsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_requirement_with_spec_id() {
        let mut store = RequirementsStore::new();
        let req = Requirement::new("Test".into(), "Description".into());

        assert_eq!(store.next_spec_number, 1);
        assert!(req.spec_id.is_none());

        store.add_requirement_with_spec_id(req);

        assert_eq!(store.requirements.len(), 1);
        assert_eq!(store.requirements[0].spec_id, Some("SPEC-001".into()));
        assert_eq!(store.next_spec_number, 2);
    }

    #[test]
    fn test_get_requirement_by_spec_id() {
        let mut store = RequirementsStore::new();
        let req = Requirement::new("Test".into(), "Description".into());
        store.add_requirement_with_spec_id(req);

        let found = store.get_requirement_by_spec_id("SPEC-001");
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Test");

        let not_found = store.get_requirement_by_spec_id("SPEC-999");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_assign_spec_ids() {
        let mut store = RequirementsStore::new();

        let mut req1 = Requirement::new("R1".into(), "D1".into());
        let mut req2 = Requirement::new("R2".into(), "D2".into());

        // Manually add without SPEC-IDs
        store.requirements.push(req1);
        store.requirements.push(req2);

        assert!(store.requirements[0].spec_id.is_none());
        assert!(store.requirements[1].spec_id.is_none());

        store.assign_spec_ids();

        assert_eq!(store.requirements[0].spec_id, Some("SPEC-001".into()));
        assert_eq!(store.requirements[1].spec_id, Some("SPEC-002".into()));
        assert_eq!(store.next_spec_number, 3);
    }

    #[test]
    fn test_assign_spec_ids_skips_existing() {
        let mut store = RequirementsStore::new();

        let mut req1 = Requirement::new("R1".into(), "D1".into());
        req1.spec_id = Some("SPEC-001".into());
        let mut req2 = Requirement::new("R2".into(), "D2".into());

        store.requirements.push(req1);
        store.requirements.push(req2);
        store.next_spec_number = 2; // Start at 2 since SPEC-001 exists

        store.assign_spec_ids();

        assert_eq!(store.requirements[0].spec_id, Some("SPEC-001".into()));
        assert_eq!(store.requirements[1].spec_id, Some("SPEC-002".into()));
        assert_eq!(store.next_spec_number, 3);
    }

    #[test]
    fn test_validate_unique_spec_ids_success() {
        let mut store = RequirementsStore::new();
        let req1 = Requirement::new("R1".into(), "D1".into());
        let req2 = Requirement::new("R2".into(), "D2".into());

        store.add_requirement_with_spec_id(req1);
        store.add_requirement_with_spec_id(req2);

        assert!(store.validate_unique_spec_ids().is_ok());
    }

    #[test]
    fn test_validate_unique_spec_ids_duplicate() {
        let mut store = RequirementsStore::new();

        let mut req1 = Requirement::new("R1".into(), "D1".into());
        req1.spec_id = Some("SPEC-001".into());
        let mut req2 = Requirement::new("R2".into(), "D2".into());
        req2.spec_id = Some("SPEC-001".into()); // Duplicate!

        store.requirements.push(req1);
        store.requirements.push(req2);

        let result = store.validate_unique_spec_ids();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate SPEC-ID"));
    }

    #[test]
    fn test_peek_next_spec_id() {
        let store = RequirementsStore::new();
        assert_eq!(store.peek_next_spec_id(), "SPEC-001");

        let mut store2 = RequirementsStore::new();
        store2.next_spec_number = 42;
        assert_eq!(store2.peek_next_spec_id(), "SPEC-042");
    }
}