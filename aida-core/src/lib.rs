pub mod ai;
pub mod db;
pub mod export;
pub mod models;
pub mod project;
pub mod registry;
pub mod scaffolding;
pub mod storage;

// Re-export commonly used types
pub use ai::{
    AiClient, AiMode, BackgroundEvaluator, EvaluationResponse, EvaluationResult, EvaluatorConfig,
    EvaluatorStatus, IssueReport, StoredAiEvaluation, SuggestedImprovement,
};
pub use models::{
    default_reaction_definitions,
    default_type_definitions,
    // AI prompt configuration types
    AiActionPromptConfig,
    AiPromptConfig,
    AiTypePromptConfig,
    Cardinality,
    Comment,
    // Comment reaction types
    CommentReaction,
    CustomFieldDefinition,
    // Custom type definition types
    CustomFieldType,
    CustomTypeDefinition,
    FeatureDefinition,
    FieldChange,
    HistoryEntry,
    IdConfigValidation,
    IdConfiguration,
    // New ID system types
    IdFormat,
    NumberingStrategy,
    ReactionDefinition,
    Relationship,
    // Relationship definition types
    RelationshipDefinition,
    RelationshipType,
    RelationshipValidation,
    Requirement,
    RequirementPriority,
    RequirementStatus,
    RequirementType,
    RequirementTypeDefinition,
    RequirementsStore,
    // URL link type
    UrlLink,
    User,
    // Team type
    Team,
    META_PREFIX_FEATURE,
    // Meta-type prefixes
    META_PREFIX_USER,
    META_PREFIX_VIEW,
    META_PREFIX_TEAM,
};
pub use project::determine_requirements_path;
pub use registry::{get_config_dir, get_registry_path, get_templates_dir, Registry};
pub use scaffolding::{
    ProjectType, ScaffoldArtifact, ScaffoldConfig, ScaffoldError, ScaffoldPreview, Scaffolder,
};
pub use storage::{
    AddResult, ConflictInfo, ConflictResolution, EditLock, FieldConflict, LockFileInfo, SaveResult,
    SessionInfo, Storage, StorageError,
};
