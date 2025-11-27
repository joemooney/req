# Requirements Manager - Project Overview

A professional requirements management system built in Rust, providing both CLI and GUI interfaces for managing software requirements with rich features including relationships, comments, history tracking, and multi-project support.

## Vision

Create a lightweight, file-based requirements management tool that is:
- Version-control friendly (YAML storage)
- Flexible enough for different project needs
- Usable from both command line and graphical interface
- Capable of tracking requirement relationships and history

## Project Structure

This is a Cargo workspace with three crates:

```
req/
├── requirements-core/    # Shared library - models, storage, business logic
├── requirements-cli/     # CLI tool (req binary)
├── requirements-gui/     # GUI application (req-gui binary, egui-based)
├── docs/                 # User documentation (markdown + HTML)
└── helper/               # Helper scripts for documentation generation
```

## Key Features

### Dual Interface
- **CLI (`req`)**: Full-featured command-line interface for scripting and quick operations
- **GUI (`req-gui`)**: Modern egui-based graphical interface with tabbed views

### SPEC-ID System
Human-friendly identifiers (SPEC-001, SPEC-002) alongside internal UUIDs. Configurable ID formats with feature-based prefixes.

### Requirement Management
- Full CRUD operations (Create, Read, Update, Delete)
- Type-specific status states (e.g., Draft, Approved, Completed, Rejected for standard types)
- Priority levels: High, Medium, Low
- Types: Functional, Non-Functional, System, User, Change Request (with type-specific workflows)
- Feature-based organization with numbered prefixes
- Tag support for flexible categorization
- Custom fields support for type-specific data (e.g., Impact, Requested By for Change Requests)

### Relationships
Define connections between requirements:
- **Parent/Child**: Hierarchical relationships
- **Verifies/VerifiedBy**: Test/verification traceability
- **References**: General reference links
- **Duplicate**: Mark duplicate requirements
- **Custom**: User-defined relationship types

### Comments & History
- Threaded comment system with replies
- Configurable emoji reactions on comments
- Full change history tracking for requirements
- User attribution with handles for @mentions

### Custom Type Definitions
- Type-specific status workflows (e.g., Change Request has: Draft → Submitted → Under Review → Approved → In Progress → Implemented → Verified → Closed)
- Custom fields per type with multiple field types (Text, TextArea, Select, Boolean, Date, User, Requirement, Number)
- Built-in type definitions for Functional, NonFunctional, System, User, and ChangeRequest types
- Settings UI for viewing type definitions

### Multi-Project Support
- Central registry (~/.requirements.config) for managing multiple projects
- Environment variable support (REQ_DB_NAME, REQ_FEATURE, REQ_REGISTRY_PATH)
- Project resolution with priority ordering

### GUI-Specific Features
- Multiple view perspectives (Flat, Parent/Child, Verification, References)
- User settings (name, email, handle, font size)
- Zoom controls (Ctrl+MouseWheel, keyboard shortcuts)
- Collapsible comment trees
- Tabbed interface (Description, Comments, Links, History)

## Technology Stack

- **Language**: Rust
- **GUI Framework**: egui (cross-platform)
- **Storage**: YAML (serde_yaml)
- **CLI Framework**: clap
- **Interactive Prompts**: inquire

## Data Storage

Requirements are stored in `requirements.yaml` files:
- Human-readable YAML format
- Git-friendly for version control
- Includes metadata, relationships, comments, and history

## Getting Started

```bash
# Build
cargo build --workspace --release

# CLI usage
req list                          # List requirements
req add --interactive             # Add requirement interactively
req show SPEC-001                 # Show requirement details
req rel add --from SPEC-001 --to SPEC-002 --type parent  # Add relationship

# GUI usage
req-gui                           # Launch graphical interface

# Open user guide
req user-guide                    # Open in browser (light mode)
req user-guide --dark             # Open in browser (dark mode)
```

## Documentation

- **README.md**: Quick start and project structure
- **CLAUDE.md**: AI assistant instructions and technical details
- **docs/user-guide.md**: Comprehensive user documentation
- **docs/user-guide.html**: Pre-generated HTML (light mode)
- **docs/user-guide-dark.html**: Pre-generated HTML (dark mode)
