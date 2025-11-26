# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust-based requirements management system with both CLI and GUI interfaces. Stores requirements as YAML files. Requirements can be organized by features, filtered by various attributes, and managed across multiple projects via a central registry.

## Project Structure

This is a Cargo workspace with three crates:

- **requirements-core**: Shared library containing models, storage, and business logic
- **requirements-cli**: Command-line interface (`req` binary)
- **requirements-gui**: Graphical interface using egui (`req-gui` binary)

## Common Commands

### Build and Run
```bash
cargo build --workspace --release   # Build all crates
cargo run -p requirements-cli -- <command>  # Run CLI
cargo run -p requirements-gui       # Run GUI

# Or after building:
./target/release/req <command>      # CLI binary
./target/release/req-gui            # GUI binary
```

### Testing and Development
```bash
cargo test --workspace              # Run all tests
cargo check --workspace             # Quick syntax check
cargo clippy --workspace            # Linting
```

## Architecture

### Module Structure (requirements-core)

- **models.rs**: Core data structures (`Requirement`, `RequirementsStore`, `RequirementStatus`, `RequirementPriority`, `RequirementType`, `Comment`, `HistoryEntry`, `Relationship`)
- **storage.rs**: YAML file persistence layer using `serde_yaml`
- **registry.rs**: Multi-project registry management (stored at `~/.requirements.config`)
- **project.rs**: Project resolution logic
- **export.rs**: Export functionality (mapping, JSON)

### Module Structure (requirements-cli)

- **main.rs**: Entry point and command handlers
- **cli.rs**: Clap-based CLI definitions
- **prompts.rs**: Interactive user prompts using `inquire`

### Module Structure (requirements-gui)

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
req add [--interactive]             # Add requirement
req list [--status X --priority Y]  # List with filters
req show <ID>                       # Show details (UUID or SPEC-ID)
req edit <ID>                       # Edit requirement
req del <ID> [-y]                   # Delete requirement
req rel add --from X --to Y --type T  # Add relationship
req comment add --id X --content Y  # Add comment
req feature list                    # List features
req db list                         # List projects
req user-guide [--dark]             # Open documentation
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
