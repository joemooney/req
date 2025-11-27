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

**With custom ID prefix:**
```bash
req add --title "Security audit" \
        --prefix SEC \
        --description "Perform security audit"
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

### Relationship Definitions

Manage custom relationship types with constraints:

```bash
# List all relationship definitions
req rel-def list

# Show details for a relationship definition
req rel-def show parent

# Add a custom relationship type
req rel-def add --name "blocks" \
    --display-name "Blocks" \
    --description "This requirement blocks another" \
    --inverse "blocked_by" \
    --cardinality n:n \
    --color "#ff6b6b"

# Edit a relationship definition
req rel-def edit parent --source-types "Functional,System"

# Remove a custom relationship definition
req rel-def remove blocks
```

**Built-in relationship types:**
- `parent` / `child` - Hierarchical decomposition (N:1 / 1:N)
- `verifies` / `verified_by` - Test relationships (N:N)
- `depends_on` / `dependency_of` - Dependencies (N:N)
- `implements` / `implemented_by` - Implementation links (N:N)
- `references` - General reference (N:N, no inverse)
- `duplicate` - Marks duplicates (symmetric)

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
2. **Left Panel** - Requirements list with search filter (collapsible in edit mode)
3. **Main Area** - Detail view, forms, or welcome screen

### Responsive Layout

When editing or adding requirements, the left panel remains visible if the window is wide enough (900+ pixels). You can:
- Click the **▶ Hide** button in the left panel header to collapse it
- Click **◀ Show List** in the form area to restore the panel

This allows you to reference other requirements while editing, or maximize form space on smaller screens.

### Navigation

- Click a requirement in the left panel to view its details
- Double-click a requirement to open it for editing
- Use the search box to filter requirements by title or description
- Click tabs (Description, Comments, Links, History) to switch views
- Use arrow keys to navigate up/down through the requirements list
- Press Enter to edit the selected requirement
- Press Space to expand/collapse tree nodes (in hierarchical views)

### Actions

| Button | Action |
|--------|--------|
| **+ Add** | Create a new requirement |
| **Reload** | Refresh from disk |
| **Edit** | Edit selected requirement |
| **Delete** | Delete selected requirement |
| **Settings** | Open settings dialog |
| **Help** | Open this user guide |

### View Perspectives

The requirements list can be displayed in different organizational views. Select your preferred view from the dropdown in the top bar or set a default in Settings > Appearance.

| View | Description |
|------|-------------|
| **Flat List** | Simple list of all requirements |
| **Parent/Child** | Hierarchical tree showing parent-child relationships |
| **Verification** | Groups requirements by verification relationships |
| **References** | Groups requirements by reference relationships |

In tree views, use the +/- buttons or press Space to expand/collapse nodes.

### View Presets

You can save your current view configuration (perspective, direction, and filters) as a named preset for quick access later.

**To save a preset:**
1. Configure your view using the View dropdown, direction selector, and filter options
2. When you have unsaved changes, a "💾 Save As..." button appears
3. Click it, enter a name, and click Save

**To use a preset:**
- Select it from the View dropdown under "Saved Presets"
- The preset will restore all saved settings (perspective, direction, type filters, feature filters)

**To delete a preset:**
- Click the ✕ button next to the preset name in the dropdown
- Confirm deletion in the dialog

**Modified indicator:**
- If you modify an active preset, its name shows with an asterisk (e.g., "My View*")
- Use "Save As..." to save changes to the same name or create a new preset

**Reset button (↺):**
- Click to return to the default Flat List view with no filters

### Adding Comments

1. Select a requirement
2. Click the "Comments" tab
3. Click "+ Add Comment"
4. Enter your comment and click Save

Comments support threading - click "Reply" on any comment to add a nested reply. Comments are displayed in a collapsible tree structure.

### Comment Reactions

You can add emoji reactions to comments to quickly indicate your response:

**Adding a reaction:**
- Click the 😊 button on any comment to open the reaction picker
- Select an emoji to add your reaction
- Click the same emoji again to remove your reaction

**Default reactions:**
| Emoji | Name | Use Case |
|-------|------|----------|
| ✅ | Resolved | Mark comment as addressed |
| ❌ | Rejected | Mark comment as declined |
| 👍 | Thumbs Up | Agree or approve |
| 👎 | Thumbs Down | Disagree or disapprove |
| ❓ | Question | Needs clarification |
| ⚠️ | Important | Attention needed |

**Customizing reactions:**
- Go to Settings > Reactions tab
- Add new custom reactions with your own emoji and labels
- Edit existing reactions (emoji, label, description)
- Delete custom reactions (built-in reactions cannot be deleted)
- Reset to defaults to restore the standard reaction set

---

## Working with Requirements

### Requirement Fields

| Field | Description |
|-------|-------------|
| **SPEC-ID** | Auto-generated identifier (e.g., SPEC-001) |
| **Title** | Short descriptive name |
| **Description** | Detailed explanation (supports Markdown) |
| **Status** | Draft, Approved, Completed, or Rejected |
| **Priority** | High, Medium, or Low |
| **Type** | Functional, NonFunctional, System, or User |
| **Feature** | Grouping category |
| **Owner** | Person responsible |
| **Tags** | Comma-separated labels |
| **ID Prefix** | Optional custom prefix override (uppercase letters only) |

### Custom ID Prefixes

By default, requirement IDs are generated based on the feature and/or type configuration. You can override this by specifying a custom prefix:

- **CLI**: Use `--prefix SEC` when adding a requirement
- **GUI**: Enter the prefix in the "ID Prefix" field (e.g., `SEC`, `PERF`, `API`)

Custom prefixes must contain only uppercase letters (A-Z). Leave blank to use the default prefix derived from feature/type settings.

**Examples:**
- `SEC-001` - Security requirement
- `PERF-001` - Performance requirement
- `API-001` - API requirement

When using "Per Prefix" numbering strategy, each custom prefix gets its own counter. With "Global Sequential" numbering, all requirements share the same counter regardless of prefix.

### Markdown Support

Requirement descriptions support Markdown formatting. When viewing a requirement, the description is rendered with full Markdown support including:

- **Headers** (`# H1`, `## H2`, etc.)
- **Bold** and *italic* text
- Bullet and numbered lists
- Code blocks with syntax highlighting
- Links and images
- Tables
- Task lists

