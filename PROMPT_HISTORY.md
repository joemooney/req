# Requirements Manager - Prompt History

A chronological record of development sessions and changes made to the Requirements Manager project.

---

## Session 1: Initial Setup and Core Features

### Initial Commit
- Created basic requirements management CLI in Rust
- Implemented core data models (Requirement, RequirementStatus, RequirementPriority, RequirementType)
- Added YAML-based storage layer
- Implemented basic CRUD operations

### Integration Planning
- **Prompt**: Plan integration with ai-provenance system
- **Actions**:
  - Added INTEGRATION.md with detailed integration plan
  - Created SIMPLIFIED_INTEGRATION.md with streamlined approach
  - Added FINAL_RECOMMENDATION.md with implementation recommendations
  - Created INTEGRATION_INDEX.md for documentation navigation

### Export Feature
- **Prompt**: Add export functionality for requirement mappings
- **Actions**:
  - Implemented `export` command in CLI
  - Support for mapping format (UUID/SPEC-ID)
  - Support for JSON export format
  - Output to file or stdout

---

## Session 2: SPEC-ID System and Relationships

### SPEC-ID Implementation
- **Prompt**: Add human-friendly SPEC-ID as alternate key
- **Actions**:
  - Designed SPEC-ID format (SPEC-XXX)
  - Added UUID_SPEC_ID_VERIFICATION.md with mapping verification
  - Added SPEC_ID_AS_ALTERNATE_KEY.md with design document
  - Implemented SPEC-ID in Requirement model
  - Added SPEC_ID_IMPLEMENTATION_COMPLETE.md summary

### Delete Command
- **Prompt**: Add ability to delete requirements
- **Actions**:
  - Implemented `del` command in CLI
  - Support for both UUID and SPEC-ID lookups
  - Added confirmation prompt with --yes/-y skip option

### Relationship System
- **Prompt**: Add relationships between requirements
- **Actions**:
  - Implemented relationship types (Parent, Child, Verifies, VerifiedBy, References, Duplicate, Custom)
  - Added `rel add` command with bidirectional support
  - Added `rel remove` command
  - Added `rel list` command
  - Improved relationship display clarity

---

## Session 3: Workspace Restructure and GUI

### Workspace Restructure
- **Prompt**: Restructure project into workspace with CLI and GUI
- **Actions**:
  - Created Cargo workspace with three crates:
    - requirements-core: Shared library
    - requirements-cli: CLI tool (req binary)
    - requirements-gui: GUI application (req-gui binary)
  - Moved core logic to shared library
  - Cleaned up old requirements-manager directory
  - Updated .gitignore

### GUI Implementation
- **Prompt**: Implement full CRUD operations in GUI
- **Actions**:
  - Implemented egui-based GUI application
  - Added requirements list with search/filter
  - Added detail view for requirements
  - Implemented Add, Edit, Delete operations
  - Added Reload functionality

---

## Session 4: Comments and History

### Comment System
- **Prompt**: Implement threaded comment system
- **Actions**:
  - Added Comment model with threading support
  - Implemented comment CRUD operations
  - Added `comment add` CLI command with parent support
  - Added `comment list`, `comment edit`, `comment delete` commands
  - Integrated comments into GUI

### Collapsible Comments
- **Prompt**: Add collapsible comment trees to GUI
- **Actions**:
  - Implemented tree view for threaded comments
  - Added expand/collapse functionality
  - Added reply button for nested comments

### Change History
- **Prompt**: Add change history tracking to requirements
- **Actions**:
  - Added HistoryEntry and FieldChange models
  - Track all field changes with old/new values
  - Record timestamp and author for changes
  - Added history display in requirement details

### Tabbed Interface
- **Prompt**: Implement tabbed interface with history in GUI
- **Actions**:
  - Added tab system (Description, Comments, Links, History)
  - Implemented DetailTab enum for view state
  - Added History tab showing change log
  - Added Links tab for relationship display

---

## Session 5: Improvements and Documentation

