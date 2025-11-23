# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust-based CLI requirements management system that stores requirements as YAML files. Requirements can be organized by features, filtered by various attributes, and managed across multiple projects via a central registry.

## Common Commands

### Build and Run
```bash
cargo build --release           # Build release version
cargo run -- <command>          # Run with command
./target/release/requirements-manager <command>  # Run compiled binary
```

### Testing and Development
```bash
cargo test                      # Run tests
cargo check                     # Quick syntax check
cargo clippy                    # Linting
```

## Architecture

### Module Structure

- **main.rs**: Entry point, command routing, and all core business logic for requirement operations (add, list, show, edit, feature management, db management)
- **cli.rs**: Clap-based CLI definitions - `Command`, `FeatureCommand`, `DbCommand` enums
- **models.rs**: Core data structures (`Requirement`, `RequirementsStore`, `RequirementStatus`, `RequirementPriority`, `RequirementType`)
- **storage.rs**: YAML file persistence layer using `serde_yaml`
- **registry.rs**: Multi-project registry management (stored at `~/.requirements.config` by default)
- **project.rs**: Project resolution logic - determines which requirements file to use based on context
- **prompts.rs**: Interactive user prompts using the `inquire` crate

### Project Resolution Priority

The system determines which requirements file to use in this order:

1. Local `requirements.yaml` file (if exists and no project specified)
2. Command line `-p/--project` option
3. `REQ_DB_NAME` environment variable
4. Single project in registry (if only one exists)
5. Default project from registry
6. Interactive prompt to select project

### Feature Numbering System

Features use sequential numbering (e.g., "1-Authentication", "2-User-Management"). The `RequirementsStore` maintains a `next_feature_number` counter that auto-increments. The `migrate_features()` function converts legacy feature names to numbered format on load.

### Registry System

Multi-project support via `~/.requirements.config` (YAML). Can be overridden with `REQ_REGISTRY_PATH` environment variable. Projects have name, path, and description. Optional default project configuration.

## Key Environment Variables

- `REQ_DB_NAME`: Specifies which project from registry to use
- `REQ_FEATURE`: Default feature name when creating requirements (defaults to "Uncategorized")
- `REQ_REGISTRY_PATH`: Override default registry location (~/.requirements.config)

## Data Model

Requirements have:
- UUID identifier
- Title, description
- Status: Draft, Approved, Completed, Rejected
- Priority: High, Medium, Low
- Type: Functional, NonFunctional, System, User
- Owner, feature, tags
- Dependencies (Vec<Uuid>)
- Timestamps (created_at, modified_at)

## Interactive vs CLI Modes

Most commands support both modes:
- **Interactive**: Use `--interactive` flag or omit required arguments to trigger prompts
- **CLI**: Provide all arguments as flags (e.g., `--title`, `--description`, `--status`)

The pattern is implemented in functions like `add_requirement_interactive()` vs `add_requirement_cli()`.

## Notable Implementation Details

- Storage automatically calls `migrate_features()` on load to ensure all features have number prefixes
- Colored output using the `colored` crate with semantic colors (green=completed, red=rejected, etc.)
- UUID parsing and requirement lookup by ID throughout
- Editor integration for multi-line description editing via `inquire::Editor`
