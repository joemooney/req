mod cli;
mod export;
mod models;
mod prompts;
mod project;
mod registry;
mod storage;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use std::collections::HashSet;
use uuid::Uuid;

use crate::cli::{Cli, Command, DbCommand, FeatureCommand};
use crate::models::{Requirement, RequirementPriority, RequirementStatus, RequirementType, RequirementsStore};
use crate::project::determine_requirements_path;
use crate::registry::{get_registry_path, Registry};
use crate::storage::Storage;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Determine which requirements file to use
    let requirements_path = determine_requirements_path(cli.project.as_deref())?;
    let storage = Storage::new(requirements_path.clone());

    match &cli.command {
        Command::Add { title, description, status, priority, r#type, owner, feature, tags, interactive } => {
            // Default to interactive mode if no specific arguments are provided
            let should_be_interactive = *interactive
                || (title.is_none() && description.is_none() && status.is_none()
                    && priority.is_none() && r#type.is_none() && owner.is_none()
                    && feature.is_none() && tags.is_none());

            if should_be_interactive {
                add_requirement_interactive(&storage)?;
            } else {
                add_requirement_cli(&storage, title, description, status, priority, r#type, owner, feature, tags)?;
            }
        }
        Command::List { status, priority, r#type, feature, tags } => {
            list_requirements(&storage, status, priority, r#type, feature, tags)?;
        }
        Command::Show { id } => {
            show_requirement(&storage, id)?;
        }
        Command::Edit { id } => {
            edit_requirement(&storage, id)?;
        }
        Command::Del { id, yes } => {
            delete_requirement(&storage, id, *yes)?;
        }
        Command::Feature(feature_cmd) => {
            handle_feature_command(feature_cmd, &storage)?;
        }
        Command::Db(db_cmd) => {
            handle_db_command(db_cmd, &requirements_path)?;
        }
        Command::Export { format, output } => {
            handle_export_command(&storage, format, output.as_deref())?;
        }
    }

    Ok(())
}

fn add_requirement_interactive(storage: &Storage) -> Result<()> {
    // Load existing requirements
    let mut store = storage.load()?;

    // Prompt user for requirement details
    let requirement = crate::prompts::prompt_new_requirement(&mut store)?;
    let id = requirement.id;

    // Add the requirement with auto-assigned SPEC-ID
    store.add_requirement_with_spec_id(requirement);
    storage.save(&store)?;

    // Get the added requirement to show SPEC-ID
    let added_req = store.get_requirement_by_id(&id).expect("Just added requirement");

    println!("{}", "Requirement added successfully!".green());
    println!("ID: {}", id);
    if let Some(spec_id) = &added_req.spec_id {
        println!("SPEC-ID: {}", spec_id.green());
    }

    Ok(())
}

fn add_requirement_cli(
    storage: &Storage,
    title: &Option<String>,
    description: &Option<String>,
    status_str: &Option<String>,
    priority_str: &Option<String>,
    type_str: &Option<String>,
    owner: &Option<String>,
    feature: &Option<String>,
    tags_str: &Option<String>,
) -> Result<()> {
    // Load existing requirements
    let mut store = storage.load()?;

    // Check required fields
    let title = match title {
        Some(t) => t.clone(),
        None => anyhow::bail!("Title is required. Use --title to specify a title."),
    };

    let description = match description {
        Some(d) => d.clone(),
        None => String::new(),
    };

    // Create a requirement with basic data
    let mut requirement = Requirement::new(title, description);

    // Set optional fields
    if let Some(status) = status_str {
        requirement.status = parse_status(status)?;
    }

    if let Some(priority) = priority_str {
        requirement.priority = parse_priority(priority)?;
    }

    if let Some(req_type) = type_str {
        requirement.req_type = parse_type(req_type)?;
    }

    if let Some(owner_val) = owner {
        requirement.owner = owner_val.clone();
    }

    if let Some(feature_val) = feature {
        requirement.feature = feature_val.clone();
    }

    if let Some(tags) = tags_str {
        let tag_set: HashSet<String> = tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        requirement.tags = tag_set;
    }

    let id = requirement.id;

    // Add the requirement with auto-assigned SPEC-ID
    store.add_requirement_with_spec_id(requirement);
    storage.save(&store)?;

    // Get the added requirement to show SPEC-ID
    let added_req = store.get_requirement_by_id(&id).expect("Just added requirement");

    println!("{}", "Requirement added successfully!".green());
    println!("ID: {}", id);
    if let Some(spec_id) = &added_req.spec_id {
        println!("SPEC-ID: {}", spec_id.green());
    }

    Ok(())
}

fn list_requirements(
    storage: &Storage,
    status: &Option<String>,
    priority: &Option<String>,
    req_type: &Option<String>,
    feature: &Option<String>,
    tags: &Option<String>,
) -> Result<()> {
    // Load requirements
    let store = storage.load()?;
    let mut requirements = store.requirements;

    // Apply filters if provided
    if let Some(status_str) = status {
        let status_filter = parse_status(status_str)?;
        requirements.retain(|r| r.status == status_filter);
    }

    if let Some(priority_str) = priority {
        let priority_filter = parse_priority(priority_str)?;
        requirements.retain(|r| r.priority == priority_filter);
    }

    if let Some(type_str) = req_type {
        let type_filter = parse_type(type_str)?;
        requirements.retain(|r| r.req_type == type_filter);
    }

    if let Some(feature_str) = feature {
        requirements.retain(|r| r.feature == *feature_str);
    }

    if let Some(tags_str) = tags {
        let tag_filters: Vec<String> = tags_str.split(',').map(|s| s.trim().to_string()).collect();
        requirements.retain(|r| tag_filters.iter().any(|tag| r.tags.contains(tag)));
    }

    // Display the requirements
    if requirements.is_empty() {
        println!("{}", "No requirements found.".yellow());
        return Ok(());
    }

    println!("{:<10} | {:<36} | {:<30} | {:<10} | {:<10} | {:<15}", "SPEC-ID", "UUID", "Title", "Status", "Priority", "Feature");
    println!("{}", "-".repeat(120));

    for req in requirements {
        let status_str = match req.status {
            RequirementStatus::Draft => "Draft".yellow(),
            RequirementStatus::Approved => "Approved".blue(),
            RequirementStatus::Completed => "Completed".green(),
            RequirementStatus::Rejected => "Rejected".red(),
        };

        let priority_str = match req.priority {
            RequirementPriority::High => "High".red(),
            RequirementPriority::Medium => "Medium".yellow(),
            RequirementPriority::Low => "Low".green(),
        };

        let spec_id_display = req.spec_id.as_ref()
            .map(|s| s.as_str())
            .unwrap_or("-");

        println!("{:<10} | {:<36} | {:<30} | {:<10} | {:<10} | {:<15}",
            spec_id_display,
            req.id.to_string(),
            req.title,
            status_str,
            priority_str,
            req.feature);
    }

    Ok(())
}

fn show_requirement(storage: &Storage, id_str: &str) -> Result<()> {
    // Load requirements first (needed for SPEC-ID lookup)
    let store = storage.load()?;

    // Parse UUID or SPEC-ID
    let id = parse_requirement_id(id_str, &store)?;

    // Find the specified requirement
    let req = store.get_requirement_by_id(&id)
        .context("Requirement not found")?;

    // Display the requirement details
    println!("{}: {}", "ID".blue(), req.id);
    if let Some(spec_id) = &req.spec_id {
        println!("{}: {}", "SPEC-ID".blue(), spec_id);
    }
    println!("{}: {}", "Title".blue(), req.title);
    println!("{}: {}", "Description".blue(), req.description);

    let status_str = match req.status {
        RequirementStatus::Draft => "Draft".yellow(),
        RequirementStatus::Approved => "Approved".blue(),
        RequirementStatus::Completed => "Completed".green(),
        RequirementStatus::Rejected => "Rejected".red(),
    };
    println!("{}: {}", "Status".blue(), status_str);

    let priority_str = match req.priority {
        RequirementPriority::High => "High".red(),
        RequirementPriority::Medium => "Medium".yellow(),
        RequirementPriority::Low => "Low".green(),
    };
    println!("{}: {}", "Priority".blue(), priority_str);

    let type_str = match req.req_type {
        RequirementType::Functional => "Functional",
        RequirementType::NonFunctional => "Non-Functional",
        RequirementType::System => "System",
        RequirementType::User => "User",
    };
    println!("{}: {}", "Type".blue(), type_str);

    println!("{}: {}", "Owner".blue(), req.owner);
    println!("{}: {}", "Feature".blue(), req.feature);
    println!("{}: {}", "Created".blue(), req.created_at);
    println!("{}: {}", "Modified".blue(), req.modified_at);

    if !req.tags.is_empty() {
        let tags_str = req.tags.iter().cloned().collect::<Vec<_>>().join(", ");
        println!("{}: {}", "Tags".blue(), tags_str);
    }

    if !req.dependencies.is_empty() {
        let deps_str = req.dependencies.iter()
            .map(|uuid| uuid.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("{}: {}", "Dependencies".blue(), deps_str);
    }

    Ok(())
}

fn edit_requirement(storage: &Storage, id_str: &str) -> Result<()> {
    // Load requirements first (needed for SPEC-ID lookup)
    let store_for_lookup = storage.load()?;

    // Parse UUID or SPEC-ID
    let id = parse_requirement_id(id_str, &store_for_lookup)?;

    // Load again as mutable
    let mut store = storage.load()?;

    // Find the specified requirement
    let req = store.get_requirement_by_id_mut(&id)
        .context("Requirement not found")?;

    // For simplicity, let's just update a few fields
    println!("Editing requirement: {}", req.title);
    println!("Leave field empty to keep current value");

    // Update title
    let title_prompt = format!("Title [{}]:", req.title);
    if let Ok(new_title) = inquire::Text::new(&title_prompt).prompt() {
        if !new_title.is_empty() {
            req.title = new_title;
        }
    }

    // Update description
    println!("Current description:");
    println!("{}", req.description);

    let description_prompt = "New description (leave empty to keep current):";
    if let Ok(new_description) = inquire::Editor::new(description_prompt)
        .with_predefined_text(&req.description)
        .prompt()
    {
        req.description = new_description;
    }

    // Update status
    let status_options = vec![
        RequirementStatus::Draft,
        RequirementStatus::Approved,
        RequirementStatus::Completed,
        RequirementStatus::Rejected,
    ];
    if let Ok(new_status) = inquire::Select::new("Status:", status_options).prompt() {
        req.status = new_status;
    }

    // Update priority
    let priority_options = vec![
        RequirementPriority::High,
        RequirementPriority::Medium,
        RequirementPriority::Low,
    ];
    if let Ok(new_priority) = inquire::Select::new("Priority:", priority_options).prompt() {
        req.priority = new_priority;
    }

    // Update owner
    let owner_prompt = format!("Owner [{}]:", req.owner);
    if let Ok(new_owner) = inquire::Text::new(&owner_prompt).prompt() {
        if !new_owner.is_empty() {
            req.owner = new_owner;
        }
    }

    // Update feature
    let feature_prompt = format!("Feature [{}]:", req.feature);
    if let Ok(new_feature) = inquire::Text::new(&feature_prompt).prompt() {
        if !new_feature.is_empty() {
            req.feature = new_feature;
        }
    }

    // Update modified time
    req.modified_at = chrono::Utc::now();

    // Save changes
    storage.save(&store)?;
    println!("{}", "Requirement updated successfully!".green());

    Ok(())
}

fn delete_requirement(storage: &Storage, id_str: &str, skip_confirm: bool) -> Result<()> {
    // Load requirements first (needed for SPEC-ID lookup)
    let store_for_lookup = storage.load()?;

    // Parse UUID or SPEC-ID
    let id = parse_requirement_id(id_str, &store_for_lookup)?;

    // Load again as mutable
    let mut store = storage.load()?;

    // Find the requirement to delete
    let req = store.get_requirement_by_id(&id)
        .context("Requirement not found")?;

    // Display requirement info
    println!("{}", "Requirement to delete:".yellow());
    println!("  ID: {}", req.id);
    if let Some(spec_id) = &req.spec_id {
        println!("  SPEC-ID: {}", spec_id);
    }
    println!("  Title: {}", req.title);
    println!("  Description: {}", req.description);

    // Confirm deletion unless --yes flag is used
    if !skip_confirm {
        let confirm = inquire::Confirm::new("Are you sure you want to delete this requirement?")
            .with_default(false)
            .prompt()?;

        if !confirm {
            println!("{}", "Deletion cancelled.".yellow());
            return Ok(());
        }
    }

    // Remove the requirement
    store.requirements.retain(|r| r.id != id);

    // Save changes
    storage.save(&store)?;
    println!("{}", "Requirement deleted successfully!".green());

    Ok(())
}

fn parse_uuid(id_str: &str) -> Result<Uuid> {
    Uuid::parse_str(id_str).with_context(|| format!("Invalid UUID: {}", id_str))
}

/// Parse requirement ID - accepts either UUID or SPEC-ID
fn parse_requirement_id(id_str: &str, store: &RequirementsStore) -> Result<Uuid> {
    // Try parsing as UUID first
    if let Ok(uuid) = Uuid::parse_str(id_str) {
        return Ok(uuid);
    }

    // Try as SPEC-ID
    if let Some(req) = store.get_requirement_by_spec_id(id_str) {
        return Ok(req.id);
    }

    anyhow::bail!("Invalid requirement ID: '{}'. Must be either a UUID or SPEC-ID (e.g., SPEC-001)", id_str)
}

fn parse_status(status_str: &str) -> Result<RequirementStatus> {
    match status_str.to_lowercase().as_str() {
        "draft" => Ok(RequirementStatus::Draft),
        "approved" => Ok(RequirementStatus::Approved),
        "completed" => Ok(RequirementStatus::Completed),
        "rejected" => Ok(RequirementStatus::Rejected),
        _ => anyhow::bail!("Invalid status: {}", status_str),
    }
}

fn parse_priority(priority_str: &str) -> Result<RequirementPriority> {
    match priority_str.to_lowercase().as_str() {
        "high" => Ok(RequirementPriority::High),
        "medium" => Ok(RequirementPriority::Medium),
        "low" => Ok(RequirementPriority::Low),
        _ => anyhow::bail!("Invalid priority: {}", priority_str),
    }
}

fn parse_type(type_str: &str) -> Result<RequirementType> {
    match type_str.to_lowercase().as_str() {
        "functional" => Ok(RequirementType::Functional),
        "non-functional" | "nonfunctional" => Ok(RequirementType::NonFunctional),
        "system" => Ok(RequirementType::System),
        "user" => Ok(RequirementType::User),
        _ => anyhow::bail!("Invalid requirement type: {}", type_str),
    }
}

/// Handle database management subcommands
fn handle_feature_command(cmd: &FeatureCommand, storage: &Storage) -> Result<()> {
    // Load existing requirements
    let mut store = storage.load()?;

    match cmd {
        FeatureCommand::Add { name, interactive } => {
            let should_be_interactive = *interactive || name.is_none();

            // Get feature name
            let feature_name = if should_be_interactive {
                // Use interactive prompting
                crate::prompts::prompt_new_feature(&mut store)?
            } else {
                // Use command line argument
                let name = name.clone().ok_or_else(|| anyhow::anyhow!("Feature name is required"))?;

                // Format with sequential number
                let next_number = store.get_next_feature_number();
                format!("{}-{}", next_number, name)
            };

            println!("{} Feature '{}' created successfully.", "✓".green(), feature_name);

            // Save the updated store to preserve the next feature number
            storage.save(&store)?;
        },
        FeatureCommand::List => {
            let features = store.get_feature_names();

            if features.is_empty() {
                println!("{}", "No features found.".yellow());
                return Ok(());
            }

            println!("{:<15} | {}", "Number", "Feature Name");
            println!("{}", "-".repeat(50));

            for feature in features {
                // Split feature by hyphen to get number and name
                if let Some((number, name)) = feature.split_once('-') {
                    if number.parse::<u32>().is_ok() {
                        println!("{:<15} | {}", number, name);
                    } else {
                        println!("{:<15} | {}", "N/A", feature);
                    }
                } else {
                    println!("{:<15} | {}", "N/A", feature);
                }
            }
        },
        FeatureCommand::Show { name } => {
            let features = store.get_feature_names();
            let mut found = false;

            for feature in features {
                if feature.contains(name) {
                    println!("{}: {}", "Feature".blue(), feature);

                    // Find requirements with this feature
                    println!("\n{}", "Requirements:".blue());
                    let requirements: Vec<&Requirement> = store.requirements.iter()
                        .filter(|r| r.feature == feature)
                        .collect();

                    if requirements.is_empty() {
                        println!("No requirements found with this feature.");
                    } else {
                        println!("{:<36} | {:<30} | {:<10} | {:<10}", "ID", "Title", "Status", "Priority");
                        println!("{}", "-".repeat(90));

                        for req in requirements {
                            let status_str = match req.status {
                                RequirementStatus::Draft => "Draft".yellow(),
                                RequirementStatus::Approved => "Approved".blue(),
                                RequirementStatus::Completed => "Completed".green(),
                                RequirementStatus::Rejected => "Rejected".red(),
                            };

                            let priority_str = match req.priority {
                                RequirementPriority::High => "High".red(),
                                RequirementPriority::Medium => "Medium".yellow(),
                                RequirementPriority::Low => "Low".green(),
                            };

                            println!("{:<36} | {:<30} | {:<10} | {:<10}",
                                req.id.to_string(),
                                req.title,
                                status_str,
                                priority_str);
                        }
                    }

                    found = true;
                    break;
                }
            }

            if !found {
                println!("{} Feature '{}' not found.", "!".yellow(), name);
            }
        },
        FeatureCommand::Edit { name, new_name, interactive } => {
            let features = store.get_feature_names();
            let mut found = false;

            // Find the feature
            for feature in features {
                if feature.contains(name) {
                    // Get new name for the feature
                    let new_feature_name = if *interactive || new_name.is_none() {
                        crate::prompts::prompt_edit_feature(&feature)?
                    } else {
                        // Use command line argument but preserve number prefix
                        let new_name = new_name.clone().unwrap();

                        if let Some((prefix, _)) = feature.split_once('-') {
                            if prefix.parse::<u32>().is_ok() {
                                format!("{}-{}", prefix, new_name)
                            } else {
                                new_name
                            }
                        } else {
                            new_name
                        }
                    };

                    // Update all requirements with this feature
                    store.update_feature_name(&feature, &new_feature_name);

                    // Save the updated store
                    storage.save(&store)?;

                    println!("{} Feature '{}' renamed to '{}'.", "✓".green(), feature, new_feature_name);
                    found = true;
                    break;
                }
            }

            if !found {
                println!("{} Feature '{}' not found.", "!".yellow(), name);
            }
        }
    }

    Ok(())
}

fn handle_db_command(cmd: &DbCommand, requirements_path: &std::path::PathBuf) -> Result<()> {
    match cmd {
        DbCommand::Register { name, path, description, default, interactive } => {
            // Get registry path
            let registry_path = get_registry_path()?;

            // Ensure registry exists
            if !registry_path.exists() {
                Registry::create_default(&registry_path)?;
            }

            // Load registry
            let mut registry = Registry::load(&registry_path)?;

            // Default to interactive mode if no specific arguments are provided or interactive flag is set
            let should_be_interactive = *interactive ||
                (name.is_none() && path.is_none() && description.is_none());

            // Project details to register
            let (project_name, project_path, project_description, is_default) = if should_be_interactive {
                // Use interactive prompting
                crate::prompts::prompt_register_project()?
            } else {
                // Use command line arguments
                let project_name = name.clone()
                    .ok_or_else(|| anyhow::anyhow!("Project name is required"))?;

                let project_path = path.clone()
                    .ok_or_else(|| anyhow::anyhow!("Project path is required"))?;

                let project_description = description.clone().unwrap_or_default();

                (project_name, project_path, project_description, *default)
            };

            // Register project
            registry.register_project(
                project_name.clone(),
                project_path.to_string_lossy().to_string(),
                project_description
            );

            // Set as default if requested
            if is_default {
                registry.set_default_project(&project_name)?;
            }

            // Save registry
            registry.save(&registry_path)?;

            println!("{} Project '{}' registered successfully.", "✓".green(), project_name);
            if is_default {
                println!("{} Project '{}' set as default.", "✓".green(), project_name);
            }
        },
        DbCommand::Path { name } => {
            // If a name is provided, try to get that specific project
            if let Some(project_name) = name {
                // Get registry path
                let registry_path = get_registry_path()?;

                // Ensure registry exists
                if !registry_path.exists() {
                    Registry::create_default(&registry_path)?;
                }

                // Load registry
                let registry = Registry::load(&registry_path)?;

                // Find the project
                if let Some(project) = registry.get_project(project_name) {
                    println!("{}", project.path);
                } else {
                    println!("{} Project '{}' not found in registry. Use 'req db register' to add it.",
                            "!".yellow(), project_name);
                }
            } else {
                // Use the already determined path
                println!("{}", requirements_path.display());
            }
        }
    }

    Ok(())
}

fn handle_export_command(storage: &Storage, format: &str, output: Option<&std::path::Path>) -> Result<()> {
    // Load requirements
    let store = storage.load()?;

    match format {
        "mapping" => {
            let output_path = output.map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from(".requirements-mapping.yaml"));
            export::generate_mapping_file(&store, &output_path)?;
        }
        "json" => {
            let output_path = output.map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("requirements.json"));
            export::export_json(&store, &output_path)?;
        }
        _ => {
            anyhow::bail!("Unknown export format: {}. Supported formats: mapping, json", format);
        }
    }

    Ok(())
}