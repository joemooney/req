// trace:FR-0152 | ai:claude:high
//! AI Project Scaffolding Module
//!
//! Provides functionality to generate Claude Code integration artifacts:
//! - CLAUDE.md project instructions
//! - .claude/commands/ directory with project-specific slash commands
//! - .claude/skills/ directory with requirements-driven development skills
//! - Code traceability configuration

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::RequirementsStore;

/// Configuration for what scaffolding artifacts to generate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldConfig {
    /// Generate CLAUDE.md project instructions
    pub generate_claude_md: bool,
    /// Generate .claude/commands/ directory with slash commands
    pub generate_commands: bool,
    /// Generate .claude/skills/ directory with skills
    pub generate_skills: bool,
    /// Include aida-req skill for requirement creation
    pub include_aida_req_skill: bool,
    /// Include aida-implement skill for requirement implementation
    pub include_aida_implement_skill: bool,
    /// Custom project type for specialized scaffolding
    pub project_type: ProjectType,
    /// Tech stack hints for context generation
    pub tech_stack: Vec<String>,
}

impl Default for ScaffoldConfig {
    fn default() -> Self {
        Self {
            generate_claude_md: true,
            generate_commands: true,
            generate_skills: true,
            include_aida_req_skill: true,
            include_aida_implement_skill: true,
            project_type: ProjectType::Generic,
            tech_stack: Vec::new(),
        }
    }
}

/// Project type for specialized scaffolding
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProjectType {
    #[default]
    Generic,
    Rust,
    Python,
    TypeScript,
    Web,
    Api,
    Cli,
}

impl ProjectType {
    /// Get all project types for UI selection
    pub fn all() -> &'static [ProjectType] {
        &[
            ProjectType::Generic,
            ProjectType::Rust,
            ProjectType::Python,
            ProjectType::TypeScript,
            ProjectType::Web,
            ProjectType::Api,
            ProjectType::Cli,
        ]
    }

    /// Get display label for the project type
    pub fn label(&self) -> &'static str {
        match self {
            ProjectType::Generic => "Generic",
            ProjectType::Rust => "Rust",
            ProjectType::Python => "Python",
            ProjectType::TypeScript => "TypeScript",
            ProjectType::Web => "Web Application",
            ProjectType::Api => "API/Backend",
            ProjectType::Cli => "CLI Tool",
        }
    }
}

/// Represents a scaffolding artifact to be generated
#[derive(Debug, Clone)]
pub struct ScaffoldArtifact {
    /// Relative path from project root
    pub path: PathBuf,
    /// Content of the artifact
    pub content: String,
    /// Description of what this artifact does
    pub description: String,
    /// Whether the file already exists
    pub exists: bool,
}

/// Result of scaffolding preview
#[derive(Debug, Clone)]
pub struct ScaffoldPreview {
    /// Artifacts to be generated
    pub artifacts: Vec<ScaffoldArtifact>,
    /// Files that would be overwritten
    pub overwrites: Vec<PathBuf>,
    /// New files that would be created
    pub new_files: Vec<PathBuf>,
    /// Directories that would be created
    pub new_dirs: Vec<PathBuf>,
}

/// Scaffolding generator
pub struct Scaffolder {
    /// Project root directory
    project_root: PathBuf,
    /// Scaffolding configuration
    config: ScaffoldConfig,
}

impl Scaffolder {
    /// Create a new scaffolder for the given project directory
    pub fn new(project_root: PathBuf, config: ScaffoldConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }

