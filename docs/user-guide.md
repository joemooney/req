# Requirements Manager User Guide

A professional requirements management system with both CLI and GUI interfaces.

## Table of Contents

- [Getting Started](#getting-started)
- [CLI Usage](#cli-usage)
- [GUI Usage](#gui-usage)
- [Working with Requirements](#working-with-requirements)
- [Features and Organization](#features-and-organization)
- [Multi-Project Support](#multi-project-support)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Settings](#settings)

---

## Getting Started

### Installation

Build the project from source:

```bash
cargo build --workspace --release
```

This creates two binaries in `target/release/`:
- `req` - Command-line interface
- `req-gui` - Graphical user interface

### Quick Start

1. **Create your first requirement:**
   ```bash
   req add --title "User login" --description "Users can log in with email and password"
   ```

2. **List all requirements:**
   ```bash
   req list
   ```

3. **Or launch the GUI:**
   ```bash
   req-gui
   ```

---

## CLI Usage

### Basic Commands

| Command | Description |
|---------|-------------|
| `req list` | List all requirements |
| `req add` | Add a new requirement |
| `req show <ID>` | Show requirement details |
| `req edit <ID>` | Edit a requirement |
| `req delete <ID>` | Delete a requirement |

### Adding Requirements

**Interactive mode:**
```bash
req add --interactive
```

**Command line mode:**
```bash
req add --title "Feature name" \
        --description "Detailed description" \
        --priority High \
        --status Draft \
        --type Functional \
        --feature "Authentication"
```

### Filtering and Searching

```bash
# Filter by status
req list --status Approved

# Filter by priority
req list --priority High

# Filter by feature
req list --feature "Authentication"

# Search by text
req list --search "login"
```

### Working with Relationships

```bash
# Add a parent-child relationship
req rel add --from SPEC-001 --to SPEC-002 --type parent

# Add bidirectional relationship
req rel add --from SPEC-001 --to SPEC-002 --type verifies -b

# List relationships
req rel list SPEC-001
```

### Feature Management

```bash
# List features
req feature list

# Rename a feature
req feature rename "Old Name" "New Name"

# Move requirements between features
req feature move SPEC-001 "New Feature"
```

### Database Management

```bash
# List registered projects
req db list

# Add a new project
req db add --name "my-project" --path "/path/to/requirements.yaml"

# Set default project
req db default "my-project"

# Remove a project
req db remove "my-project"
```

### Opening the User Guide

```bash
# Open in default browser (light mode)
req user-guide

# Open in dark mode
req user-guide --dark
```

---

## GUI Usage

Launch the GUI application:
```bash
req-gui
```

### Main Interface

The GUI consists of three main areas:

1. **Top Bar** - Contains action buttons, requirement count, and settings
2. **Left Panel** - Requirements list with search filter
3. **Main Area** - Detail view, forms, or welcome screen

### Navigation

- Click a requirement in the left panel to view its details
- Use the search box to filter requirements by title or description
- Click tabs (Description, Comments, Links, History) to switch views

### Actions

| Button | Action |
|--------|--------|
| **+ Add** | Create a new requirement |
| **Reload** | Refresh from disk |
| **Edit** | Edit selected requirement |
| **Delete** | Delete selected requirement |
| **Settings** | Open settings dialog |
| **Help** | Open this user guide |

### Adding Comments

1. Select a requirement
2. Click the "Comments" tab
3. Click "+ Add Comment"
4. Enter your comment and click Save

Comments support threading - click "Reply" on any comment to add a nested reply.

---

## Working with Requirements

### Requirement Fields

| Field | Description |
|-------|-------------|
| **SPEC-ID** | Auto-generated identifier (e.g., SPEC-001) |
| **Title** | Short descriptive name |
| **Description** | Detailed explanation |
| **Status** | Draft, Approved, Completed, or Rejected |
| **Priority** | High, Medium, or Low |
| **Type** | Functional, NonFunctional, System, or User |
| **Feature** | Grouping category |
| **Owner** | Person responsible |
| **Tags** | Comma-separated labels |

### Status Workflow

```
Draft -> Approved -> Completed
              |
              v
          Rejected
```

- **Draft**: Initial state, work in progress
- **Approved**: Reviewed and accepted
- **Completed**: Implementation finished
- **Rejected**: Not accepted or deprecated

### Relationship Types

| Type | Description |
|------|-------------|
| **Parent** | Hierarchical parent-child |
| **Verifies** | Test/verification relationship |
| **References** | General reference link |
| **Custom** | User-defined relationship |

---

## Features and Organization

Requirements are organized into numbered features for better management.

### Feature Naming

Features are automatically numbered:
- `1-Authentication`
- `2-User-Management`
- `3-Reporting`

### Default Feature

Requirements without a specified feature go to "Uncategorized". Set a default feature using the `REQ_FEATURE` environment variable:

```bash
export REQ_FEATURE="Authentication"
req add --title "New auth requirement"  # Automatically uses Authentication
```

---

## Multi-Project Support

Manage multiple requirement sets using the registry system.

### Registry Location

Default: `~/.requirements.config`

Override with: `REQ_REGISTRY_PATH` environment variable

### Project Resolution Order

1. Local `requirements.yaml` in current directory
2. `--project` command line option
3. `REQ_DB_NAME` environment variable
4. Single project in registry (if only one exists)
5. Default project from registry
6. Interactive prompt

### Example Setup

```bash
# Register projects
req db add --name "frontend" --path ~/projects/frontend/requirements.yaml
req db add --name "backend" --path ~/projects/backend/requirements.yaml

# Set default
req db default frontend

# Work with specific project
req list --project backend
```

---

## Keyboard Shortcuts

### GUI Shortcuts

| Shortcut | Action |
|----------|--------|
| **Arrow Up/Down** | Navigate requirements list |
| **Enter** | Edit selected requirement |
| **Double-click** | Edit requirement |
| **Space** | Expand/collapse tree node (in tree views) |
| **Ctrl+MouseWheel** | Zoom in/out |
| **Ctrl+Shift++** | Zoom in |
| **Ctrl+-** | Zoom out |
| **Ctrl+0** | Reset zoom to base size |

---

## Settings

### User Profile

Access settings via the gear icon (top-right) in the GUI.

| Setting | Description |
|---------|-------------|
| **Name** | Your full name (used in comments/history) |
| **Email** | Your email address |
| **Handle** | Nickname for @mentions in comments |
| **Base Font Size** | Default font size (8-32pt) |

Settings are stored in: `~/.requirements_gui_settings.yaml`

### Environment Variables

| Variable | Description |
|----------|-------------|
| `REQ_DB_NAME` | Default project name |
| `REQ_FEATURE` | Default feature for new requirements |
| `REQ_REGISTRY_PATH` | Custom registry file location |

---

## Tips and Best Practices

1. **Use meaningful SPEC-IDs**: Reference requirements by their SPEC-ID in documentation and code comments

2. **Organize by features**: Group related requirements together for better navigation

3. **Track relationships**: Link requirements to tests using "verifies" relationships

4. **Use comments for discussions**: Keep requirement discussions in the comments, not the description

5. **Regular status updates**: Keep status current to track project progress

6. **Backup your data**: The YAML format is human-readable and version-control friendly

---

## Troubleshooting

### Common Issues

**"No requirements file found"**
- Create a `requirements.yaml` in the current directory, or
- Register a project with `req db add`

**"Failed to save"**
- Check file permissions
- Ensure the directory exists

**GUI won't start**
- Ensure you have a display server running
- Check for missing system libraries (OpenGL, etc.)

### Getting Help

- Run `req --help` for CLI help
- Run `req <command> --help` for command-specific help
- Open this guide with `req user-guide`

---

*Generated for Requirements Manager v0.1.0*
