use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about = "A simple requirements management system")]
pub struct Cli {
    /// Path to the requirements file
    #[clap(long, default_value = "requirements.yaml")]
    pub file: String,

    /// Project name to use from central registry
    #[clap(long, short = 'p')]
    pub project: Option<String>,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum DbCommand {
    /// Register a project in the registry
    Register {
        /// Name of the project
        #[clap(long)]
        name: Option<String>,

        /// Path to the requirements file
        #[clap(long)]
        path: Option<PathBuf>,

        /// Description of the project
        #[clap(long)]
        description: Option<String>,

        /// Set this project as the default
        #[clap(long)]
        default: bool,

        /// Use interactive mode (prompts)
        #[clap(long)]
        interactive: bool,
    },

    /// Print the path to the database YAML file
    Path {
        /// The name of the database to lookup
        #[clap(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FeatureCommand {
    /// Add a new feature
    Add {
        /// Name of the feature
        #[clap(long)]
        name: Option<String>,

        /// Use interactive mode (prompts)
        #[clap(long)]
        interactive: bool,
    },

    /// List all features
    List,

    /// Show details for a specific feature
    Show {
        /// The name or ID of the feature to show
        name: String,
    },

    /// Edit an existing feature
    Edit {
        /// The name or ID of the feature to edit
        name: String,

        /// New name for the feature
        #[clap(long)]
        new_name: Option<String>,

        /// Use interactive mode (prompts)
        #[clap(long)]
        interactive: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum RelationshipCommand {
    /// Add a relationship between requirements
    Add {
        /// Source requirement ID (UUID or SPEC-ID)
        #[clap(long)]
        from: String,

        /// Target requirement ID (UUID or SPEC-ID)
        #[clap(long)]
        to: String,

        /// Relationship type (parent, child, duplicate, verifies, verified-by, references, or custom)
        #[clap(long)]
        r#type: String,

        /// Create bidirectional relationship (adds inverse relationship automatically)
        #[clap(long, short = 'b')]
        bidirectional: bool,
    },

    /// Remove a relationship between requirements
    Remove {
        /// Source requirement ID (UUID or SPEC-ID)
        #[clap(long)]
        from: String,

        /// Target requirement ID (UUID or SPEC-ID)
        #[clap(long)]
        to: String,

        /// Relationship type
        #[clap(long)]
        r#type: String,

        /// Remove bidirectional relationship (removes inverse relationship too)
        #[clap(long, short = 'b')]
        bidirectional: bool,
    },

    /// List all relationships for a requirement
    List {
        /// Requirement ID (UUID or SPEC-ID)
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add a new requirement
    Add {
        /// Title of the requirement
        #[clap(long)]
        title: Option<String>,

        /// Description of the requirement
        #[clap(long)]
        description: Option<String>,

        /// Status of the requirement (draft, approved, completed, rejected)
        #[clap(long)]
        status: Option<String>,

        /// Priority of the requirement (high, medium, low)
        #[clap(long)]
        priority: Option<String>,

        /// Type of the requirement (functional, non-functional, system, user)
        #[clap(long)]
        r#type: Option<String>,

        /// Owner of the requirement
        #[clap(long)]
        owner: Option<String>,

        /// Feature the requirement belongs to (defaults to REQ_FEATURE env var or "Uncategorized")
        #[clap(long)]
        feature: Option<String>,

        /// Tags for the requirement (comma-separated)
        #[clap(long)]
        tags: Option<String>,

        /// Use interactive mode (prompts)
        #[clap(long)]
        interactive: bool,
    },

    /// List all requirements
    List {
        /// Filter by status
        #[clap(long)]
        status: Option<String>,

        /// Filter by priority
        #[clap(long)]
        priority: Option<String>,

        /// Filter by type
        #[clap(long)]
        r#type: Option<String>,

        /// Filter by feature
        #[clap(long)]
        feature: Option<String>,

        /// Filter by tags (comma separated)
        #[clap(long)]
        tags: Option<String>,
    },

    /// Show details for a specific requirement
    Show {
        /// The ID of the requirement to show
        id: String,
    },

    /// Edit an existing requirement
    Edit {
        /// The ID of the requirement to edit
        id: String,
    },

    /// Delete a requirement
    Del {
        /// The ID (UUID or SPEC-ID) of the requirement to delete
        id: String,

        /// Skip confirmation prompt
        #[clap(long, short = 'y')]
        yes: bool,
    },

    /// Feature management commands
    #[clap(subcommand)]
    Feature(FeatureCommand),

    /// Database management commands
    #[clap(subcommand)]
    Db(DbCommand),

    /// Relationship management commands
    #[clap(subcommand)]
    Rel(RelationshipCommand),

    /// Export requirements to different formats
    Export {
        /// Output format (mapping, json)
        #[clap(long, short = 'f', default_value = "mapping")]
        format: String,

        /// Output file path
        #[clap(long, short = 'o')]
        output: Option<PathBuf>,
    },
}