    /// Generate a preview of what would be scaffolded
    pub fn preview(&self, store: &RequirementsStore) -> ScaffoldPreview {
        let mut artifacts = Vec::new();
        let mut overwrites = Vec::new();
        let mut new_files = Vec::new();
        let mut new_dirs = HashSet::new();

        // CLAUDE.md
        if self.config.generate_claude_md {
            let path = PathBuf::from("CLAUDE.md");
            let full_path = self.project_root.join(&path);
            let exists = full_path.exists();
            let content = self.generate_claude_md(store);

            if exists {
                overwrites.push(path.clone());
            } else {
                new_files.push(path.clone());
            }

            artifacts.push(ScaffoldArtifact {
                path,
                content,
                description: "Project instructions for Claude Code".to_string(),
                exists,
            });
        }

        // .claude/commands/ directory
        if self.config.generate_commands {
            new_dirs.insert(PathBuf::from(".claude/commands"));

            // Add default commands
            let commands = self.generate_commands(store);
            for (name, content, desc) in commands {
                let path = PathBuf::from(format!(".claude/commands/{}.md", name));
                let full_path = self.project_root.join(&path);
                let exists = full_path.exists();

                if exists {
                    overwrites.push(path.clone());
                } else {
                    new_files.push(path.clone());
                }

                artifacts.push(ScaffoldArtifact {
                    path,
                    content,
                    description: desc,
                    exists,
                });
            }
        }

        // .claude/skills/ directory
        if self.config.generate_skills {
            new_dirs.insert(PathBuf::from(".claude/skills"));

            // Add aida-req skill
            if self.config.include_aida_req_skill {
                let path = PathBuf::from(".claude/skills/aida-req.md");
                let full_path = self.project_root.join(&path);
                let exists = full_path.exists();

                if exists {
                    overwrites.push(path.clone());
                } else {
                    new_files.push(path.clone());
                }

                artifacts.push(ScaffoldArtifact {
                    path,
                    content: self.generate_aida_req_skill(store),
                    description: "Skill for adding requirements with AI evaluation".to_string(),
                    exists,
                });
            }

            // Add aida-implement skill
            if self.config.include_aida_implement_skill {
                let path = PathBuf::from(".claude/skills/aida-implement.md");
                let full_path = self.project_root.join(&path);
                let exists = full_path.exists();

                if exists {
                    overwrites.push(path.clone());
                } else {
                    new_files.push(path.clone());
                }

                artifacts.push(ScaffoldArtifact {
                    path,
                    content: self.generate_aida_implement_skill(store),
                    description: "Skill for implementing requirements with traceability".to_string(),
                    exists,
                });
            }
        }

        // Filter new_dirs to only include those that don't exist
        let new_dirs: Vec<PathBuf> = new_dirs
            .into_iter()
            .filter(|d| !self.project_root.join(d).exists())
            .collect();

        ScaffoldPreview {
            artifacts,
            overwrites,
            new_files,
            new_dirs,
        }
    }

    /// Apply the scaffolding (write files)
    pub fn apply(&self, preview: &ScaffoldPreview) -> Result<Vec<PathBuf>, ScaffoldError> {
        let mut written_files = Vec::new();

        // Create directories first
        for dir in &preview.new_dirs {
            let full_path = self.project_root.join(dir);
            fs::create_dir_all(&full_path).map_err(|e| ScaffoldError::IoError {
                path: full_path.clone(),
                message: e.to_string(),
            })?;
        }

        // Also ensure parent directories exist for all artifacts
        for artifact in &preview.artifacts {
            if let Some(parent) = artifact.path.parent() {
                let full_parent = self.project_root.join(parent);
                if !full_parent.exists() {
                    fs::create_dir_all(&full_parent).map_err(|e| ScaffoldError::IoError {
                        path: full_parent.clone(),
                        message: e.to_string(),
                    })?;
                }
            }
        }

        // Write artifacts
        for artifact in &preview.artifacts {
            let full_path = self.project_root.join(&artifact.path);
            fs::write(&full_path, &artifact.content).map_err(|e| ScaffoldError::IoError {
                path: full_path.clone(),
                message: e.to_string(),
            })?;
            written_files.push(artifact.path.clone());
        }

        Ok(written_files)
    }

