use eframe::egui;
use requirements_core::{
    Requirement, RequirementPriority, RequirementStatus, RequirementType,
    RequirementsStore, Storage, determine_requirements_path, Comment, FieldChange,
    RelationshipType, User,
};
use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Default base font size in points
const DEFAULT_FONT_SIZE: f32 = 14.0;
/// Minimum font size
const MIN_FONT_SIZE: f32 = 8.0;
/// Maximum font size
const MAX_FONT_SIZE: f32 = 32.0;
/// Font size step for zoom in/out
const FONT_SIZE_STEP: f32 = 1.0;

/// User settings for the GUI application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    /// User's full name
    pub name: String,
    /// User's email address
    pub email: String,
    /// User's nickname/handle for @mentions in comments
    pub handle: String,
    /// Base font size in points
    #[serde(default = "default_font_size")]
    pub base_font_size: f32,
    /// Preferred view perspective
    #[serde(default)]
    pub preferred_perspective: Perspective,
}

fn default_font_size() -> f32 {
    DEFAULT_FONT_SIZE
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            email: String::new(),
            handle: String::new(),
            base_font_size: DEFAULT_FONT_SIZE,
            preferred_perspective: Perspective::default(),
        }
    }
}

impl UserSettings {
    /// Get the default settings file path
    fn settings_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".requirements_gui_settings.yaml")
    }

    /// Load settings from file, or return defaults if not found
    pub fn load() -> Self {
        let path = Self::settings_path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(settings) = serde_yaml::from_str(&contents) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    /// Save settings to file
    pub fn save(&self) -> Result<(), String> {
        let path = Self::settings_path();
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        std::fs::write(&path, yaml)
            .map_err(|e| format!("Failed to write settings file: {}", e))?;
        Ok(())
    }

    /// Get the display name for use in comments/history
    /// Returns handle if set, otherwise name, otherwise "Unknown User"
    pub fn display_name(&self) -> String {
        if !self.handle.is_empty() {
            self.handle.clone()
        } else if !self.name.is_empty() {
            self.name.clone()
        } else {
            "Unknown User".to_string()
        }
    }
}

#[derive(Default, PartialEq, Clone)]
enum DetailTab {
    #[default]
    Description,
    Comments,
    Links,
    History,
}

#[derive(Default, PartialEq, Clone)]
enum SettingsTab {
    #[default]
    User,
    Appearance,
    Administration,
}

#[derive(Default, PartialEq, Clone)]
enum View {
    #[default]
    List,
    Detail,
    Add,
    Edit,
}

/// Perspective defines how requirements are organized in the list
#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub(crate) enum Perspective {
    /// Simple flat list of all requirements
    #[default]
    Flat,
    /// Tree view based on Parent/Child relationships
    ParentChild,
    /// Tree view based on Verifies/VerifiedBy relationships
    Verification,
    /// Tree view based on References relationships
    References,
}

impl Perspective {
    fn label(&self) -> &'static str {
        match self {
            Perspective::Flat => "Flat List",
            Perspective::ParentChild => "Parent/Child",
            Perspective::Verification => "Verification",
            Perspective::References => "References",
        }
    }

    /// Get the relationship types used for this perspective
    fn relationship_types(&self) -> Option<(RelationshipType, RelationshipType)> {
        match self {
            Perspective::Flat => None,
            Perspective::ParentChild => Some((RelationshipType::Parent, RelationshipType::Child)),
            Perspective::Verification => Some((RelationshipType::Verifies, RelationshipType::VerifiedBy)),
            Perspective::References => Some((RelationshipType::References, RelationshipType::References)),
        }
    }
}

/// Direction for viewing relationship hierarchies
#[derive(Default, PartialEq, Clone, Copy)]
enum PerspectiveDirection {
    /// View from parent/source to children/targets
    #[default]
    TopDown,
    /// View from children/targets to parents/sources
    BottomUp,
}

impl PerspectiveDirection {
    fn label(&self) -> &'static str {
        match self {
            PerspectiveDirection::TopDown => "Top-down",
            PerspectiveDirection::BottomUp => "Bottom-up",
        }
    }
}

pub struct RequirementsApp {
    storage: Storage,
    store: RequirementsStore,
    current_view: View,
    selected_idx: Option<usize>,
    filter_text: String,
    active_tab: DetailTab,

    // Form state
    form_title: String,
    form_description: String,
    form_status: RequirementStatus,
    form_priority: RequirementPriority,
    form_type: RequirementType,
    form_owner: String,
    form_feature: String,
    form_tags: String,
    form_parent_id: Option<Uuid>,  // Parent to link new requirement to

    // Messages
    message: Option<(String, bool)>, // (message, is_error)

    // Comment state
    comment_author: String,
    comment_content: String,
    show_add_comment: bool,
    reply_to_comment: Option<Uuid>, // Parent comment ID for replies
    collapsed_comments: HashMap<Uuid, bool>, // Track which comments are collapsed
    #[allow(dead_code)]
    edit_comment_id: Option<Uuid>,

    // Pending operations (to avoid borrow checker issues)
    pending_delete: Option<usize>,
    pending_view_change: Option<View>,
    pending_comment_add: Option<(String, String, Option<Uuid>)>, // (author, content, parent_id)
    pending_comment_delete: Option<Uuid>,

    // Settings
    user_settings: UserSettings,
    show_settings_dialog: bool,
    settings_tab: SettingsTab,
    settings_form_name: String,
    settings_form_email: String,
    settings_form_handle: String,
    settings_form_font_size: f32,
    settings_form_perspective: Perspective,

    // User management
    show_user_form: bool,
    editing_user_id: Option<Uuid>,
    user_form_name: String,
    user_form_email: String,
    user_form_handle: String,
    show_archived_users: bool,

    // Font size (runtime, can differ from saved base)
    current_font_size: f32,

    // Perspective and filtering
    perspective: Perspective,
    perspective_direction: PerspectiveDirection,
    filter_types: HashSet<RequirementType>,      // Empty = show all
    filter_features: HashSet<String>,            // Empty = show all
    tree_collapsed: HashMap<Uuid, bool>,         // Track collapsed tree nodes
    show_filter_panel: bool,                     // Toggle filter panel visibility
    show_archived: bool,                         // Whether to show archived requirements

    // Drag and drop for relationships
    drag_source: Option<usize>,                  // Index of requirement being dragged
    drop_target: Option<usize>,                  // Index of requirement being hovered over
    pending_relationship: Option<(usize, usize)>, // (source_idx, target_idx) to create relationship
}

