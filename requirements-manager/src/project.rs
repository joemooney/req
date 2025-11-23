use anyhow::Result;
use std::env;
use std::path::PathBuf;

use crate::prompts;
use crate::registry::{get_registry_path, Registry};

/// Determines the requirements file path to use based on the available information
pub fn determine_requirements_path(project_option: Option<&str>) -> Result<PathBuf> {
    // Check if requirements.yaml exists in current directory - but only if we're not explicitly
    // specifying a project via command line option or environment variable
    let use_local_file = project_option.is_none() && env::var("REQ_DB_NAME").is_err();
    let current_dir_path = PathBuf::from("requirements.yaml");

    if use_local_file && current_dir_path.exists() {
        return Ok(current_dir_path);
    }

    // Get the registry path and ensure it exists
    let registry_path = get_registry_path()?;
    if !registry_path.exists() {
        Registry::create_default(&registry_path)?;
    }

    // Load the registry
    let registry = Registry::load(&registry_path)?;

    // Priority 1: Use the command line project option if provided
    if let Some(project_name) = project_option {
        if let Some(project) = registry.get_project(project_name) {
            return Ok(PathBuf::from(&project.path));
        } else {
            anyhow::bail!("Project '{}' not found in registry", project_name);
        }
    }

    // Priority 2: Use the REQ_DB_NAME environment variable if set
    if let Ok(env_project) = env::var("REQ_DB_NAME") {
        if let Some(project) = registry.get_project(&env_project) {
            return Ok(PathBuf::from(&project.path));
        } else {
            anyhow::bail!("Project '{}' from REQ_DB_NAME not found in registry", env_project);
        }
    }

    // Priority 3: Check if there's only one project in the registry
    if registry.projects.len() == 1 {
        let (_, project) = registry.projects.iter().next().unwrap();
        return Ok(PathBuf::from(&project.path));
    }

    // Priority 4: Use the default project if configured in registry
    if let Some((_, default_project)) = registry.get_default_project() {
        return Ok(PathBuf::from(&default_project.path));
    }

    // If we got here, we need to prompt the user to select a project
    let project_name = prompts::prompt_select_project()?;

    if let Some(project) = registry.get_project(&project_name) {
        return Ok(PathBuf::from(&project.path));
    }

    anyhow::bail!("Project selection failed")
}

/// Lists available projects from the registry
pub fn list_available_projects() -> Result<Vec<(String, String)>> {
    let registry_path = get_registry_path()?;
    if !registry_path.exists() {
        Registry::create_default(&registry_path)?;
    }

    let registry = Registry::load(&registry_path)?;
    let mut projects = Vec::new();

    for (name, project) in &registry.projects {
        projects.push((name.clone(), project.description.clone()));
    }

    Ok(projects)
}