    /// Generate CLAUDE.md content
    fn generate_claude_md(&self, store: &RequirementsStore) -> String {
        let project_name = if !store.title.is_empty() {
            &store.title
        } else if !store.name.is_empty() {
            &store.name
        } else {
            "Project"
        };

        let description = if !store.description.is_empty() {
            format!("\n\n{}", store.description)
        } else {
            String::new()
        };

        let tech_stack_section = if !self.config.tech_stack.is_empty() {
            format!(
                "\n\n## Tech Stack\n\n{}",
                self.config.tech_stack
                    .iter()
                    .map(|t| format!("- {}", t))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            String::new()
        };

        let features_section = if !store.features.is_empty() {
            let features_list = store
                .features
                .iter()
                .map(|f| {
                    let prefix = if f.prefix.is_empty() { "N/A" } else { &f.prefix };
                    format!("- **{}** ({})", f.name, prefix)
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n\n## Features\n\n{}", features_list)
        } else {
            String::new()
        };

        let type_section = self.generate_type_specific_section();

        let traceability_section = r#"
## Code Traceability

When implementing requirements, add inline trace comments:

```rust
// trace:FR-0042 | ai:claude:high
fn implement_feature() {
    // Implementation
}
```

Format: `// trace:<SPEC-ID> | ai:<tool>:<confidence>`

Confidence levels:
- `high`: >80% AI-generated
- `med`: 40-80% AI with modifications
- `low`: <40% AI, mostly human
"#;

        let skills_section = r#"
## Claude Code Skills

This project uses AIDA requirements-driven development:

### /aida-req
Add new requirements with AI evaluation:
- Interactive requirement gathering
- Immediate database storage with draft status
- Background AI evaluation for quality feedback
- Follow-up actions: improve, split, link, accept

### /aida-implement
Implement requirements with traceability:
- Load and display requirement context
- Break down into child requirements as needed
- Update requirements during implementation
- Add inline traceability comments to code
"#;

        format!(
            r#"# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

{}{}{}{}{}{}{}
"#,
            project_name,
            description,
            tech_stack_section,
            features_section,
            type_section,
            traceability_section,
            if self.config.generate_skills {
                skills_section
            } else {
                ""
            }
        )
    }

    /// Generate type-specific sections based on project type
    fn generate_type_specific_section(&self) -> String {
        match self.config.project_type {
            ProjectType::Rust => r#"
## Common Commands

```bash
cargo build --workspace --release   # Build all crates
cargo test --workspace              # Run all tests
cargo check --workspace             # Quick syntax check
cargo clippy --workspace            # Linting
```
"#
            .to_string(),

            ProjectType::Python => r#"
## Common Commands

```bash
python -m venv venv                 # Create virtual environment
source venv/bin/activate            # Activate venv (Unix)
pip install -e ".[dev]"             # Install with dev dependencies
pytest                              # Run tests
black src tests                     # Format code
ruff check src tests                # Lint code
```
"#
            .to_string(),

            ProjectType::TypeScript => r#"
## Common Commands

```bash
npm install                         # Install dependencies
npm run build                       # Build project
npm test                            # Run tests
npm run lint                        # Lint code
npm run format                      # Format code
```
"#
            .to_string(),

            ProjectType::Web => r#"
## Common Commands

```bash
npm install                         # Install dependencies
npm run dev                         # Start development server
npm run build                       # Build for production
npm test                            # Run tests
```
"#
            .to_string(),

            ProjectType::Api => r#"
## Common Commands

```bash
# Start the API server
npm run dev                         # Development mode
npm start                           # Production mode

# Testing
npm test                            # Run tests
npm run test:integration            # Integration tests
```
"#
            .to_string(),

            ProjectType::Cli => r#"
## Common Commands

```bash
cargo build --release               # Build release binary
cargo run -- --help                 # Show help
cargo test                          # Run tests
```
"#
            .to_string(),

            ProjectType::Generic => String::new(),
        }
    }

    /// Generate slash commands
    fn generate_commands(&self, store: &RequirementsStore) -> Vec<(String, String, String)> {
        let mut commands = Vec::new();

        // Add a project-specific status command
        let project_name = if !store.name.is_empty() {
            &store.name
        } else {
            "project"
        };

        let status_cmd = format!(
            r#"# Project Status

Show current project status and requirements summary.

## Instructions

1. Run `aida list --status approved` to show approved requirements
2. Run `aida list --status draft` to show draft requirements needing review
3. Summarize the current state of the project

## Output Format

```
## {} Status

### Approved Requirements (Ready for Implementation)
- [SPEC-ID] Title

### Draft Requirements (Needing Review)
- [SPEC-ID] Title

### Recently Completed
- [SPEC-ID] Title
```
"#,
            project_name
        );

        commands.push((
            "status".to_string(),
            status_cmd,
            "Show project requirements status".to_string(),
        ));

        // Add a review command
        let review_cmd = r#"# Review Requirement

Review a specific requirement for quality and completeness.

## Usage

Invoke with: `/review <SPEC-ID>`

## Instructions

1. Load the requirement: `aida show $ARGUMENTS`
2. Evaluate the requirement for:
   - Clarity: Is it unambiguous?
   - Testability: Can it be verified?
   - Completeness: Does it have all necessary information?
3. Suggest improvements if needed
4. Offer to update the requirement with suggested changes

## Output Format

```
## Review: [SPEC-ID] - [Title]

### Quality Assessment
- Clarity: X/10
- Testability: X/10
- Completeness: X/10

### Issues Found
- Issue 1
- Issue 2

### Suggested Improvements
[Improved description text]

### Actions
- [ ] Update description
- [ ] Add acceptance criteria
- [ ] Approve requirement
```
"#
        .to_string();

        commands.push((
            "review".to_string(),
            review_cmd,
            "Review a requirement for quality".to_string(),
        ));

        commands
    }

    /// Generate aida-req skill content
    fn generate_aida_req_skill(&self, _store: &RequirementsStore) -> String {
        r#"# AIDA Requirement Creation Skill

## Purpose

Add a new requirement to the AIDA requirements database with AI-powered evaluation feedback.

## When to Use

Use this skill when:
- User wants to add a new requirement or feature request
- User describes something they want the system to do
- User has an idea that should be captured as a requirement
- User asks to "add a requirement" or "create a spec"

## Workflow

### Step 1: Gather Requirement Information

Ask the user for the following information (in conversational style):

1. **Description** (required): What should the system do? This can be:
   - A formal requirement: "The system shall..."
   - A question or idea to be formalized
   - A rough note that needs refinement

2. **Type** (optional, default: Functional):
   - Functional (FR) - System behaviors
   - NonFunctional (NFR) - Quality attributes (performance, security)
   - User (UR) - User needs/goals
   - System (SR) - Technical constraints
   - ChangeRequest (CR) - Modifications to existing features

3. **Priority** (optional, default: Medium):
   - High, Medium, Low

4. **Feature** (optional): Which feature area does this belong to?

5. **Tags** (optional): Comma-separated keywords

### Step 2: Add Requirement to Database

Use the `aida` CLI to add the requirement immediately:

```bash
aida add \
  --title "<generated-title>" \
  --description "<user-description>" \
  --type <type> \
  --priority <priority> \
  --status draft \
  --feature "<feature>" \
  --tags "<tags>"
```

**Title Generation**: Generate a concise title (5-10 words) from the description that captures the essence of the requirement.

### Step 3: Show Confirmation

After adding, display:
```
Requirement added: <SPEC-ID>
Title: <title>
Status: Draft (evaluation pending...)
```

### Step 4: Run AI Evaluation

Evaluate the requirement quality using the AI evaluation prompt. The evaluation should assess:

1. **Clarity** (1-10): Is the requirement clear and unambiguous?
2. **Testability** (1-10): Can this requirement be verified?
3. **Completeness** (1-10): Does it include all necessary information?
4. **Consistency** (1-10): Does it conflict with other requirements?

Provide:
- Overall quality score
- Issues found (if any)
- Suggestions for improvement
- Whether this should be split into multiple requirements

### Step 5: Offer Follow-up Actions

Based on the evaluation, offer:
- **Improve**: Let AI suggest improved description text
- **Split**: Generate child requirements if too broad
- **Link**: Suggest relationships to existing requirements
- **Accept**: Keep as-is and approve

## CLI Reference

```bash
# Add requirement
aida add --title "..." --description "..." --type functional --priority high --status draft

# Show requirement details
aida show <SPEC-ID>

# Edit requirement
aida edit <SPEC-ID> --description "..."

# List features
aida feature list
```

## Integration Notes

- Requirements are stored in `requirements.yaml` or the configured project database
- SPEC-IDs are auto-generated based on type prefix configuration
- The GUI (aida-gui) can be used to view and manage requirements with full AI features
"#
        .to_string()
    }

    /// Generate aida-implement skill content
    fn generate_aida_implement_skill(&self, _store: &RequirementsStore) -> String {
        let comment_examples = match self.config.project_type {
            ProjectType::Rust | ProjectType::Cli => {
                r#"**Rust:**
```rust
// trace:FR-0042 | ai:claude:high
fn implement_feature() {
    // Implementation here
}
```"#
            }
            ProjectType::Python => {
                r#"**Python:**
```python
# trace:FR-0042 | ai:claude:high
def implement_feature():
    """Implementation of FR-0042."""
    pass
```"#
            }
            ProjectType::TypeScript | ProjectType::Web | ProjectType::Api => {
                r#"**TypeScript/JavaScript:**
```typescript
// trace:FR-0042 | ai:claude:high
function implementFeature() {
    // Implementation here
}
```"#
            }
            ProjectType::Generic => {
                r#"**Generic (use language-appropriate comment syntax):**
```
// trace:FR-0042 | ai:claude:high
// Your implementation here
```"#
            }
        };

        format!(
            r#"# AIDA Implementation Skill

## Purpose

Implement an approved requirement with full traceability, evolving the requirement database to capture implementation details and creating child requirements as needed.

## When to Use

Use this skill when:
- User says "implement <SPEC-ID>" or "work on <requirement>"
- User triggers "Copy for Claude Code" from the aida-gui AI menu
- An approved requirement is ready to be implemented
- Continuing implementation of a requirement from a previous session

## Core Principles

### Living Documentation
The requirements database should evolve during implementation to accurately reflect:
- What was actually built (vs. what was initially specified)
- Implementation decisions and trade-offs
- Child requirements discovered during development
- Technical constraints encountered

### Traceability
All AI-generated code must include inline traceability comments linking back to requirement IDs.

## Workflow

### Step 1: Load Requirement Context

Fetch the requirement details:

```bash
aida show <SPEC-ID>
```

Display to user:
- SPEC-ID and title
- Current description
- Status, priority, type
- Related requirements (parent/child, links)
- Any existing implementation notes

### Step 2: Analyze Implementation Scope

Before writing code:
1. Identify files that will be created or modified
2. Identify any sub-tasks or child requirements
3. Confirm approach with user if there are significant decisions

If the requirement is too broad, suggest splitting:
```bash
# Create child requirements
aida add --title "..." --description "..." --type functional --status draft

# Link as child
aida rel add <PARENT-ID> <CHILD-ID> --type Parent
```

### Step 3: Implement with Traceability

When writing or modifying code, add inline traceability comments:

{}

**Comment Format:**
```
// trace:<SPEC-ID> | ai:<tool>:<confidence>
```

Where:
- `<SPEC-ID>`: The requirement being implemented (e.g., FR-0042)
- `<tool>`: Always `claude` for Claude-generated code
- `<confidence>`: `high` (>80% AI), `med` (40-80%), `low` (<40%)

### Step 4: Update Requirement During Implementation

As you implement, update the requirement to reflect reality:

```bash
# Update description with implementation details
aida edit <SPEC-ID> --description "Updated description with implementation notes..."

# Add implementation notes to history
aida comment add <SPEC-ID> "Implementation note: Used async/await pattern for..."

# Update status as appropriate
aida edit <SPEC-ID> --status completed
```

### Step 5: Create Child Requirements

When implementation reveals sub-tasks:

```bash
# Add child requirement
aida add \
  --title "Handle edge case: empty input" \
  --description "The system shall handle empty input gracefully..." \
  --type functional \
  --status draft

# Link to parent
aida rel add <PARENT-ID> <NEW-CHILD-ID> --type Parent
```

### Step 6: Document Completion

When implementation is complete:

1. Update requirement status:
```bash
aida edit <SPEC-ID> --status completed
```

2. Add completion comment:
```bash
aida comment add <SPEC-ID> "Implementation complete. Files modified: src/foo.rs, src/bar.rs"
```

3. Create "Verifies" relationship if tests were added:
```bash
aida rel add <TEST-SPEC-ID> <SPEC-ID> --type Verifies
```

## State Transitions

During implementation, requirements should transition through:

1. **Approved** -> **In Progress** (when starting implementation)
2. **In Progress** -> **Completed** (when implementation is verified)
3. **In Progress** -> **Draft** (if significant changes needed)

Update via:
```bash
aida edit <SPEC-ID> --status <new-status>
```

## CLI Reference

```bash
# Show requirement
aida show <SPEC-ID>

# Edit requirement
aida edit <SPEC-ID> --description "..." --status <status>

# Add comment
aida comment add <SPEC-ID> "Comment text"

# Add relationship
aida rel add <FROM-ID> <TO-ID> --type <Parent|Verifies|References|Duplicate>

# Create new requirement
aida add --title "..." --description "..." --type <type> --status draft

# List requirements by feature
aida list --feature <feature-name>
```
"#,
            comment_examples
        )
    }
}

/// Errors that can occur during scaffolding
#[derive(Debug)]
pub enum ScaffoldError {
    /// IO error while reading/writing files
    IoError { path: PathBuf, message: String },
}

impl std::fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScaffoldError::IoError { path, message } => {
                write!(f, "IO error at {}: {}", path.display(), message)
            }
        }
    }
}

