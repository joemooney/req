pub mod models;
pub mod storage;
pub mod export;
pub mod registry;
pub mod project;

// Re-export commonly used types
pub use models::{
    Requirement, RequirementPriority, RequirementStatus, RequirementType,
    RequirementsStore, Relationship, RelationshipType, Comment,
    HistoryEntry, FieldChange, User,
    // New ID system types
    IdFormat, NumberingStrategy, IdConfiguration,
    RequirementTypeDefinition, FeatureDefinition, IdConfigValidation,
    // Relationship definition types
    RelationshipDefinition, Cardinality, RelationshipValidation,
    // Comment reaction types
    CommentReaction, ReactionDefinition, default_reaction_definitions,
    // Meta-type prefixes
    META_PREFIX_USER, META_PREFIX_VIEW, META_PREFIX_FEATURE,
    // Custom type definition types
    CustomFieldType, CustomFieldDefinition, CustomTypeDefinition, default_type_definitions,
};
pub use storage::Storage;
pub use registry::{Registry, get_registry_path};
pub use project::determine_requirements_path;