When editing a requirement, click the **👁 Preview** button to see how your Markdown will render. Click **✏ Edit** to return to the text editor.

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

The system includes built-in relationship types with configurable constraints:

| Type | Inverse | Cardinality | Description |
|------|---------|-------------|-------------|
| **Parent** | Child | N:1 | Hierarchical decomposition |
| **Child** | Parent | 1:N | Child of parent requirement |
| **Verifies** | Verified By | N:N | Test/verification relationship |
| **Verified By** | Verifies | N:N | Verified by test requirement |
| **Depends On** | Dependency Of | N:N | Dependency relationship |
| **Dependency Of** | Depends On | N:N | Inverse dependency |
| **Implements** | Implemented By | N:N | Implementation relationship |
| **Implemented By** | Implements | N:N | Inverse implementation |
| **References** | - | N:N | General reference link |
| **Duplicate** | (symmetric) | N:N | Marks as duplicate |

**Cardinality meanings:**
- **1:1** - One source to one target
- **1:N** - One source to many targets
- **N:1** - Many sources to one target
- **N:N** - Many sources to many targets

Custom relationship types can be created with:
- Type constraints (limit which requirement types can participate)
- Cardinality rules
- Inverse relationship definitions
- Visualization colors

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
| **Ctrl+S** | Save (in Add/Edit forms) |
| **Ctrl+T** | Cycle through themes |
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

### Appearance

| Setting | Description |
|---------|-------------|
| **Theme** | Color scheme (Dark, Light, High Contrast Dark, Solarized Dark, Nord) |
| **Base Font Size** | Default font size (8-32pt) |
| **Default View** | Preferred perspective (Flat List, Parent/Child, etc.) |

### Keyboard Shortcuts

The Keybindings tab shows all customizable keyboard shortcuts. Click "Change" next to any action to set a new key combination. Press Escape to cancel. Click "Reset to Defaults" to restore default bindings.

| Action | Default Key | Default Context |
|--------|-------------|-----------------|
| Navigate Up | Up Arrow | Requirements List |
| Navigate Down | Down Arrow | Requirements List |
| Edit Requirement | Enter | Requirements List |
| Toggle Expand/Collapse | Space | Requirements List |
| Zoom In | Ctrl+Shift+Plus | Global |
| Zoom Out | Ctrl+Minus | Global |
| Reset Zoom | Ctrl+0 | Global |
| Cycle Theme | Ctrl+T | Global |
| Save | Ctrl+S | Form |

**Context/Scope:**

Each keybinding has a context that determines where it is active:

| Context | Description |
|---------|-------------|
| **Global** | Works anywhere in the application |
| **Requirements List** | Only when focused on the requirements list (not when typing in text fields) |
| **Detail View** | Only when viewing requirement details |
| **Form** | Only when in add/edit form |

You can change the context for any keybinding using the dropdown in the Settings > Keys tab. This allows you to, for example:
- Make navigation keys work globally
- Restrict certain shortcuts to specific views
- Prevent shortcuts from interfering with text input

User settings are stored in: `~/.requirements_gui_settings.yaml`

### Project Settings

Configure how requirement IDs are formatted and numbered. These settings are stored in the project's `requirements.yaml` file.

| Setting | Description |
|---------|-------------|
| **ID Format** | Single Level (PREFIX-NNN) or Two Level (FEATURE-TYPE-NNN) |
| **Numbering** | Global Sequential, Per Prefix, or Per Feature+Type |
| **Digits** | Number of digits in the numeric portion (1-6) |

**ID Format Options:**
- **Single Level**: `AUTH-001`, `FR-002` - Simple prefix with number
- **Two Level**: `AUTH-FR-001`, `PAY-NFR-001` - Feature prefix, type prefix, then number

**Numbering Options:**
- **Global Sequential**: All requirements share one counter (AUTH-001, FR-002, PAY-003)
- **Per Prefix**: Each prefix has its own counter (AUTH-001, FR-001, PAY-001)
- **Per Feature+Type**: Each feature+type combination has its own counter (only for Two Level format)

**Migrating Existing IDs:**

When you change ID configuration settings, you can optionally migrate existing requirement IDs to the new format using the "Migrate Existing IDs" button. The migration has the following constraints:

- **Digit count validation**: You cannot reduce the number of digits below the maximum currently in use. For example, if you have `SPEC-1234` (4 digits), you cannot change to 3 digits.
- **Format change requirement**: To change between Single Level and Two Level formats, you must have Global Sequential numbering selected.
- **Safe operation**: Requirements that cannot be migrated (e.g., would exceed digit limit) are skipped with a warning.

The migration dialog shows:
- Number of requirements that will be affected
- Any validation errors that prevent migration
- Warnings about potential issues

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

7. **Use Markdown in descriptions**: Format requirements with headers, lists, and code blocks for clarity

8. **Custom prefixes for cross-cutting concerns**: Use custom ID prefixes like `SEC-`, `PERF-`, `API-` for requirements that span multiple features

9. **Keyboard shortcuts for efficiency**: Learn the shortcuts (Ctrl+T for themes, arrow keys for navigation) to speed up your workflow

10. **Set your preferred view**: Configure your default perspective in Settings to match how you like to organize requirements

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
