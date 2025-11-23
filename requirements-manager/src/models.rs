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
    /// Unique identifier for the requirement
    pub id: Uuid,

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
}

/// Default value for next_feature_number
fn default_next_feature_number() -> u32 {
    1
}

impl RequirementsStore {
    /// Creates an empty requirements store
    pub fn new() -> Self {
        Self {
            requirements: Vec::new(),
            next_feature_number: 1,
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
}

impl Default for RequirementsStore {
    fn default() -> Self {
        Self::new()
    }
}