### Many Improvements (Latest Session)
- **Prompt**: Various improvements and polish
- **Actions**:
  - Enhanced GUI with user settings (name, email, handle)
  - Added configurable font size with zoom controls
  - Added multiple view perspectives (Flat, Parent/Child, Verification, References)
  - Implemented ID configuration commands
  - Added requirement type management
  - Created user-guide.md documentation
  - Generated HTML documentation (light and dark modes)
  - Added `user-guide` CLI command to open documentation
  - Created helper scripts for documentation generation

### Documentation Cleanup
- **Prompt**: Review updates and create documentation
- **Actions**:
  - Created OVERVIEW.md with project vision and structure
  - Created REQUIREMENTS.md with system requirements
  - Created PROMPT_HISTORY.md (this file)

### Arrow Key Navigation
- **Prompt**: Add arrow key navigation for requirements list panel
- **Actions**:
  - Added `get_filtered_indices()` helper function to app.rs
  - Implemented Up/Down arrow key handling in update() function
  - Navigation respects current filters (search, type, feature filters)
  - Auto-selects first/last item when nothing selected
  - Updated user-guide.md with new keyboard shortcut
  - Regenerated HTML documentation
  - Updated CLAUDE.md to reflect current workspace structure

### GUI Enhancements (Continued)
- **Prompt**: Various UI improvements
- **Actions**:
  - Enter key to edit selected requirement
  - Double-click to edit requirement
  - Spacebar to expand/collapse tree nodes
  - Full-width title and description fields in forms
  - Full-width comment content field
  - Proper indentation for threaded comments with +/- icons
  - Fixed-width expand/collapse buttons (18x18)
  - Comment text wrapping within panel width

### Theme Selection
- **Prompt**: Add theme selection in preferences
- **Actions**:
  - Added Theme enum (Dark, Light, High Contrast Dark, Solarized Dark, Nord)
  - Implemented theme application via egui::Visuals
  - Added theme selector in Appearance settings tab
  - Themes persist to user settings file

### Preferred View Setting
- **Prompt**: Save preferred view in preferences
- **Actions**:
  - Added Perspective enum (Flat, ParentChild, Verification, References)
  - Added preferred_perspective to UserSettings
  - Load saved perspective on startup
  - Perspective selector in Appearance settings tab

### Tree View Navigation Fix
- **Prompt**: Arrow keys should follow tree view display order
- **Actions**:
  - Implemented collect_tree_indices_top_down() for Parent/Child and Verification views
  - Implemented collect_tree_indices_bottom_up() for References view
  - Navigation now follows actual display order in all perspectives

### Customizable Keybindings
- **Prompt**: Add keyboard mappings panel in settings
- **Actions**:
  - Added KeyAction enum for all bindable actions
  - Created KeyBinding struct with key name, modifiers (ctrl, shift, alt)
  - Added KeyBindings collection with defaults
  - Implemented Keybindings settings tab with capture mode
  - Key capture shows "Press a key..." with Escape to cancel
  - Reset to Defaults button restores default bindings
  - Replaced hardcoded key checks with keybinding lookups
  - Keybindings persist to user settings file

### Project Settings Tab
- **Prompt**: Add project settings for ID naming schemes in settings
- **Actions**:
  - Added Project tab to settings dialog
  - IdFormat selection (Single Level vs Two Level naming)
  - NumberingStrategy selection (Global, Per Prefix, Per Feature+Type)
  - Digit count configuration (1-6 digits)
  - Live example preview showing resulting ID format
  - Project settings stored in requirements.yaml file
  - Settings loaded on dialog open, saved with other settings

### ID Migration Support
- **Prompt**: Add validation and migration for ID settings changes
- **Actions**:
  - Added `IdConfigValidation` struct in requirements-core for validation results
  - Implemented `validate_id_config_change()` method to check proposed settings
  - Added `get_max_digits_in_use()` helper to find maximum digit count in existing IDs
  - Implemented `migrate_ids_to_config()` method to update all requirement IDs
  - Validation prevents digit reduction below existing maximum
  - Format changes require Global Sequential numbering
  - Added validation display in Project settings tab (errors in red, warnings in yellow)
  - Added "Migrate Existing IDs" button when settings differ from current
  - Implemented migration confirmation dialog with affected count and warnings
  - Updated user guide documentation with migration feature details