impl std::error::Error for ScaffoldError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> RequirementsStore {
        RequirementsStore {
            name: "test-project".to_string(),
            title: "Test Project".to_string(),
            description: "A test project for scaffolding".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_default_config() {
        let config = ScaffoldConfig::default();
        assert!(config.generate_claude_md);
        assert!(config.generate_commands);
        assert!(config.generate_skills);
        assert!(config.include_aida_req_skill);
        assert!(config.include_aida_implement_skill);
        assert_eq!(config.project_type, ProjectType::Generic);
    }

    #[test]
    fn test_preview_generates_expected_artifacts() {
        let temp_dir = TempDir::new().unwrap();
        let config = ScaffoldConfig::default();
        let scaffolder = Scaffolder::new(temp_dir.path().to_path_buf(), config);
        let store = create_test_store();

        let preview = scaffolder.preview(&store);

        // Should have CLAUDE.md, 2 commands, and 2 skills
        assert!(!preview.artifacts.is_empty());

        // Check that CLAUDE.md is generated
        let claude_md = preview
            .artifacts
            .iter()
            .find(|a| a.path == PathBuf::from("CLAUDE.md"));
        assert!(claude_md.is_some());
        assert!(claude_md.unwrap().content.contains("Test Project"));
    }

    #[test]
    fn test_apply_creates_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = ScaffoldConfig::default();
        let scaffolder = Scaffolder::new(temp_dir.path().to_path_buf(), config);
        let store = create_test_store();

        let preview = scaffolder.preview(&store);
        let result = scaffolder.apply(&preview);

        assert!(result.is_ok());

        // Check that CLAUDE.md was created
        assert!(temp_dir.path().join("CLAUDE.md").exists());

        // Check that .claude directories were created
        assert!(temp_dir.path().join(".claude/commands").exists());
        assert!(temp_dir.path().join(".claude/skills").exists());
    }

    #[test]
    fn test_project_type_labels() {
        assert_eq!(ProjectType::Rust.label(), "Rust");
        assert_eq!(ProjectType::Python.label(), "Python");
        assert_eq!(ProjectType::Generic.label(), "Generic");
    }
}