impl RequirementsApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let requirements_path = determine_requirements_path(None)
            .unwrap_or_else(|_| std::path::PathBuf::from("requirements.yaml"));

        let storage = Storage::new(requirements_path);
        let store = storage.load().unwrap_or_else(|_| RequirementsStore::new());
        let user_settings = UserSettings::load();

        // Apply saved preferences
        let initial_font_size = user_settings.base_font_size;
        let initial_perspective = user_settings.preferred_perspective.clone();

        Self {
            storage,
            store,
            current_view: View::List,
            selected_idx: None,
            filter_text: String::new(),
            active_tab: DetailTab::Description,
            form_title: String::new(),
            form_description: String::new(),
            form_status: RequirementStatus::Draft,
            form_priority: RequirementPriority::Medium,
            form_type: RequirementType::Functional,
            form_owner: String::new(),
            form_feature: String::from("Uncategorized"),
            form_tags: String::new(),
            form_parent_id: None,
            message: None,
            comment_author: String::new(),
            comment_content: String::new(),
            show_add_comment: false,
            reply_to_comment: None,
            collapsed_comments: HashMap::new(),
            edit_comment_id: None,
            pending_delete: None,
            pending_view_change: None,
            pending_comment_add: None,
            pending_comment_delete: None,
            current_font_size: initial_font_size,
            user_settings,
            show_settings_dialog: false,
            settings_tab: SettingsTab::default(),
            settings_form_name: String::new(),
            settings_form_email: String::new(),
            settings_form_handle: String::new(),
            settings_form_font_size: DEFAULT_FONT_SIZE,
            settings_form_perspective: Perspective::default(),
            show_user_form: false,
            editing_user_id: None,
            user_form_name: String::new(),
            user_form_email: String::new(),
            user_form_handle: String::new(),
            show_archived_users: false,
            perspective: initial_perspective,
            perspective_direction: PerspectiveDirection::default(),
            filter_types: HashSet::new(),
            filter_features: HashSet::new(),
            tree_collapsed: HashMap::new(),
            show_filter_panel: false,
            show_archived: false,
            drag_source: None,
            drop_target: None,
            pending_relationship: None,
        }
    }

    /// Increase font size by one step
    fn zoom_in(&mut self) {
        self.current_font_size = (self.current_font_size + FONT_SIZE_STEP).min(MAX_FONT_SIZE);
    }

    /// Decrease font size by one step
    fn zoom_out(&mut self) {
        self.current_font_size = (self.current_font_size - FONT_SIZE_STEP).max(MIN_FONT_SIZE);
    }

    /// Reset font size to base setting
    fn reset_zoom(&mut self) {
        self.current_font_size = self.user_settings.base_font_size;
    }

    /// Open the user guide in the default browser
    fn open_user_guide() {
        // Get the path to the docs directory relative to the executable
        let exe_path = std::env::current_exe().ok();

        let possible_paths: Vec<PathBuf> = if let Some(ref exe) = exe_path {
            vec![
                // Relative to executable (for installed binaries)
                exe.parent().unwrap().join("../docs"),
                exe.parent().unwrap().join("../../docs"),
                // Development paths
                exe.parent().unwrap().join("../../../docs"),
                exe.parent().unwrap().join("../../../../docs"),
                // Current directory
                std::env::current_dir().unwrap_or_default().join("docs"),
                // Project root (when running from project directory)
                PathBuf::from("docs"),
            ]
        } else {
            vec![
                std::env::current_dir().unwrap_or_default().join("docs"),
                PathBuf::from("docs"),
            ]
        };

        let filename = "user-guide.html";

        // Find the first path that exists
        let doc_path = possible_paths.iter()
            .map(|p| p.join(filename))
            .find(|p| p.exists());

        if let Some(path) = doc_path {
            if let Ok(canonical) = path.canonicalize() {
                let url = format!("file://{}", canonical.to_string_lossy());

                // Try to open in browser using platform-specific commands
                #[cfg(target_os = "linux")]
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&url)
                        .spawn();
                }

                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open")
                        .arg(&url)
                        .spawn();
                }

                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", &url])
                        .spawn();
                }
            }
        }
    }

    fn reload(&mut self) {
        if let Ok(store) = self.storage.load() {
            self.store = store;
            self.message = Some(("Reloaded successfully".to_string(), false));
        } else {
            self.message = Some(("Failed to reload".to_string(), true));
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.storage.save(&self.store) {
            self.message = Some((format!("Error saving: {}", e), true));
        } else {
            self.message = Some(("Saved successfully".to_string(), false));
        }
    }

    fn clear_form(&mut self) {
        self.form_title.clear();
        self.form_description.clear();
        self.form_status = RequirementStatus::Draft;
        self.form_priority = RequirementPriority::Medium;
        self.form_type = RequirementType::Functional;
        self.form_owner.clear();
        self.form_feature = String::from("Uncategorized");
        self.form_tags.clear();

        // If a requirement is selected, pre-populate parent relationship
        self.form_parent_id = self.selected_idx
            .and_then(|idx| self.store.requirements.get(idx))
            .map(|req| req.id);
    }

    fn load_form_from_requirement(&mut self, idx: usize) {
        if let Some(req) = self.store.requirements.get(idx) {
            self.form_title = req.title.clone();
            self.form_description = req.description.clone();
            self.form_status = req.status.clone();
            self.form_priority = req.priority.clone();
            self.form_type = req.req_type.clone();
            self.form_owner = req.owner.clone();
            self.form_feature = req.feature.clone();
            let tags_vec: Vec<String> = req.tags.iter().cloned().collect();
            self.form_tags = tags_vec.join(", ");
        }
    }

    fn add_requirement(&mut self) {
        let tags: HashSet<String> = self.form_tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut req = Requirement::new(
            self.form_title.clone(),
            self.form_description.clone(),
        );
        req.status = self.form_status.clone();
        req.priority = self.form_priority.clone();
        req.req_type = self.form_type.clone();
        req.owner = self.form_owner.clone();
        req.feature = self.form_feature.clone();
        req.tags = tags;

        // Store parent ID before clearing form
        let parent_id = self.form_parent_id;

        // Get prefixes for ID generation
        let feature_prefix = self.store.get_feature_by_name(&req.feature)
            .map(|f| f.prefix.clone());
        let type_prefix = self.store.get_type_prefix(&req.req_type);

        // Capture the new requirement's ID before adding
        let new_req_id = req.id;

        // Add requirement with auto-assigned ID based on configuration
        self.store.add_requirement_with_id(
            req,
            feature_prefix.as_deref(),
            type_prefix.as_deref(),
        );

        // Create parent relationship if specified
        if let Some(parent_id) = parent_id {
            // New requirement (child) stores Parent relationship pointing to parent
            let _ = self.store.add_relationship(
                &new_req_id,
                RelationshipType::Parent,
                &parent_id,
                true, // bidirectional
            );
        }

        self.save();
        self.form_parent_id = None; // Clear parent after adding
        self.clear_form();
        self.current_view = View::List;
        self.message = Some(("Requirement added successfully".to_string(), false));
    }

    fn update_requirement(&mut self, idx: usize) {
        // First, compute any spec_id changes needed before mutable borrow
        let new_spec_id = if let Some(req) = self.store.requirements.get(idx) {
            if self.form_type != req.req_type {
                self.store.update_spec_id_for_type_change(
                    req.spec_id.as_deref(),
                    &self.form_type,
                )
            } else {
                None
            }
        } else {
            None
        };

        if let Some(req) = self.store.requirements.get_mut(idx) {
            let mut changes: Vec<FieldChange> = Vec::new();

            // Track title change
            if self.form_title != req.title {
                changes.push(Requirement::field_change("title", req.title.clone(), self.form_title.clone()));
                req.title = self.form_title.clone();
            }

            // Track description change
            if self.form_description != req.description {
                changes.push(Requirement::field_change("description", req.description.clone(), self.form_description.clone()));
                req.description = self.form_description.clone();
            }

            // Track status change
            if self.form_status != req.status {
                changes.push(Requirement::field_change("status", format!("{:?}", req.status), format!("{:?}", self.form_status)));
                req.status = self.form_status.clone();
            }

            // Track priority change
            if self.form_priority != req.priority {
                changes.push(Requirement::field_change("priority", format!("{:?}", req.priority), format!("{:?}", self.form_priority)));
                req.priority = self.form_priority.clone();
            }

            // Track type change and update spec_id prefix
            if self.form_type != req.req_type {
                changes.push(Requirement::field_change("type", format!("{:?}", req.req_type), format!("{:?}", self.form_type)));

                // Update the spec_id to reflect the new type prefix
                if let Some(ref new_id) = new_spec_id {
                    if req.spec_id.as_deref() != Some(new_id.as_str()) {
                        let old_spec_id = req.spec_id.clone().unwrap_or_default();
                        changes.push(Requirement::field_change("spec_id", old_spec_id, new_id.clone()));
                        req.spec_id = Some(new_id.clone());
                    }
                }

                req.req_type = self.form_type.clone();
            }

            // Track owner change
            if self.form_owner != req.owner {
                changes.push(Requirement::field_change("owner", req.owner.clone(), self.form_owner.clone()));
                req.owner = self.form_owner.clone();
            }

            // Track feature change
            if self.form_feature != req.feature {
                changes.push(Requirement::field_change("feature", req.feature.clone(), self.form_feature.clone()));
                req.feature = self.form_feature.clone();
            }

            // Track tags change
            let new_tags: HashSet<String> = self.form_tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if new_tags != req.tags {
                let old_tags_vec: Vec<String> = req.tags.iter().cloned().collect();
                let new_tags_vec: Vec<String> = new_tags.iter().cloned().collect();
                changes.push(Requirement::field_change("tags", old_tags_vec.join(", "), new_tags_vec.join(", ")));
                req.tags = new_tags;
            }

            // Record changes with author from user settings
            req.record_change(self.user_settings.display_name(), changes);

            self.save();
            self.clear_form();
            self.current_view = View::Detail;
            self.message = Some(("Requirement updated successfully".to_string(), false));
        }
    }

    fn delete_requirement(&mut self, idx: usize) {
        if idx < self.store.requirements.len() {
            self.store.requirements.remove(idx);
            self.save();
            self.selected_idx = None;
            self.current_view = View::List;
            self.message = Some(("Requirement deleted successfully".to_string(), false));
        }
    }

    fn toggle_archive(&mut self, idx: usize) {
        let (new_archived, author) = {
            if let Some(req) = self.store.requirements.get(idx) {
                (!req.archived, self.user_settings.display_name())
            } else {
                return;
            }
        };

        if let Some(req) = self.store.requirements.get_mut(idx) {
            let was_archived = req.archived;
            req.archived = new_archived;

            // Record change in history
            let change = Requirement::field_change(
                "archived",
                was_archived.to_string(),
                new_archived.to_string(),
            );
            req.record_change(author, vec![change]);
        }

        self.save();
        let action = if new_archived { "archived" } else { "unarchived" };
        self.message = Some((format!("Requirement {}", action), false));
    }

    fn show_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if ui.button("➕ Add").clicked() {
                    self.clear_form();
                    self.pending_view_change = Some(View::Add);
                }

                if ui.button("🔄 Reload").clicked() {
                    self.reload();
                }

                ui.separator();
                ui.label(format!("Requirements: {}", self.store.requirements.len()));

                // Show message
                if let Some((msg, is_error)) = &self.message {
                    ui.separator();
                    let color = if *is_error { egui::Color32::RED } else { egui::Color32::GREEN };
                    ui.colored_label(color, msg);
                }

                // Settings and help buttons (right-aligned)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙").on_hover_text("Settings").clicked() {
                        // Load current settings into form
                        self.settings_form_name = self.user_settings.name.clone();
                        self.settings_form_email = self.user_settings.email.clone();
                        self.settings_form_handle = self.user_settings.handle.clone();
                        self.settings_form_font_size = self.user_settings.base_font_size;
                        self.settings_form_perspective = self.user_settings.preferred_perspective.clone();
                        self.show_settings_dialog = true;
                    }
                    if ui.button("?").on_hover_text("Help - Open User Guide").clicked() {
                        Self::open_user_guide();
                    }
                    // Show current zoom level
                    ui.label(format!("{}pt", self.current_font_size as i32));
                });
            });
        });
    }

    fn show_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_settings_dialog {
            return;
        }

        egui::Window::new("⚙ Settings")
            .collapsible(false)
            .resizable(true)
            .min_width(400.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // Tabs
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::User, "👤 User");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Appearance, "🎨 Appearance");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Administration, "🔧 Administration");
                });

                ui.separator();
                ui.add_space(10.0);

                // Tab content
                match self.settings_tab {
                    SettingsTab::User => {
                        self.show_settings_user_tab(ui);
                    }
                    SettingsTab::Appearance => {
                        self.show_settings_appearance_tab(ui);
                    }
                    SettingsTab::Administration => {
                        self.show_settings_admin_tab(ui);
                    }
                }

                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        // Update settings from form
                        self.user_settings.name = self.settings_form_name.clone();
                        self.user_settings.email = self.settings_form_email.clone();
                        self.user_settings.handle = self.settings_form_handle.clone();
                        self.user_settings.base_font_size = self.settings_form_font_size;
                        self.user_settings.preferred_perspective = self.settings_form_perspective.clone();

                        // Apply the new base font size as current
                        self.current_font_size = self.settings_form_font_size;

                        // Apply the new preferred perspective
                        self.perspective = self.settings_form_perspective.clone();

                        // Save to file
                        match self.user_settings.save() {
                            Ok(()) => {
                                self.message = Some(("Settings saved successfully".to_string(), false));
                            }
                            Err(e) => {
                                self.message = Some((format!("Failed to save settings: {}", e), true));
                            }
                        }
                        self.show_settings_dialog = false;
                    }

                    if ui.button("❌ Cancel").clicked() {
                        self.show_settings_dialog = false;
                    }
                });
            });
    }

    fn show_settings_user_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("User Profile");
        ui.add_space(10.0);

        egui::Grid::new("settings_user_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut self.settings_form_name)
                    .hint_text("Your full name"));
                ui.end_row();

                ui.label("Email:");
                ui.add(egui::TextEdit::singleline(&mut self.settings_form_email)
                    .hint_text("your.email@example.com"));
                ui.end_row();

                ui.label("Handle (@):");
                ui.add(egui::TextEdit::singleline(&mut self.settings_form_handle)
                    .hint_text("nickname for @mentions"));
                ui.end_row();
            });
    }

    fn show_settings_appearance_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Display Settings");
        ui.add_space(10.0);

        egui::Grid::new("settings_appearance_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                ui.label("Base Font Size:");
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.settings_form_font_size, MIN_FONT_SIZE..=MAX_FONT_SIZE)
                        .suffix("pt")
                        .step_by(1.0));
                    if ui.button("Reset").clicked() {
                        self.settings_form_font_size = DEFAULT_FONT_SIZE;
                    }
                });
                ui.end_row();

                ui.label("Default View:");
                egui::ComboBox::from_id_salt("settings_perspective_combo")
                    .selected_text(self.settings_form_perspective.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings_form_perspective, Perspective::Flat, Perspective::Flat.label());
                        ui.selectable_value(&mut self.settings_form_perspective, Perspective::ParentChild, Perspective::ParentChild.label());
                        ui.selectable_value(&mut self.settings_form_perspective, Perspective::Verification, Perspective::Verification.label());
                        ui.selectable_value(&mut self.settings_form_perspective, Perspective::References, Perspective::References.label());
                    });
                ui.end_row();
            });

        ui.add_space(5.0);
        ui.label("Tip: Use Ctrl+MouseWheel or Ctrl+Plus/Minus to zoom");
    }

    fn show_settings_admin_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // User Management Section
            ui.heading("User Management");
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                if ui.button("➕ Add User").clicked() {
                    self.show_user_form = true;
                    self.editing_user_id = None;
                    self.user_form_name.clear();
                    self.user_form_email.clear();
                    self.user_form_handle.clear();
                }
                ui.checkbox(&mut self.show_archived_users, "Show Archived");
            });

            ui.add_space(5.0);

            // User form (inline)
            if self.show_user_form {
                ui.group(|ui| {
                    let title = if self.editing_user_id.is_some() { "Edit User" } else { "Add User" };
                    ui.label(title);

                    egui::Grid::new("user_form_grid")
                        .num_columns(2)
                        .spacing([10.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut self.user_form_name);
                            ui.end_row();

                            ui.label("Email:");
                            ui.text_edit_singleline(&mut self.user_form_email);
                            ui.end_row();

                            ui.label("Handle:");
                            ui.text_edit_singleline(&mut self.user_form_handle);
                            ui.end_row();
                        });

                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.editing_user_id.is_some() {
                                self.save_edited_user();
                            } else {
                                self.add_new_user();
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_user_form = false;
                            self.editing_user_id = None;
                        }
                    });
                });
                ui.add_space(5.0);
            }

            // Users table
            self.show_users_table(ui);

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Database Management Section
            ui.heading("Database Management");
            ui.add_space(5.0);

            if ui.button("📦 Backup Database").clicked() {
                self.backup_database();
            }
            ui.label("Creates a timestamped backup of the requirements database.");

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Statistics Section
            ui.heading("Database Statistics");
            ui.add_space(5.0);

            let total_reqs = self.store.requirements.len();
            let archived_reqs = self.store.requirements.iter().filter(|r| r.archived).count();
            let active_reqs = total_reqs - archived_reqs;
            let total_users = self.store.users.len();
            let archived_users = self.store.users.iter().filter(|u| u.archived).count();

            egui::Grid::new("settings_stats_grid")
                .num_columns(2)
                .spacing([20.0, 5.0])
                .show(ui, |ui| {
                    ui.label("Total Requirements:");
                    ui.label(format!("{}", total_reqs));
                    ui.end_row();

                    ui.label("Active Requirements:");
                    ui.label(format!("{}", active_reqs));
                    ui.end_row();

                    ui.label("Archived Requirements:");
                    ui.label(format!("{}", archived_reqs));
                    ui.end_row();

                    ui.label("Total Users:");
                    ui.label(format!("{}", total_users));
                    ui.end_row();

                    ui.label("Archived Users:");
                    ui.label(format!("{}", archived_users));
                    ui.end_row();

                    ui.label("Database Path:");
                    ui.label(self.storage.path().display().to_string());
                    ui.end_row();
                });
        });
    }

    fn show_users_table(&mut self, ui: &mut egui::Ui) {
        // Collect user data to avoid borrow issues
        let users_data: Vec<(Uuid, String, String, String, bool)> = self.store.users
            .iter()
            .filter(|u| self.show_archived_users || !u.archived)
            .map(|u| (u.id, u.name.clone(), u.email.clone(), u.handle.clone(), u.archived))
            .collect();

        if users_data.is_empty() {
            ui.label("No users defined.");
            return;
        }

        egui::Grid::new("users_table")
            .num_columns(5)
            .striped(true)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
                // Header
                ui.strong("Name");
                ui.strong("Email");
                ui.strong("Handle");
                ui.strong("Status");
                ui.strong("Actions");
                ui.end_row();

                for (id, name, email, handle, archived) in &users_data {
                    ui.label(name);
                    ui.label(email);
                    ui.label(format!("@{}", handle));
                    ui.label(if *archived { "Archived" } else { "Active" });

                    ui.horizontal(|ui| {
                        if ui.small_button("✏").on_hover_text("Edit").clicked() {
                            self.editing_user_id = Some(*id);
                            self.user_form_name = name.clone();
                            self.user_form_email = email.clone();
                            self.user_form_handle = handle.clone();
                            self.show_user_form = true;
                        }

                        let archive_label = if *archived { "Unarchive" } else { "Archive" };
                        if ui.small_button(if *archived { "↩" } else { "📁" })
                            .on_hover_text(archive_label)
                            .clicked()
                        {
                            if let Some(user) = self.store.get_user_by_id_mut(id) {
                                user.archived = !user.archived;
                            }
                            self.save();
                        }

                        if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                            self.store.remove_user(id);
                            self.save();
                        }
                    });
                    ui.end_row();
                }
            });
    }

    fn add_new_user(&mut self) {
        if self.user_form_name.is_empty() {
            self.message = Some(("User name is required".to_string(), true));
            return;
        }

        let user = User::new(
            self.user_form_name.clone(),
            self.user_form_email.clone(),
            self.user_form_handle.clone(),
        );
        self.store.add_user(user);
        self.save();

        self.show_user_form = false;
        self.user_form_name.clear();
        self.user_form_email.clear();
        self.user_form_handle.clear();
        self.message = Some(("User added successfully".to_string(), false));
    }

    fn save_edited_user(&mut self) {
        if let Some(user_id) = self.editing_user_id {
            if let Some(user) = self.store.get_user_by_id_mut(&user_id) {
                user.name = self.user_form_name.clone();
                user.email = self.user_form_email.clone();
                user.handle = self.user_form_handle.clone();
            }
            self.save();

            self.show_user_form = false;
            self.editing_user_id = None;
            self.user_form_name.clear();
            self.user_form_email.clear();
            self.user_form_handle.clear();
            self.message = Some(("User updated successfully".to_string(), false));
        }
    }

    fn backup_database(&mut self) {
        use chrono::Local;

        let db_path = self.storage.path();
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");

        // Create backup filename with timestamp
        let backup_name = if let Some(stem) = db_path.file_stem() {
            let ext = db_path.extension().map(|e| e.to_str().unwrap_or("yaml")).unwrap_or("yaml");
            format!("{}_{}.{}", stem.to_str().unwrap_or("requirements"), timestamp, ext)
        } else {
            format!("requirements_backup_{}.yaml", timestamp)
        };

        let backup_path = db_path.parent()
            .map(|p| p.join(&backup_name))
            .unwrap_or_else(|| std::path::PathBuf::from(&backup_name));

        match std::fs::copy(db_path, &backup_path) {
            Ok(_) => {
                self.message = Some((format!("Backup created: {}", backup_name), false));
            }
            Err(e) => {
                self.message = Some((format!("Backup failed: {}", e), true));
            }
        }
    }

    /// Get indices of requirements that pass the current filters (in display order)
    /// For flat view, returns in storage order. For tree views, returns in tree traversal order.
    fn get_filtered_indices(&self) -> Vec<usize> {
        match &self.perspective {
            Perspective::Flat => {
                // Flat view: simple filtered list in storage order
                self.store.requirements
                    .iter()
                    .enumerate()
                    .filter(|(_, req)| self.passes_filters(req))
                    .map(|(idx, _)| idx)
                    .collect()
            }
            _ => {
                // Tree view: traverse in display order
                let Some((outgoing_type, _)) = self.perspective.relationship_types() else {
                    return Vec::new();
                };

                let mut result = Vec::new();
                match self.perspective_direction {
                    PerspectiveDirection::TopDown => {
                        let leaves = self.find_tree_leaves(&outgoing_type);
                        for leaf_idx in leaves {
                            self.collect_tree_indices_bottom_up(leaf_idx, &outgoing_type, &mut result);
                        }
                    }
                    PerspectiveDirection::BottomUp => {
                        let roots = self.find_tree_roots(&outgoing_type);
                        for root_idx in roots {
                            self.collect_tree_indices_top_down(root_idx, &outgoing_type, &mut result);
                        }
                    }
                }
                result
            }
        }
    }

    /// Collect tree indices in top-down order (roots first, then children)
    fn collect_tree_indices_top_down(&self, idx: usize, outgoing_rel_type: &RelationshipType, result: &mut Vec<usize>) {
        // Don't add duplicates
        if result.contains(&idx) {
            return;
        }

        // Check if this node is collapsed
        let is_collapsed = self.store.requirements.get(idx)
            .map(|req| self.tree_collapsed.get(&req.id).copied().unwrap_or(false))
            .unwrap_or(false);

        result.push(idx);

        // If not collapsed, add children recursively
        if !is_collapsed {
            if let Some(req) = self.store.requirements.get(idx) {
                let children = self.get_children(&req.id, outgoing_rel_type);
                for child_idx in children {
                    self.collect_tree_indices_top_down(child_idx, outgoing_rel_type, result);
                }
            }
        }
    }

    /// Collect tree indices in bottom-up order (leaves first, then parents)
    fn collect_tree_indices_bottom_up(&self, idx: usize, outgoing_rel_type: &RelationshipType, result: &mut Vec<usize>) {
        // Don't add duplicates
        if result.contains(&idx) {
            return;
        }

        // Check if this node is collapsed
        let is_collapsed = self.store.requirements.get(idx)
            .map(|req| self.tree_collapsed.get(&req.id).copied().unwrap_or(false))
            .unwrap_or(false);

        result.push(idx);

        // If not collapsed, add parents recursively
        if !is_collapsed {
            if let Some(req) = self.store.requirements.get(idx) {
                let parents = self.get_parents(&req.id, outgoing_rel_type);
                for parent_idx in parents {
                    self.collect_tree_indices_bottom_up(parent_idx, outgoing_rel_type, result);
                }
            }
        }
    }

    /// Check if a requirement passes the current filters
    fn passes_filters(&self, req: &Requirement) -> bool {
        // Text search filter
        if !self.filter_text.is_empty() {
            let search = self.filter_text.to_lowercase();
            if !req.title.to_lowercase().contains(&search)
                && !req.description.to_lowercase().contains(&search)
                && !req.spec_id.as_ref().map(|s| s.to_lowercase().contains(&search)).unwrap_or(false)
            {
                return false;
            }
        }

        // Type filter (empty = show all)
        if !self.filter_types.is_empty() && !self.filter_types.contains(&req.req_type) {
            return false;
        }

        // Feature filter (empty = show all)
        if !self.filter_features.is_empty() && !self.filter_features.contains(&req.feature) {
            return false;
        }

        // Archive filter (hide archived unless show_archived is true)
        if req.archived && !self.show_archived {
            return false;
        }

        true
    }

    /// Get all unique feature names from requirements
    fn get_all_features(&self) -> Vec<String> {
        let mut features: Vec<String> = self.store.requirements
            .iter()
            .map(|r| r.feature.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        features.sort();
        features
    }

    /// Find root nodes for tree view (requirements that are not children of any other requirement)
    /// For Parent/Child: roots are requirements that no one has a Parent relationship pointing to
    fn find_tree_roots(&self, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        // Collect all requirement IDs that are targets of the outgoing relationship type
        // These are the "children" - the ones that parents point to
        let mut is_child: HashSet<Uuid> = HashSet::new();

        for req in &self.store.requirements {
            for rel in &req.relationships {
                if &rel.rel_type == outgoing_rel_type {
                    // This requirement has a Parent/outgoing relationship to target
                    // So target is a child
                    is_child.insert(rel.target_id);
                }
            }
        }

        // Return indices of requirements that are NOT children (i.e., they are roots)
        self.store.requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| !is_child.contains(&req.id) && self.passes_filters(req))
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Get children of a requirement for a given relationship type
    fn get_children(&self, parent_id: &Uuid, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        // Find the parent requirement
        if let Some(parent) = self.store.requirements.iter().find(|r| &r.id == parent_id) {
            // Get all target IDs where relationship type matches
            let child_ids: Vec<Uuid> = parent.relationships
                .iter()
                .filter(|r| &r.rel_type == outgoing_rel_type)
                .map(|r| r.target_id)
                .collect();

            // Convert to indices, filtering by current filters
            self.store.requirements
                .iter()
                .enumerate()
                .filter(|(_, req)| child_ids.contains(&req.id) && self.passes_filters(req))
                .map(|(idx, _)| idx)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get parents of a requirement for a given relationship type (for bottom-up view)
    /// Finds requirements that have an outgoing relationship (e.g., Parent) pointing to this child
    fn get_parents(&self, child_id: &Uuid, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        // Find all requirements that have an outgoing relationship to this child
        // e.g., find all requirements with a "Parent" relationship where target_id == child_id
        self.store.requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| {
                self.passes_filters(req) &&
                req.relationships.iter().any(|r|
                    &r.rel_type == outgoing_rel_type && &r.target_id == child_id
                )
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Find leaf nodes for bottom-up tree view (requirements with no outgoing relationships of the type)
    fn find_tree_leaves(&self, outgoing_rel_type: &RelationshipType) -> Vec<usize> {
        self.store.requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| {
                self.passes_filters(req) &&
                !req.relationships.iter().any(|r| &r.rel_type == outgoing_rel_type)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Create a relationship based on drag-drop action and current perspective
    /// Returns (source_idx, target_idx, relationship_type) where source stores the relationship to target
    fn get_relationship_for_drop(&self, dragged_idx: usize, drop_target_idx: usize) -> Option<(usize, usize, RelationshipType)> {
        if dragged_idx == drop_target_idx {
            return None; // Can't create relationship to self
        }

        let (outgoing_type, _incoming_type) = self.perspective.relationship_types()?;

        // In Parent/Child perspective:
        // - outgoing_type is Parent (stored on the child, pointing to the parent)
        // - When dragging in top-down: drop target becomes parent of dragged item
        //   So dragged (child) gets a Parent relationship pointing to drop_target (parent)
        // - When dragging in bottom-up: dragged item becomes parent of drop target
        //   So drop_target (child) gets a Parent relationship pointing to dragged (parent)
        match self.perspective_direction {
            PerspectiveDirection::TopDown => {
                // Drop target becomes parent of dragged
                // Dragged (child) stores Parent relationship pointing to drop_target (parent)
                Some((dragged_idx, drop_target_idx, outgoing_type))
            }
            PerspectiveDirection::BottomUp => {
                // Dragged becomes parent of drop_target
                // drop_target (child) stores Parent relationship pointing to dragged (parent)
                Some((drop_target_idx, dragged_idx, outgoing_type))
            }
        }
    }

    /// Create a relationship between two requirements
    fn create_relationship_from_drop(&mut self, dragged_idx: usize, drop_target_idx: usize) {
        if let Some((source_idx, target_idx, rel_type)) = self.get_relationship_for_drop(dragged_idx, drop_target_idx) {
            let source_id = self.store.requirements.get(source_idx).map(|r| r.id);
            let target_id = self.store.requirements.get(target_idx).map(|r| r.id);

            if let (Some(source_id), Some(target_id)) = (source_id, target_id) {
                // Set the relationship (replaces any existing relationship of same type)
                // source stores the relationship pointing to target
                // Bidirectional for parent/child types (adds inverse on target)
                let bidirectional = matches!(rel_type, RelationshipType::Parent | RelationshipType::Verifies);
                match self.store.set_relationship(&source_id, rel_type.clone(), &target_id, bidirectional) {
                    Ok(()) => {
                        self.save();
                        let rel_name = format!("{:?}", rel_type);
                        self.message = Some((format!("Relationship '{}' set", rel_name), false));
                    }
                    Err(e) => {
                        self.message = Some((format!("Failed to set relationship: {}", e), true));
                    }
                }
            }
        }
    }

    fn show_list_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("list_panel").min_width(400.0).show(ctx, |ui| {
            ui.heading("Requirements");
            ui.separator();

            // Search bar
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(&mut self.filter_text)
                    .hint_text("Search...")
                    .desired_width(150.0));

                // Filter toggle button
                let filter_active = !self.filter_types.is_empty() || !self.filter_features.is_empty();
                let filter_btn_text = if filter_active { "🔽 Filters ●" } else { "🔽 Filters" };
                if ui.button(filter_btn_text).clicked() {
                    self.show_filter_panel = !self.show_filter_panel;
                }
            });

            // Perspective selector
            ui.horizontal(|ui| {
                ui.label("View:");
                egui::ComboBox::from_id_salt("perspective_combo")
                    .selected_text(self.perspective.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.perspective, Perspective::Flat, Perspective::Flat.label());
                        ui.selectable_value(&mut self.perspective, Perspective::ParentChild, Perspective::ParentChild.label());
                        ui.selectable_value(&mut self.perspective, Perspective::Verification, Perspective::Verification.label());
                        ui.selectable_value(&mut self.perspective, Perspective::References, Perspective::References.label());
                    });

                // Direction selector (only shown for non-flat perspectives)
                if self.perspective != Perspective::Flat {
                    egui::ComboBox::from_id_salt("direction_combo")
                        .selected_text(self.perspective_direction.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.perspective_direction, PerspectiveDirection::TopDown, "Top-down ↓");
                            ui.selectable_value(&mut self.perspective_direction, PerspectiveDirection::BottomUp, "Bottom-up ↑");
                        });
                }
            });

            // Collapsible filter panel
            if self.show_filter_panel {
                ui.separator();
                self.show_filter_controls(ui);
            }

            ui.separator();

            // Requirement list (flat or tree)
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &self.perspective {
                    Perspective::Flat => {
                        self.show_flat_list(ui);
                    }
                    _ => {
                        self.show_tree_list(ui);
                    }
                }
            });
        });
    }

    fn show_filter_controls(&mut self, ui: &mut egui::Ui) {
        ui.label("Type Filters:");
        ui.horizontal_wrapped(|ui| {
            let types = [
                (RequirementType::Functional, "FR"),
                (RequirementType::NonFunctional, "NFR"),
                (RequirementType::System, "SR"),
                (RequirementType::User, "UR"),
                (RequirementType::ChangeRequest, "CR"),
            ];

            for (req_type, label) in types {
                let mut checked = self.filter_types.contains(&req_type);
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        self.filter_types.insert(req_type);
                    } else {
                        self.filter_types.remove(&req_type);
                    }
                }
            }

            if ui.small_button("Clear").clicked() {
                self.filter_types.clear();
            }
        });

        ui.add_space(5.0);
        ui.label("Feature Filters:");

        let features = self.get_all_features();
        ui.horizontal_wrapped(|ui| {
            for feature in &features {
                let mut checked = self.filter_features.contains(feature);
                // Truncate long feature names for display
                let display_name = if feature.len() > 15 {
                    format!("{}...", &feature[..12])
                } else {
                    feature.clone()
                };

                if ui.checkbox(&mut checked, &display_name).on_hover_text(feature).changed() {
                    if checked {
                        self.filter_features.insert(feature.clone());
                    } else {
                        self.filter_features.remove(feature);
                    }
                }
            }

            if ui.small_button("Clear").clicked() {
                self.filter_features.clear();
            }
        });

        ui.add_space(5.0);
        ui.checkbox(&mut self.show_archived, "Show Archived");
    }

    fn show_flat_list(&mut self, ui: &mut egui::Ui) {
        // Collect filtered indices first to avoid borrow issues
        let filtered_indices: Vec<usize> = self.store.requirements
            .iter()
            .enumerate()
            .filter(|(_, req)| self.passes_filters(req))
            .map(|(idx, _)| idx)
            .collect();

        for idx in filtered_indices {
            self.show_draggable_requirement(ui, idx, 0);
        }
    }

    /// Render a single requirement item with drag-and-drop support
    fn show_draggable_requirement(&mut self, ui: &mut egui::Ui, idx: usize, indent: usize) {
        let Some(req) = self.store.requirements.get(idx) else { return };

        let spec_id = req.spec_id.clone();
        let title = req.title.clone();
        let selected = self.selected_idx == Some(idx);
        let is_drag_source = self.drag_source == Some(idx);
        let is_drop_target = self.drop_target == Some(idx);
        let can_drag = self.perspective != Perspective::Flat; // Only allow drag in tree views

        let indent_space = indent as f32 * 20.0;

        ui.horizontal(|ui| {
            ui.add_space(indent_space);

            // Build the label
            let label = format!("{} - {}",
                spec_id.as_deref().unwrap_or("N/A"),
                title
            );

            // Visual feedback for drag/drop state
            let (bg_color, stroke) = if is_drop_target && can_drag {
                (egui::Color32::from_rgba_unmultiplied(100, 200, 100, 60),
                 egui::Stroke::new(2.0, egui::Color32::GREEN))
            } else if is_drag_source {
                (egui::Color32::from_rgba_unmultiplied(100, 100, 200, 60),
                 egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE))
            } else if selected {
                (ui.visuals().selection.bg_fill,
                 egui::Stroke::NONE)
            } else {
                (egui::Color32::TRANSPARENT, egui::Stroke::NONE)
            };

            // Create an interactive area that supports both click and drag
            let sense = if can_drag {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::click()
            };

            // Calculate size for the label
            let text = egui::WidgetText::from(&label);
            let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
            let desired_size = galley.size() + egui::vec2(8.0, 4.0); // padding

            let (rect, response) = ui.allocate_exact_size(desired_size, sense);

            // Paint background
            if bg_color != egui::Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, 2.0, bg_color);
            }
            if stroke != egui::Stroke::NONE {
                ui.painter().rect_stroke(rect, 2.0, stroke);
            }

            // Paint text
            let text_pos = rect.min + egui::vec2(4.0, 2.0);
            let text_color = if selected {
                ui.visuals().selection.stroke.color
            } else {
                ui.visuals().text_color()
            };
            ui.painter().galley(text_pos, galley, text_color);

            // Handle interactions
            if response.clicked() {
                self.selected_idx = Some(idx);
                self.pending_view_change = Some(View::Detail);
            }

            // Drag handling
            if can_drag {
                if response.drag_started() {
                    self.drag_source = Some(idx);
                }

                // Check if this is a drop target using pointer position (more reliable than hovered())
                if self.drag_source.is_some() && self.drag_source != Some(idx) {
                    if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        if rect.contains(pointer_pos) {
                            self.drop_target = Some(idx);
                        }
                    }
                }

                // Release is handled globally in the update loop
            }

            // Show drag indicator while dragging
            if is_drag_source && ui.input(|i| i.pointer.is_decidedly_dragging()) {
                // Show a tooltip-style indicator following the cursor
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    egui::Area::new(egui::Id::new("drag_indicator"))
                        .fixed_pos(pos + egui::vec2(10.0, 10.0))
                        .order(egui::Order::Tooltip)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.label(format!("📎 {}", spec_id.as_deref().unwrap_or("N/A")));
                            });
                        });
                }
            }
        });
    }

    fn show_tree_list(&mut self, ui: &mut egui::Ui) {
        // Get the relationship types for the current perspective
        let Some((outgoing_type, _incoming_type)) = self.perspective.relationship_types() else {
            // Fallback to flat list if no relationship types
            self.show_flat_list(ui);
            return;
        };

        match self.perspective_direction {
            PerspectiveDirection::TopDown => {
                // Find leaves (no outgoing relationships - they are not parents of anything)
                let leaves = self.find_tree_leaves(&outgoing_type);

                if leaves.is_empty() {
                    ui.label("No leaf requirements found for this perspective.");
                    ui.label("(All requirements have outgoing relationships)");
                    ui.add_space(10.0);
                    ui.label("Showing flat list instead:");
                    ui.separator();
                    self.show_flat_list(ui);
                } else {
                    for leaf_idx in leaves {
                        self.show_tree_node_bottom_up(ui, leaf_idx, &outgoing_type, 0);
                    }
                }
            }
            PerspectiveDirection::BottomUp => {
                // Find roots (requirements that are not children of anyone)
                let roots = self.find_tree_roots(&outgoing_type);

                if roots.is_empty() {
                    ui.label("No root requirements found for this perspective.");
                    ui.label("(All requirements have incoming relationships)");
                    ui.add_space(10.0);
                    ui.label("Showing flat list instead:");
                    ui.separator();
                    self.show_flat_list(ui);
                } else {
                    for root_idx in roots {
                        self.show_tree_node(ui, root_idx, &outgoing_type, 0);
                    }
                }
            }
        }
    }

    fn show_tree_node(&mut self, ui: &mut egui::Ui, idx: usize, outgoing_rel_type: &RelationshipType, depth: usize) {
        let Some(req) = self.store.requirements.get(idx) else { return };

        let req_id = req.id;
        let children = self.get_children(&req_id, outgoing_rel_type);
        let has_children = !children.is_empty();

        let is_collapsed = self.tree_collapsed.get(&req_id).copied().unwrap_or(false);

        // Show expand/collapse button and requirement on same line
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 20.0);

            // Expand/collapse button or placeholder of same size
            if has_children {
                let btn_text = if is_collapsed { "+" } else { "-" };
                if ui.small_button(btn_text).clicked() {
                    self.tree_collapsed.insert(req_id, !is_collapsed);
                }
            } else {
                // Use a label with same text to maintain exact same spacing as the expand/collapse button
                let _ = ui.small_button(" ");
            }

            // Show the draggable requirement inline
            self.show_draggable_requirement_inline(ui, idx);
        });

        // Show children if expanded
        if has_children && !is_collapsed {
            for child_idx in children {
                self.show_tree_node(ui, child_idx, outgoing_rel_type, depth + 1);
            }
        }
    }

    /// Render requirement item inline (without indent, for use in tree nodes)
    fn show_draggable_requirement_inline(&mut self, ui: &mut egui::Ui, idx: usize) {
        let Some(req) = self.store.requirements.get(idx) else { return };

        let spec_id = req.spec_id.clone();
        let title = req.title.clone();
        let selected = self.selected_idx == Some(idx);
        let is_drag_source = self.drag_source == Some(idx);
        let is_drop_target = self.drop_target == Some(idx);

        let label = format!("{} - {}",
            spec_id.as_deref().unwrap_or("N/A"),
            title
        );

        // Visual feedback for drag/drop state
        let (bg_color, stroke) = if is_drop_target {
            (egui::Color32::from_rgba_unmultiplied(100, 200, 100, 60),
             egui::Stroke::new(2.0, egui::Color32::GREEN))
        } else if is_drag_source {
            (egui::Color32::from_rgba_unmultiplied(100, 100, 200, 60),
             egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE))
        } else if selected {
            (ui.visuals().selection.bg_fill,
             egui::Stroke::NONE)
        } else {
            (egui::Color32::TRANSPARENT, egui::Stroke::NONE)
        };

        // Calculate size for the label
        let text = egui::WidgetText::from(&label);
        let galley = text.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
        let desired_size = galley.size() + egui::vec2(8.0, 4.0);

        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

        // Paint background
        if bg_color != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, 2.0, bg_color);
        }
        if stroke != egui::Stroke::NONE {
            ui.painter().rect_stroke(rect, 2.0, stroke);
        }

        // Paint text
        let text_pos = rect.min + egui::vec2(4.0, 2.0);
        let text_color = if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        ui.painter().galley(text_pos, galley, text_color);

        // Handle interactions
        if response.clicked() {
            self.selected_idx = Some(idx);
            self.pending_view_change = Some(View::Detail);
        }

        if response.drag_started() {
            self.drag_source = Some(idx);
        }

        // Check if this is a drop target using pointer position
        if self.drag_source.is_some() && self.drag_source != Some(idx) {
            if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pointer_pos) {
                    self.drop_target = Some(idx);
                }
            }
        }

        // Release is handled globally in the update loop

        // Show drag indicator while dragging
        if is_drag_source && ui.input(|i| i.pointer.is_decidedly_dragging()) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                egui::Area::new(egui::Id::new("drag_indicator_inline"))
                    .fixed_pos(pos + egui::vec2(10.0, 10.0))
                    .order(egui::Order::Tooltip)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(format!("📎 {}", spec_id.as_deref().unwrap_or("N/A")));
                        });
                    });
            }
        }
    }

    fn show_tree_node_bottom_up(&mut self, ui: &mut egui::Ui, idx: usize, outgoing_rel_type: &RelationshipType, depth: usize) {
        let Some(req) = self.store.requirements.get(idx) else { return };

        let req_id = req.id;
        let parents = self.get_parents(&req_id, outgoing_rel_type);
        let has_parents = !parents.is_empty();

        let is_collapsed = self.tree_collapsed.get(&req_id).copied().unwrap_or(false);

        // Show expand/collapse button and requirement on same line
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 20.0);

            // Expand/collapse button or placeholder of same size
            if has_parents {
                let btn_text = if is_collapsed { "+" } else { "-" };
                if ui.small_button(btn_text).clicked() {
                    self.tree_collapsed.insert(req_id, !is_collapsed);
                }
            } else {
                // Use a label with same text to maintain exact same spacing as the expand/collapse button
                let _ = ui.small_button(" ");
            }

            // Show the draggable requirement inline
            self.show_draggable_requirement_inline(ui, idx);
        });

        // Show parents if expanded (going up the tree)
        if has_parents && !is_collapsed {
            for parent_idx in parents {
                self.show_tree_node_bottom_up(ui, parent_idx, outgoing_rel_type, depth + 1);
            }
        }
    }

    fn show_detail_view(&mut self, ui: &mut egui::Ui) {
        if let Some(idx) = self.selected_idx {
            if let Some(req) = self.store.requirements.get(idx).cloned() {
                // Buttons need mutable access, so handle them separately
                let mut load_edit = false;
                let mut delete_req = false;
                let mut toggle_archive = false;
                let is_archived = req.archived;

                ui.horizontal(|ui| {
                    ui.heading(&req.title);
                    if is_archived {
                        ui.label("(Archived)");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✏ Edit").clicked() {
                            load_edit = true;
                        }
                        if ui.button("🗑 Delete").clicked() {
                            delete_req = true;
                        }
                        let archive_label = if is_archived { "Unarchive" } else { "Archive" };
                        if ui.button(archive_label).clicked() {
                            toggle_archive = true;
                        }
                    });
                });

                if load_edit {
                    self.load_form_from_requirement(idx);
                    self.pending_view_change = Some(View::Edit);
                }
                if delete_req {
                    self.pending_delete = Some(idx);
                }
                if toggle_archive {
                    self.toggle_archive(idx);
                }

                ui.separator();

                // Metadata grid (always shown)
                egui::Grid::new("detail_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("ID:");
                        ui.label(req.spec_id.as_deref().unwrap_or("N/A"));
                        ui.end_row();

                        ui.label("Status:");
                        ui.label(format!("{:?}", req.status));
                        ui.end_row();

                        ui.label("Priority:");
                        ui.label(format!("{:?}", req.priority));
                        ui.end_row();

                        ui.label("Type:");
                        ui.label(format!("{:?}", req.req_type));
                        ui.end_row();

                        ui.label("Feature:");
                        ui.label(&req.feature);
                        ui.end_row();

                        ui.label("Owner:");
                        ui.label(&req.owner);
                        ui.end_row();

                        if !req.tags.is_empty() {
                            ui.label("Tags:");
                            let tags_vec: Vec<String> = req.tags.iter().cloned().collect();
                            ui.label(tags_vec.join(", "));
                            ui.end_row();
                        }
                    });

                ui.separator();

                // Tabbed content
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_tab, DetailTab::Description, "📄 Description");
                    ui.selectable_value(&mut self.active_tab, DetailTab::Comments, format!("💬 Comments ({})", req.comments.len()));
                    ui.selectable_value(&mut self.active_tab, DetailTab::Links, format!("🔗 Links ({})", req.relationships.len()));
                    ui.selectable_value(&mut self.active_tab, DetailTab::History, format!("📜 History ({})", req.history.len()));
                });

                ui.separator();

                // Tab content
                let req_id = req.id;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match &self.active_tab {
                        DetailTab::Description => {
                            self.show_description_tab(ui, &req);
                        }
                        DetailTab::Comments => {
                            self.show_comments_tab(ui, &req, idx);
                        }
                        DetailTab::Links => {
                            self.show_links_tab(ui, &req, req_id);
                        }
                        DetailTab::History => {
                            self.show_history_tab(ui, &req);
                        }
                    }
                });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("Select a requirement from the list");
            });
        }
    }

    fn show_description_tab(&self, ui: &mut egui::Ui, req: &Requirement) {
        ui.heading("Description");
        ui.add_space(10.0);
        ui.label(&req.description);
    }

    fn show_comments_tab(&mut self, ui: &mut egui::Ui, req: &Requirement, idx: usize) {
        ui.horizontal(|ui| {
            ui.heading("Comments");
            if ui.button("➕ Add Comment").clicked() {
                self.show_add_comment = true;
                self.reply_to_comment = None;
                // Pre-fill author from user settings
                self.comment_author = self.user_settings.display_name();
                self.comment_content.clear();
            }
        });

        ui.add_space(10.0);

        if self.show_add_comment {
            self.show_comment_form(ui, idx);
        }

        if req.comments.is_empty() {
            ui.label("No comments yet");
        } else {
            for comment in &req.comments {
                self.show_comment_tree(ui, comment, idx, 0);
            }
        }
    }

    fn show_links_tab(&mut self, ui: &mut egui::Ui, req: &Requirement, req_id: Uuid) {
        ui.heading("Relationships");
        ui.add_space(10.0);

        if req.relationships.is_empty() {
            ui.label("No relationships defined");
        } else {
            // Collect relationship info first to avoid borrow issues
            let rel_info: Vec<_> = req.relationships.iter().map(|rel| {
                let target_idx = self.store.requirements.iter()
                    .position(|r| r.id == rel.target_id);
                let target_label = self.store.requirements.iter()
                    .find(|r| r.id == rel.target_id)
                    .and_then(|r| r.spec_id.as_ref())
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string());
                let target_title = self.store.requirements.iter()
                    .find(|r| r.id == rel.target_id)
                    .map(|r| r.title.clone())
                    .unwrap_or_else(|| "(not found)".to_string());
                (rel.rel_type.clone(), rel.target_id, target_idx, target_label, target_title)
            }).collect();

            let mut relationship_to_remove: Option<(RelationshipType, Uuid)> = None;

            for (rel_type, target_id, target_idx, target_label, target_title) in rel_info {
                ui.horizontal(|ui| {
                    // Break link button
                    if ui.small_button("x").on_hover_text("Remove relationship").clicked() {
                        relationship_to_remove = Some((rel_type.clone(), target_id));
                    }

                    let label = format!("{:?} {} - {}", rel_type, target_label, target_title);

                    let response = ui.add(
                        egui::Label::new(&label)
                            .sense(egui::Sense::click())
                    );

                    // Show hover cursor and tooltip
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    response.clone().on_hover_text("Double-click to view");

                    // Navigate on double-click
                    if response.double_clicked() {
                        if let Some(idx) = target_idx {
                            self.selected_idx = Some(idx);
                            self.pending_view_change = Some(View::Detail);
                        }
                    }
                });
            }

            // Remove relationship if requested
            if let Some((rel_type, target_id)) = relationship_to_remove {
                let bidirectional = matches!(rel_type, RelationshipType::Parent | RelationshipType::Child | RelationshipType::Verifies | RelationshipType::VerifiedBy);
                if let Err(e) = self.store.remove_relationship(&req_id, &rel_type, &target_id, bidirectional) {
                    self.message = Some((format!("Failed to remove relationship: {}", e), true));
                } else {
                    self.save();
                    self.message = Some(("Relationship removed".to_string(), false));
                }
            }
        }
    }

    fn show_history_tab(&self, ui: &mut egui::Ui, req: &Requirement) {
        ui.heading("Change History");
        ui.add_space(10.0);

        if req.history.is_empty() {
            ui.label("No changes recorded yet");
        } else {
            for entry in req.history.iter().rev() {  // Show newest first
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("🕒 {}", entry.timestamp.format("%Y-%m-%d %H:%M:%S")));
                        ui.label(format!("👤 {}", entry.author));
                    });

                    ui.add_space(5.0);

                    for change in &entry.changes {
                        ui.horizontal(|ui| {
                            ui.label(format!("  📝 {}", change.field_name));
                        });
                        ui.horizontal(|ui| {
                            ui.label("    ❌");
                            ui.colored_label(egui::Color32::from_rgb(200, 100, 100), &change.old_value);
                        });
                        ui.horizontal(|ui| {
                            ui.label("    ✅");
                            ui.colored_label(egui::Color32::from_rgb(100, 200, 100), &change.new_value);
                        });
                    }
                });
                ui.add_space(10.0);
            }
        }
    }

    fn show_form(&mut self, ui: &mut egui::Ui, is_edit: bool) {
        let title = if is_edit { "Edit Requirement" } else { "Add Requirement" };
        ui.heading(title);
        ui.separator();

        // Calculate available width for text fields
        let available_width = ui.available_width();

        // Title field - full width
        ui.label("Title:");
        ui.add(egui::TextEdit::singleline(&mut self.form_title)
            .desired_width(available_width));
        ui.add_space(8.0);

        // Metadata row - Status, Priority, Type, Owner, Feature, Tags
        ui.horizontal_wrapped(|ui| {
            ui.label("Status:");
            egui::ComboBox::new("status_combo", "")
                .selected_text(format!("{:?}", self.form_status))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.form_status, RequirementStatus::Draft, "Draft");
                    ui.selectable_value(&mut self.form_status, RequirementStatus::Approved, "Approved");
                    ui.selectable_value(&mut self.form_status, RequirementStatus::Completed, "Completed");
                    ui.selectable_value(&mut self.form_status, RequirementStatus::Rejected, "Rejected");
                });

            ui.add_space(16.0);
            ui.label("Priority:");
            egui::ComboBox::new("priority_combo", "")
                .selected_text(format!("{:?}", self.form_priority))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.form_priority, RequirementPriority::High, "High");
                    ui.selectable_value(&mut self.form_priority, RequirementPriority::Medium, "Medium");
                    ui.selectable_value(&mut self.form_priority, RequirementPriority::Low, "Low");
                });

            ui.add_space(16.0);
            ui.label("Type:");
            egui::ComboBox::new("type_combo", "")
                .selected_text(format!("{:?}", self.form_type))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.form_type, RequirementType::Functional, "Functional");
                    ui.selectable_value(&mut self.form_type, RequirementType::NonFunctional, "NonFunctional");
                    ui.selectable_value(&mut self.form_type, RequirementType::System, "System");
                    ui.selectable_value(&mut self.form_type, RequirementType::User, "User");
                    ui.selectable_value(&mut self.form_type, RequirementType::ChangeRequest, "Change Request");
                });
        });
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            ui.label("Owner:");
            ui.add(egui::TextEdit::singleline(&mut self.form_owner)
                .desired_width(150.0));

            ui.add_space(16.0);
            ui.label("Feature:");
            ui.add(egui::TextEdit::singleline(&mut self.form_feature)
                .desired_width(150.0));

            ui.add_space(16.0);
            ui.label("Tags:");
            ui.add(egui::TextEdit::singleline(&mut self.form_tags)
                .desired_width(200.0)
                .hint_text("comma-separated"));
        });
        ui.add_space(4.0);

        // Show parent relationship for new requirements (not edit)
        if !is_edit {
            if let Some(parent_id) = self.form_parent_id {
                let parent_info = self.store.requirements.iter()
                    .find(|r| r.id == parent_id)
                    .map(|r| {
                        let spec = r.spec_id.as_deref().unwrap_or("N/A");
                        format!("{} - {}", spec, r.title)
                    });

                if let Some(parent_label) = parent_info {
                    ui.horizontal(|ui| {
                        ui.label("Parent:");
                        ui.label(&parent_label);
                        if ui.small_button("x").on_hover_text("Remove parent").clicked() {
                            self.form_parent_id = None;
                        }
                    });
                }
            }
        }

        ui.add_space(8.0);

        // Description field - full width and takes remaining height
        ui.label("Description:");

        // Calculate remaining height for description (leave space for buttons)
        let remaining_height = ui.available_height() - 50.0;
        let description_height = remaining_height.max(8.0 * self.current_font_size * 1.4); // At least 8 lines

        egui::ScrollArea::vertical()
            .max_height(description_height)
            .show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.form_description)
                    .desired_width(available_width)
                    .desired_rows(8)
                    .hint_text("Enter requirement description..."));
            });

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("💾 Save").clicked() {
                if is_edit {
                    if let Some(idx) = self.selected_idx {
                        self.update_requirement(idx);
                    }
                } else {
                    self.add_requirement();
                }
            }
            if ui.button("❌ Cancel").clicked() {
                self.clear_form();
                self.pending_view_change = Some(if is_edit { View::Detail } else { View::List });
            }
        });
    }

    fn show_comment_form(&mut self, ui: &mut egui::Ui, _req_idx: usize) {
        let available_width = ui.available_width();

        ui.group(|ui| {
            ui.label(if self.reply_to_comment.is_some() { "Add Reply" } else { "Add Comment" });

            ui.horizontal(|ui| {
                ui.label("Author:");
                ui.add(egui::TextEdit::singleline(&mut self.comment_author)
                    .desired_width(200.0));
            });

            ui.label("Content:");
            ui.add(egui::TextEdit::multiline(&mut self.comment_content)
                .desired_width(available_width - 20.0)  // Account for group padding
                .desired_rows(4)
                .hint_text("Enter comment..."));

            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    if !self.comment_author.is_empty() && !self.comment_content.is_empty() {
                        self.pending_comment_add = Some((
                            self.comment_author.clone(),
                            self.comment_content.clone(),
                            self.reply_to_comment,
                        ));
                        self.show_add_comment = false;
                    }
                }
                if ui.button("❌ Cancel").clicked() {
                    self.show_add_comment = false;
                    self.reply_to_comment = None;
                    self.comment_author.clear();
                    self.comment_content.clear();
                }
            });
        });
    }

    fn show_comment_tree(&mut self, ui: &mut egui::Ui, comment: &Comment, req_idx: usize, depth: usize) {
        let indent = depth as f32 * 24.0;
        let is_collapsed = self.collapsed_comments.get(&comment.id).copied().unwrap_or(false);

        ui.horizontal(|ui| {
            // Add horizontal indentation
            if indent > 0.0 {
                ui.add_space(indent);
            }

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    // Collapse/expand button if there are replies
                    if !comment.replies.is_empty() {
                        let button_text = if is_collapsed { "+" } else { "-" };
                        if ui.small_button(button_text).clicked() {
                            self.collapsed_comments.insert(comment.id, !is_collapsed);
                        }
                    } else {
                        ui.add_space(18.0); // Spacing when no collapse button
                    }

                    ui.label(format!("👤 {}", comment.author));
                    ui.label(format!("🕒 {}", comment.created_at.format("%Y-%m-%d %H:%M")));
                });

                ui.label(&comment.content);

                ui.horizontal(|ui| {
                    if ui.small_button("💬 Reply").clicked() {
                        self.show_add_comment = true;
                        self.reply_to_comment = Some(comment.id);
                        // Pre-fill author from user settings
                        self.comment_author = self.user_settings.display_name();
                        self.comment_content.clear();
                    }
                    if ui.small_button("🗑 Delete").clicked() {
                        self.pending_comment_delete = Some(comment.id);
                    }
                });
            });
        });

        // Show replies if not collapsed
        if !is_collapsed {
            for reply in &comment.replies {
                self.show_comment_tree(ui, reply, req_idx, depth + 1);
            }
        }
    }

    fn add_comment_to_requirement(&mut self, idx: usize, author: String, content: String, parent_id: Option<Uuid>) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            if let Some(parent) = parent_id {
                // This is a reply
                let reply = Comment::new_reply(author, content, parent);
                if let Err(e) = req.add_reply(parent, reply) {
                    self.message = Some((format!("Error adding reply: {}", e), true));
                    return;
                }
            } else {
                // This is a top-level comment
                let comment = Comment::new(author, content);
                req.add_comment(comment);
            }

            self.save();
            self.comment_author.clear();
            self.comment_content.clear();
            self.reply_to_comment = None;
            self.message = Some(("Comment added successfully".to_string(), false));
        }
    }

    fn delete_comment_from_requirement(&mut self, idx: usize, comment_id: Uuid) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            if let Err(e) = req.delete_comment(&comment_id) {
                self.message = Some((format!("Error deleting comment: {}", e), true));
                return;
            }
            self.save();
            self.message = Some(("Comment deleted successfully".to_string(), false));
        }
    }
}

