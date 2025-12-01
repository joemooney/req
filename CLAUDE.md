# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AIDA (AI-Driven Architecture) is a Rust-based requirements management system with both CLI and GUI interfaces. Stores requirements as YAML files. Requirements can be organized by features, filtered by various attributes, and managed across multiple projects via a central registry. Includes AI-powered evaluation, duplicate detection, and Claude Code integration.

## Project Structure

This is a Cargo workspace with three crates:

- **aida-core**: Shared library containing models, storage, and business logic
- **aida-cli**: Command-line interface (`aida` binary)
- **aida-gui**: Graphical interface using egui (`aida-gui` binary)

## Common Commands

### Build and Run
```bash
cargo build --workspace --release   # Build all crates
cargo run --bin aida -- <command>   # Run CLI
cargo run --bin aida-gui            # Run GUI

# Or after building:
./target/release/aida <command>     # CLI binary
./target/release/aida-gui           # GUI binary
```

### Testing and Development
```bash
cargo test --workspace              # Run all tests
cargo check --workspace             # Quick syntax check
cargo clippy --workspace            # Linting
```

## Architecture

### Module Structure (aida-core)

- **models.rs**: Core data structures (`Requirement`, `RequirementsStore`, `RequirementStatus`, `RequirementPriority`, `RequirementType`, `Comment`, `HistoryEntry`, `Relationship`)
- **storage.rs**: YAML file persistence layer using `serde_yaml`
- **db/**: Database abstraction layer for multiple storage backends
  - **traits.rs**: `DatabaseBackend` trait defining the storage interface
  - **yaml_backend.rs**: YAML storage implementation (wraps existing Storage)
  - **sqlite_backend.rs**: SQLite storage implementation with WAL mode
  - **migration.rs**: Migration utilities (YAML↔SQLite, JSON import/export)
- **registry.rs**: Multi-project registry management (stored at `~/.requirements.config`)
- **project.rs**: Project resolution logic
- **export.rs**: Export functionality (mapping, JSON)
- **scaffolding.rs**: Claude Code project scaffolding (CLAUDE.md, commands, skills generation)

### Module Structure (aida-cli)

- **main.rs**: Entry point and command handlers
- **cli.rs**: Clap-based CLI definitions
- **prompts.rs**: Interactive user prompts using `inquire`

### Module Structure (aida-gui)

- **main.rs**: Entry point
- **app.rs**: Main application state and UI rendering (egui)

### Project Resolution Priority

1. Local `requirements.yaml` file (if exists and no project specified)
2. Command line `-p/--project` option
3. `REQ_DB_NAME` environment variable
4. Single project in registry (if only one exists)
5. Default project from registry
6. Interactive prompt to select project

## Key Environment Variables

- `REQ_DB_NAME`: Specifies which project from registry to use
- `REQ_FEATURE`: Default feature name when creating requirements
- `REQ_REGISTRY_PATH`: Override default registry location

## Data Model

Requirements have:
- UUID identifier + SPEC-ID (human-friendly, e.g., SPEC-001)
- Title, description
- Status: Draft, Approved, Completed, Rejected
- Priority: High, Medium, Low
- Type: Functional, NonFunctional, System, User, ChangeRequest
- Owner, feature, tags
- Relationships (parent/child, verifies, references, custom)
- Comments (threaded)
- History (field change tracking)
- Timestamps (created_at, modified_at)

## GUI Keyboard Shortcuts

- **Arrow Up/Down**: Navigate requirements list
- **Enter**: Edit selected requirement
- **Double-click**: Edit requirement
- **Space**: Expand/collapse tree node (in tree views)
- **Ctrl+MouseWheel**: Zoom in/out
- **Ctrl+Shift++**: Zoom in
- **Ctrl+-**: Zoom out
- **Ctrl+0**: Reset zoom

## CLI Commands

```bash
aida add [--interactive]            # Add requirement
aida list [--status X --priority Y] # List with filters
aida show <ID>                      # Show details (UUID or SPEC-ID)
aida edit <ID>                      # Edit requirement
aida del <ID> [-y]                  # Delete requirement
aida rel add --from X --to Y --type T  # Add relationship
aida comment add --id X --content Y # Add comment
aida feature list                   # List features
aida db list                        # List projects
aida user-guide [--dark]            # Open documentation
```

## Documentation

- **docs/user-guide.md**: Source markdown
- **docs/user-guide.html**: Generated light mode HTML
- **docs/user-guide-dark.html**: Generated dark mode HTML
- **helper/generate-docs.sh**: Script to regenerate HTML

## Notable Implementation Details

- Storage auto-migrates legacy formats on load
- SPEC-IDs are auto-generated and configurable (prefix, digits)
- GUI persists user settings to `~/.requirements_gui_settings.yaml`
- Comments support threading with parent references
- History tracks all field changes with old/new values

## Claude Code Skills

AIDA provides Claude Code skills for requirements-driven development:

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

## AI Integration

The GUI includes AI-powered features (requires LLM API access):
- **Evaluate**: Quality assessment with scoring
- **Find Duplicates**: Detect similar requirements
- **Improve**: AI-suggested description improvements
- **Generate Children**: Break down into sub-requirements
- **Copy for Claude Code**: Format requirement for implementation
- **Scaffold Project**: Generate Claude Code integration artifacts (Settings > AI tab)

### Project Scaffolding

Generate Claude Code integration files from Settings > AI > "Scaffold Project":
- **CLAUDE.md**: Project instructions with context, tech stack, and features
- **.claude/commands/**: Project-specific slash commands (status, review)
- **.claude/skills/**: Requirements-driven development skills (aida-req, aida-implement)

Supported project types: Rust, Python, TypeScript, Web, API, CLI, Generic