### Theme Cycling Shortcut
- **Prompt**: Ctrl-T should cycle through the themes
- **Actions**:
  - Added `CycleTheme` action to `KeyAction` enum
  - Added `next()` method to `Theme` enum for cycling through themes
  - Added default keybinding Ctrl+T in `KeyBindings::default()`
  - Added keybinding handler in update function to cycle and save theme
  - Theme order: Dark → Light → High Contrast Dark → Solarized Dark → Nord → Dark
  - Updated user guide documentation with new shortcut

### Markdown Support for Descriptions
- **Prompt**: Add markdown editor/preview for requirement descriptions
- **Actions**:
  - Added `egui_commonmark` crate (v0.18) for markdown rendering
  - Added `CommonMarkCache` to RequirementsApp state for caching rendered markdown
  - Updated detail view to render descriptions as markdown
  - Added preview toggle in edit form (Edit/Preview button)
  - Shows "Supports Markdown" hint in description field header
  - Reset preview mode when clearing form or loading requirement for edit
  - Updated user guide with Markdown support documentation

### Custom ID Prefix Override
- **Prompt**: Allow per-requirement prefix override for flexible ID organization
- **Actions**:
  - Added `prefix_override: Option<String>` field to Requirement model
  - Added `validate_prefix()` and `set_prefix_override()` methods for validation
  - Validation ensures prefix contains only uppercase letters (A-Z)
  - Updated `add_requirement_with_id()` to use prefix_override when set
  - Updated `generate_requirement_id_with_override()` for custom prefix ID generation
  - Updated migration functions to respect prefix_override
  - Added "ID Prefix" field to GUI form with validation indicator
  - Added `--prefix` option to CLI `add` command
  - Per Prefix numbering treats custom prefixes as their own counter
  - Global Sequential numbering uses shared counter regardless of prefix
  - Updated user guide documentation with custom prefix usage

### Prefix Update Bug Fix
- **Prompt**: Updating prefix doesn't update the spec_id, need conflict checking
- **Actions**:
  - Added `regenerate_spec_id_for_prefix_change()` method to RequirementsStore
  - Added `is_spec_id_available()` helper function
  - Rewrote `update_requirement()` in GUI to handle prefix changes properly
  - Checks for ID conflicts before allowing changes
  - Shows error message if new ID would conflict with existing requirement

### Collapsible Left Panel in Edit Mode
- **Prompt**: Keep left panel open in edit mode when window is wide enough with expand/collapse option
- **Actions**:
  - Added `left_panel_collapsed: bool` field to RequirementsApp state
  - Modified update() to conditionally show left panel based on screen width (900px minimum)
  - Added "▶ Hide" button in left panel header when in form view
  - Added "◀ Show List" button in central panel when panel is hidden
  - Updated show_list_panel() function signature to accept `in_form_view: bool`
  - Updated user guide with Responsive Layout section

---

## Git Operations Summary

### Key Commits
| Hash | Description |
|------|-------------|
| 93429bd | Initial commit |
| 8c240c3 | Export command |
| 31353f1 | SPEC-ID implementation |
| b5c4ae5 | Delete command |
| ca97e05 | Relationship system |
| 411edb4 | Workspace restructure |
| 4b91e82 | GUI CRUD operations |
| a16d853 | Threaded comments |
| 41096d3 | Change history |
| 3ec7ace | Tabbed interface |
| 4e96abf | Many improvements |

### Branches
- **main**: Primary development branch

---

## Technical Decisions

### Storage Format
- Chose YAML for human-readability and Git-friendliness
- All data in single requirements.yaml file per project

### ID System
- Dual ID system: UUID for internal use, SPEC-ID for human reference
- Configurable ID formats and numbering strategies

### GUI Framework
- Selected egui for cross-platform Rust GUI
- Immediate mode rendering for simplicity

### Architecture
- Workspace structure to share code between CLI and GUI
- Core library contains all business logic
- CLI and GUI are thin wrappers around core