impl eframe::App for RequirementsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply current font size to the context
        let mut style = (*ctx.style()).clone();
        for (_text_style, font_id) in style.text_styles.iter_mut() {
            font_id.size = self.current_font_size;
        }
        ctx.set_style(style);

        // Handle keyboard shortcuts for zoom
        let mut zoom_delta: f32 = 0.0;
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            let shift = i.modifiers.shift;

            // Ctrl+Shift+Plus or Ctrl+= to zoom in
            if ctrl && (i.key_pressed(egui::Key::Plus) || (shift && i.key_pressed(egui::Key::Equals))) {
                zoom_delta = 1.0;
            }
            // Ctrl+Minus to zoom out
            if ctrl && i.key_pressed(egui::Key::Minus) {
                zoom_delta = -1.0;
            }
            // Ctrl+0 to reset zoom
            if ctrl && i.key_pressed(egui::Key::Num0) {
                zoom_delta = 0.0;
                self.reset_zoom();
            }

            // Ctrl+MouseWheel to zoom - use raw scroll delta and check for events
            if ctrl {
                // Check for scroll events
                for event in &i.events {
                    if let egui::Event::MouseWheel { delta, .. } = event {
                        if delta.y > 0.0 {
                            zoom_delta = 1.0;
                        } else if delta.y < 0.0 {
                            zoom_delta = -1.0;
                        }
                    }
                }
                // Also check raw scroll delta as fallback
                if zoom_delta == 0.0 && i.raw_scroll_delta.y != 0.0 {
                    if i.raw_scroll_delta.y > 0.0 {
                        zoom_delta = 1.0;
                    } else {
                        zoom_delta = -1.0;
                    }
                }
            }
        });

        // Apply zoom after input closure
        if zoom_delta > 0.0 {
            self.zoom_in();
        } else if zoom_delta < 0.0 {
            self.zoom_out();
        }

        // Handle keyboard navigation in the requirements list
        // Only when in List or Detail view and no text field has focus
        if (self.current_view == View::List || self.current_view == View::Detail)
            && !ctx.wants_keyboard_input()
        {
            let mut nav_delta: i32 = 0;
            let mut enter_pressed = false;
            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) {
                    nav_delta = 1;
                } else if i.key_pressed(egui::Key::ArrowUp) {
                    nav_delta = -1;
                }
                if i.key_pressed(egui::Key::Enter) {
                    enter_pressed = true;
                }
            });

            // Enter key edits the selected requirement
            if enter_pressed {
                if let Some(idx) = self.selected_idx {
                    self.load_form_from_requirement(idx);
                    self.pending_view_change = Some(View::Edit);
                }
            }

            if nav_delta != 0 {
                let filtered_indices = self.get_filtered_indices();
                if !filtered_indices.is_empty() {
                    let new_selection = if let Some(current_idx) = self.selected_idx {
                        // Find current position in filtered list
                        if let Some(pos) = filtered_indices.iter().position(|&idx| idx == current_idx) {
                            // Move up or down within bounds
                            let new_pos = (pos as i32 + nav_delta)
                                .max(0)
                                .min(filtered_indices.len() as i32 - 1) as usize;
                            Some(filtered_indices[new_pos])
                        } else {
                            // Current selection not in filtered list, select first/last
                            if nav_delta > 0 {
                                Some(filtered_indices[0])
                            } else {
                                Some(filtered_indices[filtered_indices.len() - 1])
                            }
                        }
                    } else {
                        // Nothing selected, select first or last based on direction
                        if nav_delta > 0 {
                            Some(filtered_indices[0])
                        } else {
                            Some(filtered_indices[filtered_indices.len() - 1])
                        }
                    };

                    if new_selection != self.selected_idx {
                        self.selected_idx = new_selection;
                        self.pending_view_change = Some(View::Detail);
                    }
                }
            }
        }

        // Handle pending operations (to avoid borrow checker issues)
        if let Some(idx) = self.pending_delete.take() {
            self.delete_requirement(idx);
        }
        if let Some(view) = self.pending_view_change.take() {
            self.current_view = view;
        }
        if let Some((author, content, parent_id)) = self.pending_comment_add.take() {
            if let Some(idx) = self.selected_idx {
                self.add_comment_to_requirement(idx, author, content, parent_id);
            }
        }
        if let Some(comment_id) = self.pending_comment_delete.take() {
            if let Some(idx) = self.selected_idx {
                self.delete_comment_from_requirement(idx, comment_id);
            }
        }
        if let Some((source_idx, target_idx)) = self.pending_relationship.take() {
            self.create_relationship_from_drop(source_idx, target_idx);
        }

        // Handle drag release globally - check if primary button was just released
        let released = ctx.input(|i| i.pointer.primary_released());
        if released && self.drag_source.is_some() {
            if let (Some(source), Some(target)) = (self.drag_source, self.drop_target) {
                if source != target {
                    self.pending_relationship = Some((source, target));
                }
            }
            self.drag_source = None;
            self.drop_target = None;
        }

        // Clear drag state if mouse is not pressed (safety cleanup)
        ctx.input(|i| {
            if !i.pointer.any_down() && self.drag_source.is_some() {
                // Mouse released but we didn't catch it - clear state
            }
        });

        self.show_top_panel(ctx);

        // Only show list panel when not in form view
        if self.current_view == View::List || self.current_view == View::Detail {
            self.show_list_panel(ctx);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match &self.current_view {
                View::List | View::Detail => {
                    self.show_detail_view(ui);
                }
                View::Add => {
                    self.show_form(ui, false);
                }
                View::Edit => {
                    self.show_form(ui, true);
                }
            }
        });

        // Show settings dialog (modal overlay)
        self.show_settings_dialog(ctx);
    }
}
